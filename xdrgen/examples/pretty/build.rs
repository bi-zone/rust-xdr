use xdrgen::pretty::{GenerateOptions, ConstTaggingOptions};
use quote::quote;

extern crate xdrgen;

fn main() {
    println!("cargo:rerun-if-changed=../example.x");
    let input = std::fs::read_to_string("../example.x").unwrap();
    let header = "
        #![allow(dead_code)]
        use xdr_codec;
    ";
    let tagging = Some(ConstTaggingOptions {
        const_filter: |name| name.starts_with("VERSION_"),
        quote: |ty, tag| quote!(
            impl crate::Versioned for #ty {
                const VERSION: i64 = #tag;
            }
        ),
    });
    let output = xdrgen::generate_pretty(&input, &GenerateOptions{header, tagging, ..Default::default()}).unwrap();
    std::fs::create_dir_all("generated").unwrap();
    std::fs::write("generated/pretty_xdr.rs", output).unwrap();
}
