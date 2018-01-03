use std::path::PathBuf;
use std::fs::File;
use std::io::Read;
use cretonne::ir::Function;
use cton_reader::parse_functions;

pub struct CtonModule {
    pub functions: Vec<Function>,
}

pub fn parse_cton_file(path: PathBuf) -> Result<CtonModule, String> {
    let text = read_to_string(&path).expect("file has contents");
    let functions = parse_functions(&text).expect("parsed cton file");

    Ok(CtonModule { functions: functions })
}

fn read_to_string(path: &PathBuf) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("{:?}: {}", path, e))?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer).expect(
        "read should not fail",
    );
    Ok(buffer)
}
