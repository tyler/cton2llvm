extern crate cton2llvm;
use cton2llvm::{parse_cton_file, translate};
use std::path::PathBuf;

#[test]
fn basic() {
    let path = PathBuf::from("tests/basic.cton");
    let cton_module = parse_cton_file(path).expect("parsed cton file");
    match translate(cton_module) {
        Ok(_) => println!("compiled!"),
        Err(e) => panic!("Failed to compile: {:?}", e),
    }
}
