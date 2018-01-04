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
    let llvm_module = translate(cton_module).expect("translated llvm module");

    if o.print_debug {
        llvm_module.print_to_stderr();
    }

    match o.emit {
        OutputEmit::IR => llvm_module.print_to_file(&o.output),
        OutputEmit::Bitcode => {
            if llvm_module.write_bitcode_to_path(&o.output) {
                Ok(())
            } else {
                Err(String::from("failed to write bitcode to file"))
            }
        }
    }.expect("write to file");
}

fn setup_inkwell() -> Result<(), String> {
    Target::initialize_native(&InitializationConfig::default())?;
    Ok(())
}

struct Options {
    pub input: PathBuf,
    pub output: PathBuf,
    pub emit: OutputEmit,
    pub print_debug: bool,
}

enum OutputEmit {
    IR,
    Bitcode,
}

impl Options {
    pub fn from_args(m: &ArgMatches) -> Result<Self, String> {
        let input = PathBuf::from(m.value_of("input").expect("input is a required arg"));
        if !input.exists() {
            return Err(format!("input file does not exist"));
        }

        let output = PathBuf::from(m.value_of("output").expect("output is a required arg"));

        let emit = match m.value_of("emit") {
            Some("ir") => OutputEmit::IR,
            Some("bitcode") => OutputEmit::Bitcode,
            _ => panic!("unreachable"),
        };

        Ok(Options {
            input: input,
            output: output,
            emit: emit,
            print_debug: m.is_present("print-debug"),
        })
    }

    pub fn get() -> Result<Self, String> {
        let args = App::new("cton2llvm")
            .arg(
                Arg::with_name("emit")
                    .long("emit")
                    .takes_value(true)
                    .multiple(false)
                    .required(false)
                    .possible_values(&["bitcode", "ir"])
                    .default_value("bitcode")
                    .help("Format to output"),
            )
            .arg(
                Arg::with_name("print-debug")
                    .long("print-debug")
                    .takes_value(false)
                    .multiple(false)
                    .required(false)
                    .help("Print IR to stderr"),
            )
            .arg(
                Arg::with_name("output")
                    .short("o")
                    .takes_value(true)
                    .multiple(false)
                    .required(true)
                    .value_name("FILE")
                    .help("Output file"),
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
