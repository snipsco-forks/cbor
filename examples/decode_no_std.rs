// To make this actually runnable, remove the comment signs from the below, and add libc to
// dev-dependencies

// #![feature(start)]
// #![no_std]
//
// extern crate libc;
//
// #[start]
// fn start(_argc: isize, _argv: *const *const u8) -> isize {
//     main();
//     0
// }

#[macro_use]
extern crate serde_derive;
extern crate serde;

use serde::Serialize;

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct Color(u16, u16, u16);

fn main() {
    let input = b"\x83\x19\x12\x34\x00\x00";
    let red: Color = serde_cbor::from_slice(&input[..]).unwrap();
    assert!(red == Color(4660, 0, 0));
    // println!("{:?}", red);

    let mut buf = [255u8; 10];
    {
        let mut w = serde_cbor::WindowedInfinity::new(&mut buf, 0);
        red.serialize(&mut serde_cbor::ser::Serializer::new(&mut w));
    }
    assert!(&buf[..input.len()] == input, "Reserialization changed");
}
