use cppshift::ast;
use miette::Error;

use crate::common::Sources;

mod common;

fn main() -> Result<(), Error> {
    for source in Sources::default() {
        if let Ok(source_content) = source.read_to_string() {
            println!("Source file {}:", source.to_str().unwrap_or("<unknown>"));
            let source_ast = ast::parse_file(&source_content)?;
            println!("{source_ast:#?}");
        } else if let Some(path) = source.to_str() {
            eprintln!("Can't read source file `{:?}`", path);
        } else {
            eprintln!("Can't read source file");
        }
    }

    Ok(())
}
