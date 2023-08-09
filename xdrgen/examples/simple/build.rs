extern crate xdrgen;

fn main() {
    println!("cargo:rerun-if-changed=../example.x");
    println!("cargo:rerun-if-changed=../header.x");

    let header = std::fs::read_to_string("../header.x").unwrap();
    let example = std::fs::read_to_string("../example.x").unwrap();
    std::fs::write("src/simple.x", header + &example).unwrap();
    xdrgen::compile("src/simple.x", &[]).unwrap();
}
