use xdrgen::pretty::{GenerateOptions, ConstTaggingOptions};
use quote::quote;

extern crate xdrgen;

fn main() {
    println!("cargo:rerun-if-changed=../example.x");
    println!("cargo:rerun-if-changed=../header.x");

    let input = std::fs::read_to_string("../example.x").unwrap();
    let rust_header = "
        #![allow(dead_code)]
        use xdr_codec;
        type FromHeader = i32;
    ";
    let xdr_header = &std::fs::read_to_string("../header.x").unwrap();
    let tagging = Some(ConstTaggingOptions {
        const_filter: |name| name.starts_with("VERSION_"),
        ty_filter: |_ty, _tag| true,
        quote: |ty, tag| quote!(
            impl crate::Versioned for #ty {
                const VERSION: i64 = #tag;
            }
        ),
    });
    let _simple_output = xdrgen::generate_pretty(&(input.clone() + &xdr_header), &GenerateOptions{rust_header, tagging: tagging.clone(), ..Default::default()}).unwrap();
    let output = xdrgen::generate_pretty(&input, &GenerateOptions{rust_header, xdr_header, tagging, ..Default::default()}).unwrap();
    std::fs::create_dir_all("generated").unwrap();
    std::fs::write("generated/pretty_xdr.rs", output).unwrap();
}
