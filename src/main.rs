extern crate clap;
extern crate cton2llvm;
extern crate inkwell;

use inkwell::targets::{InitializationConfig, Target};

use cton2llvm::{parse_cton_file, translate};

use clap::{App, Arg, ArgMatches};
use std::path::PathBuf;

fn main() {
    let o = Options::get().expect("cmdline args");

    println!("Going to compile {:?} into {:?}.", o.input, o.output);
    setup_inkwell().expect("inkwell initialized");

    let cton_module = parse_cton_file(o.input).expect("parsed cton file");
    let _llvm_module = translate(cton_module).expect("translated llvm module");

}

fn setup_inkwell() -> Result<(), String> {
    Target::initialize_native(&InitializationConfig::default())?;
    Ok(())
}

pub struct Options {
    pub input: PathBuf,
    pub output: PathBuf,
}

impl Options {
    pub fn from_args(m: &ArgMatches) -> Result<Self, String> {
        let input = PathBuf::from(m.value_of("input").expect("input is a required arg"));
        if !input.exists() {
            return Err(format!("input file does not exist"));
        }

        let output = PathBuf::from(m.value_of("output").expect("output is a required arg"));

        Ok(Options {
            input: input,
            output: output,
        })
    }

    pub fn get() -> Result<Self, String> {
        let args = App::new("cton2llvm")
            .arg(
                Arg::with_name("output")
                    .short("o")
                    .takes_value(true)
                    .multiple(false)
                    .required(true)
                    .help("Output LLVM IR file"),
            )
            .arg(
                Arg::with_name("input")
                    .takes_value(true)
                    .multiple(false)
                    .required(true)
                    .help("Input Cretonne IL file"),
            )
            .get_matches();
        Self::from_args(&args)
    }
}
