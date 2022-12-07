//! XDR codec generation
//!
//! This crate provides library interfaces for programatically generating Rust code to implement
//! RFC4506 XDR encoding/decoding, as well as a command line tool "xdrgen".
//!
//! It is intended to be used with the "xdr-codec" crate, which provides the runtime library for
//! encoding/decoding primitive types, strings, opaque data and arrays.

#![recursion_limit = "128"]

extern crate xdr_codec as xdr;

#[macro_use]
extern crate quote;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;

#[macro_use]
extern crate nom;

#[macro_use]
extern crate bitflags;

use std::env;
use std::fmt::Display;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

type Result<T, E = xdr::Error> = std::result::Result<T, E>;

mod spec;
use spec::{Emit, Emitpack, Symtab};

pub fn exclude_definition_line(line: &str, exclude_defs: &[&str]) -> bool {
    exclude_defs.iter().fold(false, |acc, v| {
        acc || line.contains(&format!("const {}", v))
            || line.contains(&format!("struct {}", v))
            || line.contains(&format!("enum {}", v))
            || line.contains(&format!("for {}", v))
    })
}

/// Generate Rust code from an RFC4506 XDR specification
///
/// `infile` is simply a string used in error messages; it may be empty. `input` is a read stream of
/// the specification, and `output` is where the generated code is sent.
/// `exclude_defs` is list of not generated type definitions.
pub fn generate<In, Out>(
    infile: &str,
    mut input: In,
    mut output: Out,
    exclude_defs: &[&str],
) -> Result<()>
where
    In: Read,
    Out: Write,
{
    let mut source = String::new();

    input.read_to_string(&mut source)?;

    let xdr = match spec::specification(&source) {
        Ok(defns) => Symtab::new(&defns),
        Err(e) => return Err(xdr::Error::from(format!("parse error: {}", e))),
    };

    let xdr = xdr;

    let res: Vec<_> = {
        let consts = xdr
            .constants()
            .filter_map(|(c, &(v, ref scope))| {
                if scope.is_none() {
                    Some(spec::Const(c.clone(), v))
                } else {
                    None
                }
            })
            .map(|c| c.define(&xdr));

        let typespecs = xdr
            .typespecs()
            .map(|(n, ty)| spec::Typespec(n.clone(), ty.clone()))
            .map(|c| c.define(&xdr));

        let typesyns = xdr
            .typesyns()
            .map(|(n, ty)| spec::Typesyn(n.clone(), ty.clone()))
            .map(|c| c.define(&xdr));

        let packers = xdr
            .typespecs()
            .map(|(n, ty)| spec::Typespec(n.clone(), ty.clone()))
            .filter_map(|c| c.pack(&xdr).transpose());

        let unpackers = xdr
            .typespecs()
            .map(|(n, ty)| spec::Typespec(n.clone(), ty.clone()))
            .filter_map(|c| c.unpack(&xdr).transpose());

        consts
            .chain(typespecs)
            .chain(typesyns)
            .chain(packers)
            .chain(unpackers)
            .collect::<Result<Vec<_>>>()?
    };

    let _ = writeln!(
        output,
        r#"
// GENERATED CODE
//
// Generated from {} by xdrgen.
//
// DO NOT EDIT
"#,
        infile
    );

    for it in res {
        let line = it.to_string();
        if !exclude_definition_line(&line, exclude_defs) {
            let _ = writeln!(output, "{}\n", line);
        }
    }

    Ok(())
}

/// Generate pretty Rust code from an RFC4506 XDR specification
///
/// `input` is a string with XDR specification
/// `header` is Rust code to prepend before generated output
#[cfg(feature = "pretty")]
pub fn generate_pretty(input: &str, header: &str, exclude_defs: &[&str]) -> Result<String, anyhow::Error> {
    use proc_macro2::TokenStream;

    let mut file = syn::parse_file(header)?;

    let xdr = match spec::specification(&input) {
        Ok(defns) => Symtab::new(&defns),
        Err(e) => anyhow::bail!(xdr::Error::from(format!("parse error: {}", e))),
    };

    fn filter_exlude<'a, V>(exclude_defs: &'a [&str]) -> impl 'a + FnMut(&(&String, V)) -> bool {
        move |(name, _): &(&String, V),| {
            !exclude_defs.contains(&name.as_str())
        }
    }
    
    let consts = xdr
        .constants()
        .filter(filter_exlude(exclude_defs))
        .filter_map(|(c, &(v, ref scope))| {
            if scope.is_none() {
                Some(spec::Const(c.clone(), v))
            } else {
                None
            }
        })
        .map(|c| c.define(&xdr));

    let typespecs: Vec<_> = xdr
        .typespecs()
        .filter(filter_exlude(exclude_defs))
        .map(|(n, ty)| spec::Typespec(n.clone(), ty.clone()))
        .collect();
    
    let typedefines = typespecs
        .iter()
        .map(|c| c.define(&xdr));

    let typesyns = xdr
        .typesyns()
        .filter(filter_exlude(exclude_defs))
        .map(|(n, ty)| spec::Typesyn(n.clone(), ty.clone()))
        .map(|c| c.define(&xdr));

    let packers = typespecs
        .iter()
        .filter_map(|c| c.pack(&xdr).transpose());

    let unpackers = typespecs
        .iter()
        .filter_map(|c| c.unpack(&xdr).transpose());

    let stream = consts
            .chain(typedefines)
            .chain(typesyns)
            .chain(packers)
            .chain(unpackers)
            .collect::<Result<TokenStream>>()?;

    let body: syn::File = syn::parse2(stream)?;

    file.attrs.append(&mut {body.attrs});
    file.items.append(&mut {body.items});

    Ok(prettyplease::unparse(&file))
}

/// Simplest possible way to generate Rust code from an XDR specification.
///
/// It is intended for use in a build.rs script:
///
/// ```ignore
/// extern crate xdrgen;
///
/// fn main() {
///    xdrgen::compile("src/simple.x").unwrap();
/// }
/// ```
///
/// Output is put into OUT_DIR, and can be included:
///
/// ```ignore
/// mod simple {
///    use xdr_codec;
///
///    include!(concat!(env!("OUT_DIR"), "/simple_xdr.rs"));
/// }
/// ```
///
/// If your specification uses types which are not within the specification, you can provide your
/// own implementations of `Pack` and `Unpack` for them.
pub fn compile<P>(infile: P, exclude_defs: &[&str]) -> Result<()>
where
    P: AsRef<Path> + Display,
{
    let input = File::open(&infile)?;

    let mut outdir = PathBuf::from(env::var("OUT_DIR").unwrap_or(String::from(".")));
    let outfile = PathBuf::from(infile.as_ref())
        .file_stem()
        .unwrap()
        .to_owned()
        .into_string()
        .unwrap()
        .replace("-", "_");

    outdir.push(&format!("{}_xdr.rs", outfile));

    let output = File::create(outdir)?;

    generate(
        infile.as_ref().as_os_str().to_str().unwrap_or("<unknown>"),
        input,
        output,
        exclude_defs,
    )
}
