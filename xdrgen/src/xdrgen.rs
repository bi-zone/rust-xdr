#![crate_type = "bin"]

extern crate xdrgen;
extern crate env_logger;
extern crate clap;

use std::fs::File;
use std::io::{BufReader, Write};
use std::io::{stderr, stdin, stdout};

use clap::{Command, arg};

use xdrgen::generate;

fn main() {
    let _ = env_logger::init();

    let matches = Command::new("XDR code generator")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(arg!(<FILE> "Set .x file"))
        .get_matches();

    let output = stdout();
    let mut err = stderr();

    let res = if let Some(fname) = matches.get_one::<String>("FILE") {
        let f = match File::open(fname) {
            Ok(f) => f,
            Err(e) => {
                let _ = writeln!(&mut err, "Failed to open {}: {}", fname, e);
                std::process::exit(1);
            }
        };
        generate(fname, BufReader::new(f), output, &[])
    } else {
        generate("stdin", BufReader::new(stdin()), output, &[])
    };

    if let Err(e) = res {
        let _ = writeln!(&mut err, "Failed: {}", e);
    }
}
