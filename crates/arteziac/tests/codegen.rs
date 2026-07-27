use arteziac::{analysis::analyze, codegen::Codegen, lexer::lex, lower::lower, parser::Parser};
use inkwell::context::Context;

fn check(src: &str) -> String {
    let tokens = lex(src);
    let (file, pdiags) = Parser::new(tokens).parse_file();
    assert!(pdiags.is_empty(), "parse errors: {pdiags:?}");
    let (a, adiags) = analyze(&file, src);
    assert!(adiags.is_empty(), "analysis errors: {adiags:?}");
    let program = lower(&file, &a);

    let ctx = Context::create();
    let module = Codegen::new(&ctx, &a).run(&program);

    // LLVM verifier. A translation bug surfaces here as a readable message
    if let Err(e) = module.verify() {
        panic!("LLVM verification failed:\n{}", e.to_string());
    }

    module.print_to_string().to_string() // owned String before ctx drops
}

#[test]
fn cg_const_return() {
    insta::assert_snapshot!(check("func answer() -> Int { return 42; }"));
}

#[test]
fn cg_struct() {
    insta::assert_snapshot!(check("struct Foo { bar: Int } func main() -> Int { let f = Foo { bar: 5 }; return f.bar; }"));
}

// Test order -- simplest outward, same as lowering:

// arithmetic - 1 + 2 * 3 - the Binary arms
// locals - let x = 1; return x; - allocas, load, store
// params - func add(a, b) { return a + b; } - param stores
// if-diamond - confirms two-pass block creation (branches to later blocks)
// while/for - back-edges, the loop CFG in LLVM
// calls - cross-function func_vals lookup

// Reviewing .ll snapshots: unlike TIR (which is designed the format of), LLVM's IR format is fixed, so we are reading LLVM's output.
// What to check: the function signature matches (define i64 @answer()), the allocas are in the entry block, loads/stores wire to the right slots,
// br/ret terminators are present, and - critically - it verifies (the harness asserts this). You don't need to hand-trace every SSA value the way we did TIR;
// the verifier catches structural errors, and we are mostly confirming the shape is sane (right number of blocks, branches go where TIR said).
// The .ll is more "does this look like reasonable IR" than "is every value correct" - because the verifier already guarantees validity,
// and the TIR snapshots already guaranteed the logic.

// One optional-but-recommended step: after the IR verifies, run the optimization passes and snapshot the optimized IR too,
// to confirm mem2reg actually promotes the allocas to SSA (the payoff of the whole memory-based design).
// But that's a nice-to-have; get unoptimized IR verifying and snapshotted first.