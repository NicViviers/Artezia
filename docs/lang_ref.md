# Artezia Language Reference — v0.1 (Lexer & Parser Target)

Kotlin/Rust-flavored syntax. This document defines the lexical grammar and syntactic grammar for the v0.1 front end. Semantics noted only where they affect parsing.

---

## 1. Lexical Structure

### 1.1 Source
UTF-8. Newlines are not significant (statements end with `;`-free newline rules? **No** — see 1.7: Artezia is newline-terminated like Kotlin, with continuation rules).

### 1.2 Comments
```
// line comment
/* block comment, /* nests */ still comment */
/// doc comment (attaches to next item)
```

### 1.3 Identifiers
```
ident      = XID_Start XID_Continue*        // Unicode, like Rust
raw_ident  = r#ident                        // escape keywords: r#scope
```

### 1.4 Keywords (reserved)
```
let  mut  func  return  if  else  match  for  while  loop  in  break  continue
import  export  extern  struct  enum  trait  impl  type  pub
scope  nursery  spawn  within  retry  deadline  defer  select
true  false  null  and  or  not  as  is
```
Reserved for future use (lex as keywords, parse error with "reserved" diagnostic):
```
async  await  yield  const  static  unsafe  where  comptime
```

### 1.5 Literals
```
int      = 123 | 1_000_000 | 0xFF | 0o77 | 0b1010          // '_' separators anywhere between digits
float    = 1.5 | 1e9 | 2.5e-3                               // no leading/trailing dot (write 0.5, 1.0)
string   = "hello\n"        // escapes: \n \t \r \\ \" \0 \u{1F600}
         | "count = ${expr}" // interpolation — see 1.6
raw_str  = r"no \escapes" | r#"can contain "quotes""#
char     = 'a' | '\n' | '\u{1F600}'
bool     = true | false
duration = 5s | 100ms | 2m | 1h | 30us | 10ns              // int/float + unit suffix, single token
size     = 4kb | 10mb | 1gb | 512b                          // (lex now, semantics later)
```
Duration/size suffixes are lexed as part of the numeric token (`DurationLit(5, Sec)`). Suffix set: `ns us ms s m h` and `b kb mb gb`. Ambiguity note: `5m` is 5 minutes; there is no bare-meter unit.

### 1.6 String interpolation
`"a ${expr} b"` lexes as: `StrPart("a ")`, `InterpStart`, tokens of `expr`, `InterpEnd`, `StrPart(" b")` — the lexer tracks brace depth inside `${}`. `\$` escapes a literal dollar. (chumsky handles this fine with a mode/stack; alternatively lex the whole string and re-lex the holes in the parser.)

### 1.7 Statement termination (Kotlin-style newlines)
Statements are terminated by newline OR `;`. A newline does NOT terminate when:
- the previous token is one that cannot end an expression: binary operator, `,`, `(`, `[`, `{`, `.`, `=`, `->`, etc.
- the next token is one that cannot start a statement: `)`, `]`, `}`, `.`, binary operators, `else`, `catch`
Practical implementation: the lexer emits `Newline` tokens; the parser skips them in "continuation" positions. Keep this rule table in one place — it is the #1 source of parser bugs in newline-terminated languages.

### 1.8 Operators & punctuation
```
+  -  *  /  %  **                       // ** = power, right-assoc
== != < > <= >=
and or not                              // keyword logical ops (Kotlin-ish); && || ! accepted as aliases
&  |  ^  <<  >>  ~                      // bitwise
=  +=  -=  *=  /=  %=  &=  |=  ^=  <<=  >>=
.  ..  ..=  ->  =>  ::  :  ,  ;  ?  ?:  ?.
( ) [ ] { }  @  #
```
`..` exclusive range, `..=` inclusive. `?:` elvis, `?.` safe-call (Kotlin heritage). `?` postfix error-propagation (Rust heritage). `#` starts attributes and `#dep` script pragmas.

---

## 2. Types (syntax only)

```
Type = Path GenericArgs?                 // Int, String, List<Int>, map::HashMap<K,V>
     | "(" Type ("," Type)* ")"          // tuple: (Int, String); () = unit
     | "[" Type "]"                      // slice/list shorthand: [Int] = List<Int>
     | Type "?"                          // optional: Int?  (sugar for Option<Int>)
     | "func" "(" TypeList? ")" ("->" Type)?    // function type
Path = ident ("::" ident)*
GenericArgs = "<" Type ("," Type)* ">"
```
Built-in type names (not keywords, just well-known): `Int Int8 Int16 Int32 Int64 UInt Float Float32 Float64 Bool String Char Bytes Duration Size Unit Never`.

---

## 3. Declarations

### 3.1 Variables
```
let x = 42                    // immutable, inferred
let x: Int = 42               // annotated
let mut y = 0                 // mutable
let (a, b) = pair()           // destructuring (tuple, struct patterns later)
```

### 3.2 Functions
```
func add(a: Int, b: Int) -> Int {
    return a + b
}

func greet(name: String) {                 // no ->  = Unit
    print("hi ${name}")
}

func last(xs: [Int]) -> Int? = xs.get(xs.len() - 1)    // expression body

pub func api() { }                          // visible outside module
export func c_entry(x: Int32) -> Int32 { }  // exposed with C ABI (extern-visible)
```
- Parameters: `ident ":" Type` with optional default `count: Int = 10`. Named args at call site: `retry(attempts: 3)`.
- Generics: `func map<T, U>(xs: [T], f: func(T) -> U) -> [U]`.
- Effects (parse & store, minimal checking in v0.1): `func fetch(url: String) -> Bytes uses IO`.

### 3.3 Extern (FFI)
```
extern "C" {
    func malloc(size: UInt) -> RawPtr uses Alloc
    func puts(s: CStr) -> Int32 uses IO
}
```

### 3.4 Structs, enums (v0.1 minimal)
```
struct Point { x: Float, y: Float }
struct Point3 { x: Float, y: Float, z: Float = 0.0 }     // field defaults

#[repr(C)]                                                // required for FFI-crossing
struct CPoint { x: Float32, y: Float32 }

enum Shape {
    Circle(radius: Float),
    Rect(w: Float, h: Float),
    Empty,
}
```
Construction: `Point { x: 1.0, y: 2.0 }`, `Shape.Circle(radius: 3.0)`.
Methods (v0.1): `impl Point { func norm(self) -> Float { ... } }`.

### 3.5 Imports
```
import std::io
import std::io::{read, write}
import std::collections as coll
import http                       // resolves via package manager / #dep
```
Script pragma (lexed only in script mode, before first item): `#dep http@1.2`

### 3.6 Type aliases
```
type UserId = Int64
```

---

## 4. Statements & Expressions

Artezia is expression-oriented: `if`, `match`, blocks, `within` are expressions; loops are statements (v0.1).

### 4.1 Blocks
```
let x = {
    let a = compute()
    a * 2                 // last expression = block value (no trailing newline issues: it's the last expr)
}
```

### 4.2 Control flow
```
if cond { a() } else if other { b() } else { c() }
let v = if cond { 1 } else { 2 }                     // expression form requires else

while cond { ... }
loop { ... break ... }                                // infinite
for i in 0..10 { ... }
for item in list { ... }
for (i, x) in list.enumerate() { ... }

match shape {
    Shape.Circle(r) if r > 1.0 -> "big circle"
    Shape.Circle(r)            -> "circle"
    Shape.Rect(w, h)           -> "rect ${w}x${h}"
    _                          -> "other"
}
```
Match arms: `Pattern ("if" Expr)? "->" Expr` newline-or-comma separated. Patterns: literals, `_`, bindings, enum variants, tuples, `a | b` alternatives, ranges `1..=9`.

### 4.3 Error handling (v0.1 direction)
```
func read_config() -> Config throws IoError { ... }   // or Result<Config, IoError> — pick one; parser supports `throws` clause
let c = read_config()?                                 // propagate
let c = read_config() ?: default_config()              // elvis fallback
```

### 4.4 Defer
```
defer file.close()          // runs at scope exit, LIFO
```

### 4.5 Operator precedence (low → high)
```
1.  or  ||
2.  and &&
3.  not !          (prefix)
4.  == != < > <= >= is
5.  ?: (elvis, right-assoc)
6.  .. ..=
7.  | ^ &          (bitwise, in this order: | lowest)
8.  << >>
9.  + -
10. * / %
11. **             (right-assoc)
12. unary - ~ 
13. postfix: () [] . ?. ?  as
```

---

## 5. Concurrency Syntax

```
scope {                                   // structured block: joins all children on exit
    spawn download(url1)                  // fire-and-join-at-scope-end
    let h = spawn compute()               // TaskHandle<T>
    let r = h.await()                     // explicit join (not a keyword — method on handle)
}

nursery {                                 // supervised: children may fail independently
    spawn worker(1)
    spawn worker(2)
} on_error (e) { log.warn("child died: ${e}") }

within 5s {
    slow_call()
} else {
    fallback()
}
// `within Expr Block ("else" Block)?` — Expr must type as Duration; expression-valued

deadline(t) { ... }                       // absolute-time variant

retry(attempts: 3, backoff: exp(100ms, max: 2s)) {
    flaky()
}
// `retry` parses as a call with trailing block (see 6.1) — not a keyword form

select {
    msg = ch1.recv() -> handle(msg)
    ch2.send(x)      -> sent()
    after 1s         -> timeout()
}
```
Parser notes:
- `scope`, `nursery`, `within`, `deadline`, `select`, `spawn`, `defer` are keywords.
- `spawn Expr` — Expr must be a call (enforce in AST validation, not grammar, for better diagnostics).
- `retry` is NOT a keyword: it's a std function taking a trailing closure — which motivates 6.1.

## 6. Closures & Trailing Blocks

```
let f = { x -> x * 2 }                     // Kotlin-style lambda
let g = { (x: Int, y: Int) -> Int in x + y }   // full form (pick one style — see note)
list.map { it * 2 }                        // trailing-lambda call sugar, implicit `it`
retry(attempts: 3) { flaky() }             // zero-arg trailing block
```
**Design decision needed before implementing:** Kotlin's `{ x -> body }` lambda syntax is ambiguous with block expressions in a newline-terminated language. Recommended v0.1 resolution: a `{` directly following a call/identifier in expression position with a `->` after the parameter list is a lambda; a bare `{ ... }` with no `->` is a zero-parameter lambda in argument position and a block elsewhere. Rust-style `|x| x * 2` is the unambiguous fallback if this fights chumsky too hard — decide early, this ripples everywhere.

## 7. Grammar Sketch (EBNF-ish, parser roadmap)

```
File        = ScriptPragma* Item* | Stmt*            // project mode | script mode
Item        = Attr* Vis? (Func | Struct | Enum | Impl | Import | TypeAlias | Extern)
Vis         = "pub"
Attr        = "#[" Meta "]"
Func        = "export"? "func" ident Generics? "(" Params? ")" ("->" Type)? Effects? (Block | "=" Expr)
Effects     = "uses" ident ("," ident)*
Params      = Param ("," Param)*  ;  Param = ident ":" Type ("=" Expr)?
Struct      = "struct" ident Generics? "{" (Field ",")* "}"
Enum        = "enum" ident Generics? "{" (Variant ",")* "}"
Impl        = "impl" Type "{" Func* "}"
Import      = "import" Path ("::" "{" ident+ "}")? ("as" ident)?

Stmt        = Let | Expr | While | For | Loop | Defer | Return | Break | Continue
Let         = "let" "mut"? Pattern (":" Type)? "=" Expr
Expr        = ... per precedence table (4.5), plus:
            | If | Match | Block | Scope | Nursery | Within | Deadline | Select | Spawn | Lambda
Scope       = "scope" Block
Nursery     = "nursery" Block ("on_error" "(" ident ")" Block)?
Within      = "within" Expr Block ("else" Block)?
Spawn       = "spawn" Expr
Select      = "select" "{" SelectArm+ "}"
SelectArm   = (Pattern "=" Expr | Expr | "after" Expr) "->" Expr
```

## 8. Lexer/Parser Implementation Notes (chumsky + ariadne)

1. **Token-first:** lex to a `Vec<(Token, Span)>`, parse over tokens — not chumsky's char-level parsing for the whole grammar. Interpolated strings and newline rules are far easier with a hand-rolled or logos-based lexer feeding chumsky.
2. **Spans everywhere:** every AST node carries a `Span`; ariadne quality depends on it. Make `Spanned<T>` a habit from node one.
3. **Recovery:** use chumsky's error recovery (`recover_with`) at statement and item boundaries so one error doesn't kill the file — multiple diagnostics per run is part of the diagnostics-as-brand goal.
4. **Newline handling:** emit `Newline` tokens; write one `stmt_end` combinator (newline | `;` | before-`}`) and one `nl_cont` skipper for continuation positions. Centralize the 1.7 rule table.
5. **Keep the AST dumb:** pure syntax, no resolved names/types. Validation like "spawn takes a call" happens in a later pass with good diagnostics, not in the grammar.
6. **Test files first:** every construct in this document should exist as a `tests/parse/ok_*.tia` and several `err_*.tia` files before or alongside its implementation.
```