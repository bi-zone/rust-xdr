extern crate xdrgen;

fn main() {
    println!("cargo:rerun-if-changed=src/simple.x");
    let input = std::fs::read_to_string("src/simple.x").unwrap();
    let header = "
        #![allow(dead_code)]
        use xdr_codec;
    ";
    let output = xdrgen::generate_pretty(&input, header, &[]).unwrap();
    std::fs::create_dir_all("generated").unwrap();
    std::fs::write("generated/simple_xdr.rs", output).unwrap();
}
