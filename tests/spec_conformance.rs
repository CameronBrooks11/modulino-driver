//! src/spec.rs must say exactly what spec/nodes.toml says.

use modulino_driver::spec::{ChipId, WriteLen, CHIPS, NODES};
use toml::{Table, Value};

fn load(path: &str) -> Table {
    let full = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), path);
    let text = std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{full}: {e}"));
    text.parse().unwrap_or_else(|e| panic!("{full}: {e}"))
}

fn int(v: &Value) -> usize {
    usize::try_from(v.as_integer().expect("integer")).expect("non-negative")
}

fn write_len(v: &Value) -> WriteLen {
    let t = v.as_table().expect("write_len is a table");
    let mut keys: Vec<&str> = t.keys().map(String::as_str).collect();
    keys.sort_unstable();
    match keys.as_slice() {
        ["default"] => WriteLen::Fixed(int(&t["default"])),
        ["grayscale", "mono"] => WriteLen::Mode {
            mono: int(&t["mono"]),
            grayscale: int(&t["grayscale"]),
        },
        other => panic!("unknown write_len modes {other:?}"),
    }
}

#[test]
fn nodes_toml_matches_spec_rs() {
    let doc = load("spec/nodes.toml");
    let rows = doc["node"].as_array().expect("[[node]] rows");
    assert_eq!(rows.len(), NODES.len(), "number of node kinds");

    for (row, spec) in rows.iter().zip(NODES.iter()) {
        let name = row["kind"].as_str().expect("kind");
        assert_eq!(name, spec.name, "kind order");
        let straps: Vec<u8> = row["straps"]
            .as_array()
            .expect("straps")
            .iter()
            .map(|v| u8::try_from(int(v)).expect("strap fits u8"))
            .collect();
        assert_eq!(straps, spec.straps, "{name}: straps");
        assert_eq!(
            int(&row["read_payload"]),
            spec.read_payload,
            "{name}: read_payload"
        );
        assert_eq!(
            write_len(&row["write_len"]),
            spec.write_len,
            "{name}: write_len"
        );
    }
}

fn chip_id(v: &Value) -> ChipId {
    let t = v.as_table().expect("id is a table");
    let u8_at = |k: &str| u8::try_from(int(&t[k])).expect("byte");
    match t["method"].as_str().expect("method") {
        "register8" => ChipId::Register8 {
            register: u8_at("register"),
            value: u8_at("value"),
            mask: u8_at("mask"),
        },
        "register16" => ChipId::Register16 {
            register: u16::try_from(int(&t["register"])).expect("u16"),
            value: u8_at("value"),
            mask: u8_at("mask"),
        },
        "sht3x_status" => ChipId::Sht3xStatus {
            command: u16::try_from(int(&t["command"])).expect("u16"),
        },
        "none" => ChipId::None,
        other => panic!("unknown id method {other:?}"),
    }
}

#[test]
fn chips_toml_matches_spec_rs() {
    let doc = load("spec/chips.toml");
    let rows = doc["chip"].as_array().expect("[[chip]] rows");
    assert_eq!(rows.len(), CHIPS.len(), "number of chip kinds");

    for (row, spec) in rows.iter().zip(CHIPS.iter()) {
        let name = row["kind"].as_str().expect("kind");
        assert_eq!(name, spec.name, "kind order");
        let addresses: Vec<u8> = row["addresses"]
            .as_array()
            .expect("addresses")
            .iter()
            .map(|v| u8::try_from(int(v)).expect("address fits u8"))
            .collect();
        assert_eq!(addresses, spec.addresses, "{name}: addresses");
        assert_eq!(chip_id(&row["id"]), spec.id, "{name}: id");
    }
}

#[test]
fn chip_kinds_match_their_names() {
    for chip in CHIPS {
        assert_eq!(format!("{:?}", chip.kind).to_lowercase(), chip.name);
    }
}
