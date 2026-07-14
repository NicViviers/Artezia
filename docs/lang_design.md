# Artezia (`tia`) — Design Document v0.2

> **Thesis:** A compiled, LLVM-backed language that feels like a scripting language, with structured concurrency built into the grammar — spawn a million tasks, never color a function, and when things go slow or wrong, the toolchain explains itself.

---

## 1. Identity

- **Name:** Artezia. CLI + extension: `tia` / `.tia`. Convention: `artezia_*` crates = core functionality, `tia*` = tooling.
- **Positioning:** general-purpose, server/compute-leaning. Kotlin's coroutine ergonomics + Go's colorless runtime + scripting startup + Rust-quality diagnostics.
- **Non-goals (v0.x):** implicit auto-parallelization (TLS moonshot — later), GPU backends, distribution, interpreter tier. Design keeps the doors open (§7); nothing more.

## 2. Feature Set

### 2.1 Structured concurrency (language-level, colorless)
- `scope { spawn f() }` — tasks owned by scopes; block exits when all children finish; a child error cancels siblings and rethrows. No free-floating `go` — detached work requires passing an explicit long-lived scope object (visible in signatures).
- `nursery { }` — supervision variant: independent child failures, collected errors, restart policies.
- No `async`/`suspend` coloring: every function can block; the runtime makes it cheap.
- Channels + `select` in std (not syntax) for v0.

### 2.2 Time & resilience as syntax
```tia
within 5s { call_backend() } else { return Fallback.stale_cache() }
retry(attempts: 3, backoff: exp(100ms, max: 2s)) { flaky_rpc() }
deadline(t) { ... }
```
Thin desugarings over scopes: `within` = scope + timer + subtree cancellation (correct cleanup by construction). `retry` requires an idempotent-safe (`retryable`) block — enforced as a diagnostic. Composable: `retry(3) { within 2s { ... } }`.

### 2.3 First-class scripting (JIT-powered, no interpreter)
- `tia run script.tia` — analysis runs eagerly (all type errors immediately), codegen is **lazy** (ORC stubs; functions compile at `-O0` on first call). `main` compiles in milliseconds; startup stays snappy.
- Shebang `#!/usr/bin/env tia`; inline deps `#dep http@1.2` resolved to a content-addressed cache; `tia freeze` pins.
- Top-level statements in script mode; `main` in project mode (`tia.toml`).
- `tia build script.tia` promotes any script to a static native binary — one mental model, two execution modes.

### 2.4 REPL & live introspection
- `tia repl` — each snippet compiles as a mini-module into the persistent ORC session (Julia/clang-repl model). `:type`, `:doc`, `:time`.
- `:tasks` / `tia attach <pid>` — live scope tree of any running Artezia process: every task, its scope path, state (running / blocked-on-channel / sleeping / in-FFI), time consumed. A viewer over data the scheduler already owns.
- `tia explain` — what the JIT decided: tier-ups, inlining, yield-check placement.

### 2.5 Diagnostics as the brand
- ariadne end to end; every error = what/why/fix; machine-applicable fixes via `tia fix`. Wired from milestone 0 — a habit, not a feature.

### 2.6 Painless FFI
- `extern "C"` both directions (`export fn` for embedding Artezia in C programs).
- `tia bind header.h` — libclang-based binding generator producing typed externs.
- Artezia layout is unspecified by default; FFI-crossing types require `#[repr(C)]`, compiler-enforced with fix-it diagnostics.
- FFI calls are safepoint-bookended; long C calls migrate off the scheduler thread (Go's syscall-promotion trick) so the pool never starves.
- Externs carry coarse effect annotations (`uses IO`) — cheap now, load-bearing later.

### 2.7 Toolchain (one binary)
`tia run | build | repl | test | fmt | fix | bind | attach | explain | doc`.
Highlight: `tia test --shake` — run each test under N seeded random scheduler interleavings. Reproducible race hunting; small work on top of the scheduler, genuine differentiator.

## 3. Architecture: One Compiler, One Runtime, Two Last Miles

```
source → parse → typecheck → TIR passes (yield points, safepoints, effects)
       → LLVM module(s)                          [arteziac stops here]
              ├── tia_jit: lazy ORC, -O0 first-call, tiering to -O2   → `tia run` / repl
              └── AOT: full -O2, object files, link artezia_rt.a + entry stub → `tia build`
```

- Emitted IR is **identical in both modes**: same safepoint polls, same calls to the runtime's C ABI (`tia_rt_spawn`, `tia_rt_yield`, `tia_rt_alloc`, ...). Runtime symbols are unresolved externals; JIT resolves them to the runtime linked into the `tia` process, AOT links statically.
- **Semantics belong to the language, not the mode.** Explicit TIR rules for anything modes would otherwise disagree on — e.g. module-level `let`s initialize eagerly at startup in declaration order, both modes. This is what keeps `--shake` results and bug reports transferable between run/build.

### 3.1 arteziac's public surface
```rust
pub struct CompileSession { /* typechecked program, TIR cache */ }
impl CompileSession {
    fn analyze(&mut self, src) -> Result<(), Diagnostics>;   // eager: parse + typecheck ALL
    fn codegen_unit(&self, funcs: &[FuncId], opt: OptLevel) -> LlvmModule;  // JIT, on demand
    fn all_units(&self, opt: OptLevel) -> Vec<LlvmModule>;                  // AOT
}
```
Analysis is always eager and complete; only *codegen* is lazy. Module/Context ownership stays on the consumer side (or pass bitcode — simpler v0, sidesteps LLVM context-threading).

## 4. Workspace Structure

```
artezia/
├── crates/
│   ├── arteziac/        # compiler lib: lexer, parser, AST, spans, typeck, TIR, LLVM codegen
│   ├── artezia_rt/      # runtime: scheduler, tasks, scopes, channels, GC, safepoints
│   ├── artezia_std/     # stdlib: .tia sources + Rust native shims into artezia_rt
│   ├── artezia_diag/    # Diagnostic type, error-code registry, fix-it format (bottom of graph)
│   ├── tia_jit/         # ORC session, lazy stubs, tiering policy, profiling counters
│   └── tia/             # CLI: arg parsing + orchestration only; AOT link step lives here for now
└── tests/               # end-to-end: .tia files → expected stdout/diagnostics (snapshot-style)
```

Dependency rules (enforce ruthlessly):
```
tia → arteziac, tia_jit, artezia_rt
tia_jit → arteziac, artezia_rt
arteziac → artezia_diag
artezia_rt → artezia_diag ONLY        # runtime never knows the compiler exists
artezia_std → artezia_rt (shims)
```
- No interpreter crate, no lli: `tia run` = tia_jit with lazy ORC at `-O0`.
- std is a sandwich: Rust for machine/runtime-touching code (I/O, channel internals, hashing, time — registered as natives with artezia_rt), Artezia source for everything expressible (collections algorithms, strings, formatting). Early ratio will be Rust-heavy; migrate down over time. Discipline: native fns declare signatures + effects in `.tia` interface files like ordinary FFI — std gets no semantic backdoor.
- std ships embedded in the `tia` binary (include_str!/precompiled TIR) so `tia run` needs zero external files.

## 5. artezia_rt — Implementation Guide

The runtime is the language's identity. Build it in this order:

### 5.1 Tasks & scheduler
- **Green tasks:** heap-allocated, small growable stacks (start ~4–8 KB, segmented or copied-on-grow). Task struct: stack, saved registers/context, state, scope pointer, budget counter, task-locals.
- **M:N scheduler:** one worker OS-thread per core; per-worker Chase-Lev work-stealing deque (`crossbeam-deque` implements it) + one global injector queue. Steal when local deque empties. Park idle workers on a futex/condvar; wake on injection.
- **Context switch:** hand-rolled asm or `corosensei`-style switching; a yield saves callee-saved regs + SP, jumps to scheduler. Target < ~50ns.

### 5.2 Preemption (the fairness guarantee)
- **Yield points:** arteziac's TIR pass inserts a check at every loop back-edge and function entry: load a per-task flag/budget, conditionally call `tia_rt_yield`. Predictable-branch cost ≈ free; strip-mine hot inner loops (check every N iterations) when profiling justifies.
- **Budget:** per-task counter (reductions or time slice); scheduler tick (a timer thread) sets the preempt flag on tasks exceeding their slice.
- **Signal backstop:** a monitor thread watches per-worker "last yield" timestamps; a worker stuck >10ms gets `SIGURG`; the handler (async-signal-safe, minimal) marks the task and reschedules at the interrupted point. Requires native frames → lands with the JIT milestone. FFI is excluded (see 5.5).

### 5.3 Scopes, cancellation, supervision
- **Scope tree:** every task holds a parent-scope pointer; scopes hold child lists. This one structure powers join semantics, cancellation, `:tasks`, supervision, and (later) replay.
- **Cancellation:** setting a scope cancelled flips a flag on the whole subtree; tasks observe it at yield points (guaranteed observed — a cancelled task cannot spin forever). Blocked tasks (channel/sleep/IO) are woken with a Cancelled result. Cleanup = normal unwinding through `defer`/drop.
- **within/retry desugar here:** `within` spawns the body in a child scope plus a timer task racing it; loser's subtree is cancelled. No special runtime support beyond scopes + timers.

### 5.4 Blocking, I/O, timers
- All blocking goes through the runtime: channels, sleeps, and I/O park the *task*, never the worker thread.
- v0 I/O: a reactor thread with epoll/kqueue (or `mio`), parking tasks on readiness. Design the interface completion-shaped so an io_uring backend can slot in later without semantic change.
- Timer wheel or binary heap on the reactor for sleeps/deadlines (feeds `within`).

### 5.5 FFI discipline
- Entering an extern call: mark the worker "in FFI." If it stays there past a threshold, the monitor thread hands the worker's run-queue to a freshly spun (or pooled) worker — the Go syscall trick. Returning from FFI re-enters through a safepoint.

### 5.6 Memory
- v0 GC: precise, non-moving (mark-sweep) — keeps FFI and the signal backstop simple. Safepoints + stack maps from day one (the JIT emits stack maps; they're required for precise scanning and are the same machinery preemption uses). Moving/generational GC is a v2 upgrade the design explicitly permits.
- Allocation via `tia_rt_alloc`; per-worker thread-local allocation buffers to avoid contention.

### 5.7 C ABI surface
Everything compiled code needs, stable and small: `tia_rt_spawn`, `tia_rt_yield`, `tia_rt_scope_enter/exit`, `tia_rt_alloc`, `tia_rt_chan_*`, `tia_rt_sleep`, `tia_rt_panic`. JIT resolves these in-process; AOT links `artezia_rt.a`. The runtime is also directly unit-testable from Rust through this surface before the language can even express concurrency.

## 6. Build Order

- **M0 — front end:** chumsky lexer/parser, AST, typecheck skeleton, ariadne diagnostics wired from the first error. `tests/` snapshot harness up.
- **M1 — execution:** TIR + LLVM codegen for a minimal language subset; tia_jit lazy ORC; `tia run` works (no concurrency yet). *A usable scripting language.*
- **M2 — the runtime:** artezia_rt per §5.1–5.4; yield-point TIR pass; `scope`/`spawn`/cancellation; then `within`/`retry` as desugarings. *The identity exists.*
- **M3 — AOT:** `-O2` pipeline, object emission, link with artezia_rt.a + entry stub → `tia build`. Signal backstop lands here.
- **M4 — tooling teeth:** `tia fmt`, `test` blocks, `--shake`, REPL over the ORC session, `:tasks`/`attach`.
- **M5 — FFI + polish:** `extern "C"`, `tia bind`, first `tia explain`.

(M1 before M2 — inverted from v0.1 of this doc — because without an interpreter, the JIT *is* the only way to run anything, so it must exist before the runtime has a user.)

## 7. Load-Bearing Walls (decide now, build later)

1. **Layout opacity by default** → future profile-guided layout optimization, adaptive data structures.
2. **Coarse effect tracking** (pure / mem / IO) → future TLS speculation, deterministic replay, `retry` safety, memoization.
3. **Safepoints + stack maps everywhere** → preemption now; moving GC, snapshots, rollback later.
4. **All nondeterminism through the runtime** (time, RNG, scheduling, I/O) → replay and `--shake` stay possible.
5. **Scheduler owns a first-class task tree** → observability, supervision, replay read one structure.
6. **Mode-independent semantics:** any behavior JIT and AOT could disagree on gets an explicit TIR-level rule.