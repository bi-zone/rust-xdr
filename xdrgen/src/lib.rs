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

mod spec;
use spec::{Emit, Emitpack, Symtab, SymDef};

mod error;
pub use self::error::{Result, Error};

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

    let defns = spec::specification(&source)?;
    let mut xdr = Symtab::new();
    xdr.update_consts(&defns, &());

    let res: Vec<_> = {
        let consts = xdr
            .constants()
            .map(SymDef::map_value)
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
            .map(SymDef::map_value)
            .map(|(n, ty)| spec::Typespec(n.clone(), ty.clone()))
            .map(|c| c.define(&xdr));

        let typesyns = xdr
            .typesyns()
            .map(SymDef::map_value)
            .map(|(n, ty)| spec::Typesyn(n.clone(), ty.clone()))
            .map(|c| c.define(&xdr));

        let packers = xdr
            .typespecs()
            .map(SymDef::map_value)
            .map(|(n, ty)| spec::Typespec(n.clone(), ty.clone()))
            .filter_map(|c| c.pack(&xdr).transpose());

        let unpackers = xdr
            .typespecs()
            .map(SymDef::map_value)
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

#[cfg(feature = "pretty")]
pub mod pretty {
    use std::collections::BTreeMap;

    use proc_macro2::{TokenStream, Ident};

    use crate::spec::{Defn, quote_ident, SymDef};

    #[derive(Default)]
    pub struct GenerateOptions<'a> {
        pub rust_header: &'a str,
        pub exclude_defs: &'a [&'a str],
        pub tagging: Option<ConstTaggingOptions>,
        pub xdr_header: &'a str,
    }

    #[derive(Clone)]
    pub struct ConstTaggingOptions {
        pub const_filter: fn(&str) -> bool,
        pub ty_filter: fn(&str, &str) -> bool,
        pub quote: fn(&Ident, &Ident) -> proc_macro2::TokenStream,
    }

    impl ConstTaggingOptions {
        pub(super) fn tagged_types<'a>(&'a self, input: &'a [Defn], exclude_defs: &[&str]) -> BTreeMap<&str, TokenStream> {
            let mut result = BTreeMap::new();
            let mut tag = None;
            for def in input {
                match (def, &tag) {
                    (Defn::Const(name, _), _) if !exclude_defs.contains(&name.as_str()) => if (self.const_filter)(name) {
                        tag = Some((name.as_str(), quote_ident(name)));
                    },
                    (Defn::Typespec(name, _), Some(tag))  if !exclude_defs.contains(&name.as_str()) && (self.ty_filter)(name.as_str(), tag.0) => {
                        result.insert(name.as_str(), (self.quote)(&quote_ident(name), &tag.1));
                    },
                    _ => {}
                }
            }
            result
        }
    }

    pub(super) fn filter_exlude<'a, V>(exclude_defs: &'a [&str]) -> impl 'a + FnMut(&(&String, V)) -> bool {
        move |(name, _): &(&String, V),| {
            !exclude_defs.contains(&name.as_str())
        }
    }

    #[derive(Clone)]
    pub(super) struct Meta {
        pub(super) header: bool,
    }

    pub(super) fn filter_header_out<V>((_, def): &(&String, &SymDef<V, Meta>)) -> bool {
        !def.meta.header
    }
}

/// Generate pretty Rust code from an RFC4506 XDR specification
///
/// `input` is a string with XDR specification
/// `header` is Rust code to prepend before generated output
#[cfg(feature = "pretty")]
pub fn generate_pretty(input: &str, options: &pretty::GenerateOptions) -> Result<String, anyhow::Error> {
    use anyhow::Context;
    use proc_macro2::TokenStream;

    let mut file = syn::parse_file(options.rust_header)?;

    let xdr_header_defns = if options.xdr_header.is_empty() {
        vec![]
    } else {
        spec::specification(options.xdr_header).context("parse XDR header")?
    };
    let defns = spec::specification(&input).context("parse main XDR input")?;

    let mut tagged_types = options.tagging.as_ref().map(|tagging| tagging.tagged_types(&defns, options.exclude_defs)).unwrap_or_default();

    let mut xdr = Symtab::new();
    
    xdr.update_consts(&xdr_header_defns, &pretty::Meta{ header: true });
    xdr.update_consts(&defns, &pretty::Meta{ header: false });

    let consts = xdr
        .constants()
        .filter(pretty::filter_header_out)
        .map(SymDef::map_value)
        .filter(pretty::filter_exlude(options.exclude_defs))
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
        .filter(pretty::filter_header_out)
        .map(SymDef::map_value)
        .filter(pretty::filter_exlude(options.exclude_defs))
        .map(|(n, ty)| spec::Typespec(n.clone(), ty.clone()))
        .collect();
    
    let typedefines = typespecs
        .iter()
        .flat_map(|c| {
            [
                c.define(&xdr),
                Ok(tagged_types.remove(c.0.as_str()).unwrap_or_default()),
            ]
        });

    let typesyns = xdr
        .typesyns()
        .filter(pretty::filter_header_out)
        .map(SymDef::map_value)
        .filter(pretty::filter_exlude(options.exclude_defs))
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

    // prettyplease treats this as newline
    fn trailing_hardbreak(item: syn::Item) -> [syn::Item; 2] {
        [item, syn::Item::Verbatim(TokenStream::new())]
    }

    file.attrs.append(&mut {body.attrs});
    file.items.reserve(body.items.len() * 2);
    file.items.extend(body.items.into_iter().map(trailing_hardbreak).flatten());

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
