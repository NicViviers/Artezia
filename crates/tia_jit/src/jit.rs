use arteziac::{analysis::analyze, codegen::Codegen, lexer::lex, lower::lower, parser::Parser};
use inkwell::{OptimizationLevel, context::Context, execution_engine::JitFunction, module::Module, passes::PassBuilderOptions, targets::{InitializationConfig, Target, TargetMachine}};

fn dump_opt_code<'ctx>(module: Module<'ctx>) -> String {
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).unwrap();
    let machine = target
        .create_target_machine(
            &triple,
            &TargetMachine::get_host_cpu_name().to_string(),
            &TargetMachine::get_host_cpu_features().to_string(),
            OptimizationLevel::Aggressive,
            inkwell::targets::RelocMode::Default,
            inkwell::targets::CodeModel::Default
        ).unwrap();

    module
        .run_passes("default<O2>", &machine, PassBuilderOptions::create())
        .expect("passes failed");

    module.print_to_string().to_string()
}

fn run(src: &str) -> i64 {
    let tokens = lex(src);
    let (file, pdiags) = Parser::new(tokens).parse_file();
    assert!(pdiags.is_empty(), "parse errors: {pdiags:?}");

    let (a, adiags) = analyze(&file, src);
    assert!(adiags.is_empty(), "analysis errors: {adiags:?}");

    let program = lower(&file, &a);

    let ctx = Context::create();
    let module = Codegen::new(&ctx, &a).run(&program);
    module.verify().expect("LLVM verification failed");

    println!("Pre-passes:\n{}", module.print_to_string().to_string());
    println!("\n\nPost-passes:\n{}", dump_opt_code(module.clone()));

    Target::initialize_native(&InitializationConfig::default())
        .expect("failed to initialize native target");

    let ee = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT");

    unsafe {
        let f: JitFunction<unsafe extern "C" fn() -> i64> = ee.get_function("main").expect("no `main` in module");
        f.call()
    }
}

// TODO: Migrate or add some arteziac tests that use JIT to physically confirm functionality
// TODO: Opt dump doesn't work - maybe because of WSL?

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn run_counter() {
        assert_eq!(run(
            "struct C { n: Int } \
            func C.new() -> C { return C { n: 0 }; } \
            func C.get(self) -> Int { return self.n; } \
            func C.inc(mut self) { self.n = self.n + 1; } \
            func main() -> Int { let c = C.new(); c.inc(); c.inc(); return c.get(); }"
        ), 2);
    }
}