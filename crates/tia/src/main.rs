use clap::{Arg, Command};

fn main() {
    let matches = Command::new("tia")
        .version("0.1.0")
        .about("Artezia compiler CLI tool")
        .subcommand(
            Command::new("run")
            .arg(Arg::new("file").required(true))
            .arg(Arg::new("opt").long("opt").value_parser(["O0", "O1", "O2", "O3"]).default_value("O2"))
        )
        .subcommand(
            Command::new("build")
            .arg(Arg::new("file").required(true))
        ).get_matches();

    if let Some((s, sub)) = matches.subcommand() {
        match s {
            "run" => {
                let file: &String = sub.get_one("file").unwrap();
                let opt: &String = sub.get_one("opt").unwrap();
                tia_jit::execute(std::path::Path::new(file), opt);
            }

            "build" => unimplemented!(),
            _ => unimplemented!()
        }
    }
}
