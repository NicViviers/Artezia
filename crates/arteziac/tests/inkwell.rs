use inkwell::context::Context;

#[test]
fn inkwell_smoke() {
    let ctx = Context::create();
    let module = ctx.create_module("smoke");
    let builder = ctx.create_builder();

    // define: i64 answer() { ret i64 42 }
    let i64t = ctx.i64_type();
    let fn_type = i64t.fn_type(&[], false);
    let function = module.add_function("answer", fn_type, None);
    let entry = ctx.append_basic_block(function, "entry");
    builder.position_at_end(entry);
    builder.build_return(Some(&i64t.const_int(42, false))).unwrap();

    let ir = module.print_to_string().to_string();
    println!("{ir}");
    assert!(module.verify().is_ok(), "module failed verification");
}