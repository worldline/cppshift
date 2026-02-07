use std::{env, fs};

use cppshift::ast;
use miette::Error;

fn main() -> Result<(), Error> {
    for args in env::args() {
        if let Ok(source) = fs::read_to_string(&args) {
            println!("Source file {args}:");
            let source_ast = ast::parse_file(&source)?;
            println!("{source_ast:#?}");
        } else {
            eprintln!("Can't read source file `{args}`");
        }
    }

    Ok(())
}
