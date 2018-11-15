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

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct ColorByName {
    red: u16,
    green: u16,
    blue: u16,
    name: [u8; 5],
}

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

    let red_by_name = ColorByName { red: red.0, green: red.1, blue: red.2, name: *b"-red-" };
    // println!("Serilizing {:?}", red_by_name);

    let mut buf = [255u8; 40];
    let cursor = {
        let mut w = serde_cbor::WindowedInfinity::new(&mut buf, 0);
        {
            let mut ser = &mut serde_cbor::ser::Serializer::new(&mut w);
            ser.self_describe().unwrap();
            red_by_name.serialize(ser);
        }
        w.get_cursor()
    };
    let red_by_name_again: ColorByName = serde_cbor::from_slice(&buf[..cursor as usize]).unwrap();
    assert!(red_by_name == red_by_name_again, "Deserialization produced differences");
    // println!("Deserialized again to {:?}", red_by_name_again);
}
