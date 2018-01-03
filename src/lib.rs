extern crate cretonne;
extern crate cton_reader;
extern crate inkwell;

pub mod parser;
pub use parser::parse_cton_file;

pub mod translator;
pub use translator::translate;
