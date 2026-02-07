use std::{env, fs};

use cppshift::Lexer;
use miette::Error;

fn main() -> Result<(), Error> {
    for args in env::args() {
        if let Ok(source) = fs::read_to_string(&args) {
            println!("Source file {args}:");
            for token in Lexer::new(&source) {
                println!("{:?}", token?);
            }
        } else {
            eprintln!("Can't read source file `{args}`");
        }
    }

    Ok(())
}
