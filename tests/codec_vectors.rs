//! Runs the language-neutral vectors in spec/vectors/ against the codecs.

use modulino_driver::codec::buttons;
use toml::{Table, Value};

fn load(path: &str) -> Table {
    let full = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), path);
    let text = std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{full}: {e}"));
    text.parse().unwrap_or_else(|e| panic!("{full}: {e}"))
}

fn cases<'a>(doc: &'a Table, op: &str) -> &'a [Value] {
    let cases = doc[op].as_array().expect("array of cases");
    assert!(!cases.is_empty(), "no {op} cases");
    cases
}

fn bytes<const N: usize>(v: &Value) -> [u8; N] {
    let list: Vec<u8> = v
        .as_array()
        .expect("byte array")
        .iter()
        .map(|b| u8::try_from(b.as_integer().expect("integer")).expect("byte"))
        .collect();
    list.try_into().expect("length")
}

fn flag(t: &Value, key: &str) -> bool {
    t[key].as_bool().expect("bool")
}

#[test]
fn buttons_vectors() {
    let doc = load("spec/vectors/buttons.toml");
    let mut ops: Vec<&str> = doc.keys().map(String::as_str).collect();
    ops.sort_unstable();
    assert_eq!(ops, ["decode", "encode_leds"], "unknown op table");

    for case in cases(&doc, "decode") {
        let got = buttons::decode(&bytes(&case["input"]));
        let want = &case["expect"];
        assert_eq!(
            (got.a, got.b, got.c),
            (flag(want, "a"), flag(want, "b"), flag(want, "c")),
            "decode {:?}",
            case["input"]
        );
    }

    for case in cases(&doc, "encode_leds") {
        let a = &case["args"];
        let got = buttons::encode_leds(flag(a, "a"), flag(a, "b"), flag(a, "c"));
        assert_eq!(got, bytes(&case["expect"]), "encode_leds {a:?}");
    }
}
