use std::fs;
use std::path::{Path, PathBuf};

use arteziac::analysis::analyze;
use arteziac::{codegen::Codegen, loader, lower::lower, };
use inkwell::context::Context;
use inkwell::execution_engine::JitFunction;
use inkwell::targets::{InitializationConfig, Target};
use inkwell::OptimizationLevel;

/// A throwaway directory of .tia files, cleaned up on drop.
struct TempProject {
    dir: PathBuf,
}

impl TempProject {
    fn new(tag: &str, files: &[(&str, &str)]) -> Self {
        let dir = std::env::temp_dir().join(format!("artezia_test_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");

        for (name, src) in files {
            let path = dir.join(format!("{name}.tia"));
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create subdir");
            }
            fs::write(&path, src).expect("write temp file");
        }
        TempProject { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.tia"))
    }

    fn dir(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Load only - returns (merged item count, rendered-ish diagnostic messages)
fn load_project(entry: &Path, stdlib: &Path) -> (usize, Vec<String>) {
    let loaded = loader::load(entry, stdlib);
    let msgs = loaded
        .diags
        .iter()
        .map(|d| d.message.clone())
        .collect::<Vec<_>>();
    (loaded.file.items.len(), msgs)
}

/// Full pipeline + JIT. Panics with the diagnostics if anything fails
fn run_project(entry: &Path, stdlib: &Path) -> i64 {
    let loaded = loader::load(entry, stdlib);
    assert!(
        loaded.diags.is_empty(),
        "load errors: {:?}",
        loaded.diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );

    let (a, adiags) = analyze(&loaded.file, &loaded.map.text);
    assert!(
        adiags.is_empty(),
        "analysis errors: {:?}",
        adiags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );

    let program = lower(&loaded.file, &a);

    Target::initialize_native(&InitializationConfig::default())
        .expect("init native target");

    let ctx = Context::create();
    let module = Codegen::new(&ctx, &a).run(&program);
    module.verify().expect("LLVM verify failed");

    let ee = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("create JIT");

    unsafe {
        let f: JitFunction<unsafe extern "C" fn() -> i64> =
            ee.get_function("main").expect("no `main`");
        f.call()
    }
}

#[test]
fn loads_two_files_via_import() {
    let p = TempProject::new("two_files", &[
        ("util", "func double(x: Int) -> Int { return x * 2; }"),
        ("main", "import util;\nfunc main() -> Int { return double(21); }"),
    ]);

    let (item_count, diags) = load_project(&p.path("main"), p.dir());
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    // main.tia: import + func main (2)
    // util.tia: func double (1)
    assert_eq!(item_count, 3);
}

#[test]
fn import_cycle_terminates() {
    // a imports b, b imports a. The visited-set must break the cycle
    // In a flat namespace a cycle is legal - nothing to reject
    let p = TempProject::new("cycle", &[
        ("a", "import b;\nfunc fa() -> Int { return 1; }"),
        ("b", "import a;\nfunc fb() -> Int { return 2; }"),
    ]);

    let (item_count, diags) = load_project(&p.path("a"), p.dir());
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    // a: import + fa (2), b: import + fb (2)
    assert_eq!(item_count, 4);
}

#[test]
fn diamond_import_loads_shared_file_once() {
    // a -> b, a -> c, b -> d, c -> d. `d` must appear exactly once, or its items get declared twice and resolve reports "already defined"
    let p = TempProject::new("diamond", &[
        ("d", "func shared() -> Int { return 7; }"),
        ("b", "import d;\nfunc fb() -> Int { return shared(); }"),
        ("c", "import d;\nfunc fc() -> Int { return shared(); }"),
        ("a", "import b;\nimport c;\nfunc main() -> Int { return fb() + fc(); }"),
    ]);

    let (_, diags) = load_project(&p.path("a"), p.dir());
    assert!(
        diags.is_empty(),
        "diamond import produced diagnostics (d loaded twice?): {diags:?}"
    );

    // and it actually runs: 7 + 7
    assert_eq!(run_project(&p.path("a"), p.dir()), 14);
}

#[test]
fn missing_import_reports_one_diagnostic() {
    let p = TempProject::new("missing", &[
        ("main", "import nope;\nfunc main() -> Int { return 1; }"),
    ]);

    let (_, diags) = load_project(&p.path("main"), p.dir());
    assert_eq!(diags.len(), 1, "expected exactly one diagnostic: {diags:?}");
    assert!(
        diags[0].contains("nope"),
        "diagnostic should name the missing module: {}",
        diags[0]
    );
}

#[test]
fn cross_file_function_call_runs() {
    let p = TempProject::new("e2e", &[
        ("util", "func double(x: Int) -> Int { return x * 2; }"),
        ("main", "import util;\nfunc main() -> Int { return double(21); }"),
    ]);

    // Proves: spans offset correctly, NodeIds don't collide, resolve sees both files' items, codegen emits both functions, the call links
    assert_eq!(run_project(&p.path("main"), p.dir()), 42);
}

#[test]
fn cross_file_struct_and_methods_run() {
    // Everything here is DefId/NodeId keyed (struct_names, struct_infos, methods, field_indices) - so this fails loudly if id continuity broke
    let p = TempProject::new("shapes", &[
        ("shapes", "struct P { x: Int, y: Int }\n\
                    func P.new(x: Int, y: Int) -> P { return P { x: x, y: y }; }\n\
                    func P.sum(self) -> Int { return self.x + self.y; }"),
        ("main", "import shapes;\n\
                  func main() -> Int { let p = P.new(1, 2); return p.sum(); }"),
    ]);

    assert_eq!(run_project(&p.path("main"), p.dir()), 3);
}

#[test]
fn cross_file_mut_method_runs() {
    // mut self across a file boundary: pointer receiver, field store
    let p = TempProject::new("counter", &[
        ("counter", "struct C { n: Int }\n\
                     func C.new() -> C { return C { n: 0 }; }\n\
                     func C.inc(mut self) { self.n = self.n + 1; }\n\
                     func C.get(self) -> Int { return self.n; }"),
        ("main", "import counter;\n\
                  func main() -> Int { let c = C.new(); c.inc(); c.inc(); return c.get(); }"),
    ]);

    assert_eq!(run_project(&p.path("main"), p.dir()), 2);
}

#[test]
fn duplicate_name_across_files_is_an_error() {
    // TODO: In a flat namespace this is a genuine redefinition. THIS TEST SHOULD CHANGE when per-file scopes make it legal!!
    let p = TempProject::new("dupe", &[
        ("a", "func helper() -> Int { return 1; }"),
        ("main", "import a;\nfunc helper() -> Int { return 2; }\n\
                  func main() -> Int { return helper(); }"),
    ]);

    let loaded = loader::load(&p.path("main"), p.dir());
    assert!(loaded.diags.is_empty(), "loading itself should succeed");

    let (_a, adiags) = analyze(&loaded.file, &loaded.map.text);
    assert_eq!(
        adiags.len(), 1,
        "expected exactly one redefinition diagnostic: {:?}",
        adiags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(adiags[0].message.contains("already defined"));
}