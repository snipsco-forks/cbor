#![no_std]

#[macro_use]
extern crate serde_derive;
extern crate serde;

#[derive(Debug, Deserialize, PartialEq)]
struct Color(u16, u16, u16);

fn main() {
    let red = b"\x83\x19\x12\x34\x00\x00";
    let red: Color = serde_cbor::from_slice(&red[..]).unwrap();
    assert!(red == Color(4660, 0, 0));
    // println!("{:?}", red);
}
