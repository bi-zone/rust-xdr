use xdr_codec;

use std::io::Cursor;
use xdr_codec::{unpack,pack};

mod simple {
    #![allow(dead_code)]
    use xdr_codec;
    include!(concat!(env!("OUT_DIR"), "/example_xdr.rs"));
}

fn main() {
    let bar = simple::Bar { data: vec![1,2,3] };
    let foo = simple::Foo {
        a: 1, b: 2, c: 3,
        bar: vec![bar.clone()],
        bar_pair: simple::BarPair([bar.clone(), bar.clone()]),
        barish: None,
        name: String::from("foox"),
        thing: simple::Things::C,
        type_: 123,
    };
    // "derive_serde" feature makes this working
    // println!("Serialized JSON: {}", serde_json::to_string(&foo).unwrap());

    let mut buf = Vec::new();

    pack(&foo, &mut buf).unwrap();
    println!("foo={:?}", foo);
    println!("buf={:?} len={}", buf, buf.len());

    let mut cur = Cursor::new(buf);
    
    let foo2 = unpack(&mut cur).unwrap();

    println!("foo={:?}", foo);
    println!("foo2={:?}", foo2);
    assert_eq!(foo, foo2);
}
