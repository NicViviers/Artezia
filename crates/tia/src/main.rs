use clap::{Arg, Command};

// TODO: Print compiler diagnostics nicely using ariadne

fn main() {
    let matches = Command::new("tia")
        .version("0.1.0")
        .about("Artezia compiler CLI tool")
        .subcommand(
            Command::new("run")
            .arg(Arg::new("file").required(true))
            .arg(Arg::new("opt").long("opt-level").value_parser(["O0", "O1", "O2", "O3"]).default_value("O2"))
        )
        .subcommand(
            Command::new("build")
            .arg(Arg::new("file").required(true))
        ).get_matches();

    if let Some(sub) = matches.subcommand_matches("run") {
        let file: &String = sub.get_one("file").unwrap();
        let opt: &String = sub.get_one("opt").unwrap();

        if let Ok(source) = std::fs::read_to_string(file) {
            println!("{}", tia_jit::run(&source));
        }
    }
}
