//! spec/nodes.toml must agree with the node firmware at the commit it pins.
//!
//! Reads `Core/Src/main.c` from a checkout of `arduino/node_modulino_firmware`
//! named by `FIRMWARE_DIR` (`just drift` makes one; CI checks one out at the
//! pin). It compares two things: the `NODE_*` strap defines, and the write
//! lengths returned by `getReceiveBufferSize()`.
//!
//! The extractor is deliberately narrow: it understands the lines main.c has
//! at the pin and panics on anything else. If a firmware change breaks it,
//! replace it with a reviewed manual check rather than widening it
//! (docs/design.md section 6).

use std::collections::BTreeMap;
use std::path::Path;

use toml::{Table, Value};

/// Each kind's firmware strap names, by hand. A `NODE_*` define that no kind
/// claims fails the check, so a new board kind in the firmware shows up.
const KINDS: &[(&str, &[&str])] = &[
    ("buttons", &["NODE_BUTTONS"]),
    ("buzzer", &["NODE_BUZZER"]),
    ("vibro", &["NODE_VIBRO"]),
    ("knob", &["NODE_ENCODER", "NODE_ENCODER_2"]),
    ("pixels", &["NODE_SMARTLEDS"]),
    ("joystick", &["NODE_JOYSTICK"]),
    ("opto_relay", &["NODE_OPTORELAY"]),
    ("latch_relay", &["NODE_LATCHRELAY"]),
    ("led_matrix", &["NODE_LEDMATRIX"]),
    ("motors", &["NODE_MOTOR"]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Len {
    Fixed(usize),
    Mode { mono: usize, grayscale: usize },
}

fn number(s: &str) -> u64 {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => s.parse(),
    }
    .unwrap_or_else(|_| panic!("not a number: {s:?}"))
}

/// `#define NAME VALUE` lines, as written.
fn defines(source: &str) -> BTreeMap<String, String> {
    source
        .lines()
        .filter_map(|l| l.strip_prefix("#define "))
        .filter_map(|rest| {
            let mut parts = rest.split_whitespace();
            let name = parts.next()?;
            let value = parts.next()?;
            Some((name.to_string(), value.to_string()))
        })
        .collect()
}

/// One `return` expression of `getReceiveBufferSize()`.
fn length(expr: &str, defines: &BTreeMap<String, String>) -> Len {
    if let Some((cond, arms)) = expr.split_once('?') {
        let (yes, no) = arms
            .split_once(':')
            .unwrap_or_else(|| panic!("unexpected return {expr:?}"));
        assert_eq!(
            cond.trim(),
            "ledMatrixGrayscaleMode",
            "unexpected condition in return {expr:?}"
        );
        return Len::Mode {
            grayscale: number(yes) as usize,
            mono: number(no) as usize,
        };
    }
    if let Some((name, factor)) = expr.split_once('*') {
        let value = defines
            .get(name.trim())
            .unwrap_or_else(|| panic!("no #define for {name:?} in return {expr:?}"));
        return Len::Fixed((number(value) * number(factor)) as usize);
    }
    if expr
        .trim()
        .chars()
        .all(|c| c.is_ascii_hexdigit() || c == 'x')
    {
        return Len::Fixed(number(expr) as usize);
    }
    panic!("unexpected return {expr:?}; replace the extractor with a manual check");
}

/// Write length per `NODE_*` name, plus the length after the switch, which
/// a kind with no `case` gets.
fn lengths(source: &str, defines: &BTreeMap<String, String>) -> (BTreeMap<String, Len>, Len) {
    let start = source
        .find("uint8_t getReceiveBufferSize() {")
        .expect("getReceiveBufferSize() not found");
    let body = &source[start..];
    let end = body.find("\n}").expect("getReceiveBufferSize() not closed");
    let mut out = BTreeMap::new();
    let mut pending: Vec<String> = Vec::new();
    let mut fallback = None;
    for line in body[..end].lines().skip(1) {
        let line = line.split("//").next().unwrap().trim();
        if line.is_empty() || line == "switch (PINSTRAP_ADDRESS) {" || line == "}" {
            continue;
        }
        if let Some(label) = line.strip_prefix("case ").and_then(|l| l.strip_suffix(':')) {
            pending.push(label.to_string());
            continue;
        }
        if let Some(expr) = line
            .strip_prefix("return ")
            .and_then(|l| l.strip_suffix(';'))
        {
            let len = length(expr, defines);
            if pending.is_empty() {
                assert!(fallback.is_none(), "two returns outside a case");
                fallback = Some(len);
            }
            for label in pending.drain(..) {
                out.insert(label, len);
            }
            continue;
        }
        panic!("unexpected line in getReceiveBufferSize(): {line:?}");
    }
    assert!(pending.is_empty(), "case labels without a return");
    (out, fallback.expect("no return after the switch"))
}

fn spec_len(v: &Value) -> Len {
    let t = v.as_table().expect("write_len is a table");
    let int = |k: &str| t[k].as_integer().expect("integer") as usize;
    if t.contains_key("default") {
        Len::Fixed(int("default"))
    } else {
        Len::Mode {
            mono: int("mono"),
            grayscale: int("grayscale"),
        }
    }
}

#[test]
#[ignore = "needs a firmware checkout in FIRMWARE_DIR; run `just drift`"]
fn nodes_toml_matches_pinned_firmware() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let spec: Table = std::fs::read_to_string(root.join("spec/nodes.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let firmware = spec["firmware"].as_table().unwrap();
    let pin = firmware["commit"].as_str().unwrap();
    let file = firmware["file"].as_str().unwrap();

    let dir = std::env::var("FIRMWARE_DIR").expect("FIRMWARE_DIR is not set; run `just drift`");
    let dir = Path::new(&dir);
    let head = std::fs::read_to_string(dir.join(".git/HEAD"))
        .unwrap_or_else(|e| panic!("{}: {e}", dir.join(".git/HEAD").display()));
    assert_eq!(
        head.trim(),
        pin,
        "FIRMWARE_DIR is not checked out at the pin"
    );
    let source = std::fs::read_to_string(dir.join(file)).unwrap();

    let defines = defines(&source);
    let (lengths, fallback) = lengths(&source, &defines);

    let firmware_nodes: Vec<&String> = defines.keys().filter(|k| k.starts_with("NODE_")).collect();
    for name in &firmware_nodes {
        assert!(
            KINDS
                .iter()
                .any(|(_, names)| names.contains(&name.as_str())),
            "{name} in the firmware belongs to no kind"
        );
    }
    let rows = spec["node"].as_array().unwrap();
    assert_eq!(rows.len(), KINDS.len(), "kinds in spec/nodes.toml");

    for row in rows {
        let kind = row["kind"].as_str().unwrap();
        let names = KINDS
            .iter()
            .find(|(k, _)| *k == kind)
            .unwrap_or_else(|| panic!("{kind} has no firmware names"))
            .1;
        let mut want: Vec<u64> = row["straps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_integer().unwrap() as u64)
            .collect();
        let mut got: Vec<u64> = names
            .iter()
            .map(|n| number(defines.get(*n).unwrap_or_else(|| panic!("{n} not defined"))))
            .collect();
        want.sort_unstable();
        got.sort_unstable();
        assert_eq!(want, got, "{kind}: straps");

        let lens: Vec<Len> = names
            .iter()
            .map(|n| lengths.get(*n).copied().unwrap_or(fallback))
            .collect();
        for len in lens {
            assert_eq!(spec_len(&row["write_len"]), len, "{kind}: write length");
        }
    }
}
