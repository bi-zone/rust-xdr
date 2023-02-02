use xdr_codec;

use std::io::Cursor;
use xdr_codec::{unpack,pack};

#[path ="../generated/pretty_xdr.rs"]
mod xdr;

fn main() {
    let bar = xdr::Bar { data: vec![1,2,3] };
    let foo = xdr::Foo {
        a: 1, b: 2, c: 3,
        bar: vec![bar.clone()],
        bar_pair: xdr::BarPair([bar.clone(), bar.clone()]),
        barish: None,
        name: String::from("foox"),
        thing: xdr::Things::C,
        type_: 123,
    };
    let foobar = xdr::Foobar::C(foo);

    let mut buf = Vec::new();

    pack(&foobar, &mut buf).unwrap();
    println!("buf={:?} len={}", buf, buf.len());

    let mut cur = Cursor::new(buf);
    
    let foobar2 = unpack(&mut cur).unwrap();

    println!("foobar={:?}", foobar);
    println!("foobar2={:?}", foobar2);
    assert_eq!(foobar, foobar2);
}
