use std::path::Path;
use arteziac::{analysis::analyze, loader, lower::lower, codegen::Codegen};
use artezia_diag as diag;
use inkwell::{OptimizationLevel, context::Context, execution_engine::JitFunction, passes::PassBuilderOptions, targets::{InitializationConfig, Target, TargetMachine}};

fn to_opt(opt: &String) -> OptimizationLevel {
    match opt.as_str() {
        "O0" => OptimizationLevel::None,
        "O1" => OptimizationLevel::Less,
        "O2" => OptimizationLevel::Default,
        "O3" => OptimizationLevel::Aggressive,
        _ => unreachable!()
    }
}

pub fn execute(entry: &Path, opt: &String) {
    let loaded = loader::load(entry, Path::new("/home/nicholas/Projects/Artezia/stdlib"));
    if diag::report_all(&loaded.diags, &loaded.map, false) > 0 {
        return;
    }

    let (a, adiags) = analyze(&loaded.file, &loaded.map.text);
    if diag::report_all(&adiags, &loaded.map, false) > 0 {
        return;
    }

    let program = lower(&loaded.file, &a);

    let ctx = Context::create();
    let module = Codegen::new(&ctx, &a).run(&program);
    module.verify().expect("LLVM verification failed");

    Target::initialize_native(&InitializationConfig::default())
        .expect("failed to initialize native target");

    // Optimization pass
    let llvm_opt = to_opt(opt);
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).unwrap();
    let machine = target
        .create_target_machine(
            &triple,
            &TargetMachine::get_host_cpu_name().to_string(),
            &TargetMachine::get_host_cpu_features().to_string(),
            llvm_opt,
            inkwell::targets::RelocMode::Default,
            inkwell::targets::CodeModel::Default
        ).unwrap();
    module
        .run_passes(
            format!("default<{}>", opt.as_str()).as_str(),
            &machine,
            PassBuilderOptions::create()
        )
        .expect("passes failed");
    println!("--- IR: ---\n{}\n\n", module.print_to_string().to_string());

    let ee = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("failed to create JIT");

    unsafe {
        let f: JitFunction<unsafe extern "C" fn() -> i64> = ee.get_function("main").expect("no `main` in module");
        println!("main() output: {}", f.call());
    }
}