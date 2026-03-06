# CppShift

[<img alt="github" src="https://img.shields.io/badge/github-46beaa?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/worldline/cppshift)
[<img alt="crates-io" src="https://img.shields.io/badge/crates.io-ffeb78?style=for-the-badge&labelColor=555555&logo=rust" height="20">](https://crates.io/crates/cppshift)
[<img alt="docs-rs" src="https://img.shields.io/badge/docs.rs-41b4d2?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/cppshift)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/worldline/cppshift/ci.yml?branch%3Amain&style=for-the-badge" height="20">](https://github.com/worldline/cppshift/actions?query=branch%3Amain)
[<img alt="dependency status" src="https://img.shields.io/deps-rs/repo/github/worldline/cppshift?style=for-the-badge" height="20">](https://deps.rs/repo/github/worldline/cppshift)

This project provide a C++ scanner and a transpiler.
The goal is not to use compiled C++ like [cxx](https://cxx.rs/) can do, but to convert c++ code into native Rust.

This conversion need to be use with simple extract of code with specific need.
The goal is not to bind an entire C++ library, but convert only the code you need.

## Lexer

The first component is a Lexer, which parses and validates C++20 lexical elements.

To parse a C++ source file and display the decomposed tokens, run the _lexer_ example:
```bash
cargo run --example lexer -- <source_path>
```

## AST

The AST (Abstract Syntax Tree) structures a source file into a hierarchical tree.

It uses the Lexer’s token stream to build a syntax tree representing the C++ source code.

The design of the AST objects is heavily inspired by the [syn](https://github.com/dtolnay/syn) crate.
To parse a C++ source file and inspect the AST decomposition, run the _ast_ example:
```bash
cargo run --example ast -- <source_path>
```
