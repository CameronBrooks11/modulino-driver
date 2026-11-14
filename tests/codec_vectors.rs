//! Runs the language-neutral vectors in spec/vectors/ against the codecs.
//!
//! Each file holds one table per codec operation. A case has `input` or
//! `args`, and either `expect` or `error`. An encode case whose bytes start
//! with a firmware command prefix must say `node = "reserved_prefix"`, and
//! no other case may.

use modulino_driver::codec::joystick::{self, JoystickRaw};
use modulino_driver::codec::pixels::{self, Led, LEDS};
use modulino_driver::codec::{buttons, buzzer, knob, latch_relay, opto_relay, vibro, OutOfRange};
use modulino_driver::Color;
use toml::{Table, Value};

fn load(kind: &str, ops: &[&str]) -> Table {
    let full = format!("{}/spec/vectors/{kind}.toml", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{full}: {e}"));
    let doc: Table = text.parse().unwrap_or_else(|e| panic!("{full}: {e}"));
    let mut found: Vec<&str> = doc.keys().map(String::as_str).collect();
    let mut want = ops.to_vec();
    found.sort_unstable();
    want.sort_unstable();
    assert_eq!(found, want, "{kind}: op tables");
    doc
}

/// Every file in spec/vectors/ has a runner below.
#[test]
fn every_vector_file_runs() {
    let dir = format!("{}/spec/vectors", env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .expect("spec/vectors")
        .map(|e| e.expect("entry").file_name().into_string().expect("utf-8"))
        .filter(|name| name.ends_with(".toml"))
        .collect();
    files.sort_unstable();
    let run = [
        "buttons.toml",
        "buzzer.toml",
        "joystick.toml",
        "knob.toml",
        "latch_relay.toml",
        "opto_relay.toml",
        "pixels.toml",
        "vibro.toml",
    ];
    assert_eq!(files, run);
}

fn cases<'a>(doc: &'a Table, op: &str) -> &'a [Value] {
    let cases = doc[op].as_array().expect("array of cases");
    assert!(!cases.is_empty(), "no {op} cases");
    cases
}

fn int(v: &Value) -> i64 {
    v.as_integer().expect("integer")
}

fn arg<T: TryFrom<i64>>(case: &Value, key: &str) -> T {
    T::try_from(int(&case["args"][key])).unwrap_or_else(|_| panic!("{key} out of range"))
}

fn flag(t: &Value, key: &str) -> bool {
    t[key].as_bool().expect("bool")
}

fn bytes<const N: usize>(v: &Value) -> [u8; N] {
    let list: Vec<u8> = v
        .as_array()
        .expect("byte array")
        .iter()
        .map(|b| u8::try_from(int(b)).expect("byte"))
        .collect();
    list.try_into().expect("length")
}

/// Check an encode case: the bytes or the error, and the prefix marking.
fn check_encode<const N: usize>(case: &Value, got: Result<[u8; N], OutOfRange>) {
    match case.get("error").map(|e| e.as_str().expect("error name")) {
        Some("out_of_range") => assert_eq!(got, Err(OutOfRange), "{case:?}"),
        Some(other) => panic!("unknown error {other:?}"),
        None => {
            let frame = got.unwrap_or_else(|e| panic!("{case:?}: {e:?}"));
            assert_eq!(frame, bytes(&case["expect"]), "{case:?}");
            let reserved = frame.starts_with(b"CF") || frame.starts_with(b"DIE");
            let marked = case.get("node").and_then(Value::as_str) == Some("reserved_prefix");
            assert_eq!(reserved, marked, "prefix marking: {case:?}");
        }
    }
}

#[test]
fn buttons_vectors() {
    let doc = load("buttons", &["decode", "encode_leds"]);
    for case in cases(&doc, "decode") {
        let got = buttons::decode(&bytes(&case["input"]));
        let want = &case["expect"];
        assert_eq!(
            (got.a, got.b, got.c),
            (flag(want, "a"), flag(want, "b"), flag(want, "c")),
            "{case:?}"
        );
    }
    for case in cases(&doc, "encode_leds") {
        let a = &case["args"];
        let got = buttons::encode_leds(flag(a, "a"), flag(a, "b"), flag(a, "c"));
        check_encode(case, Ok(got));
    }
}

#[test]
fn buzzer_vectors() {
    let doc = load("buzzer", &["encode_tone", "encode_stop"]);
    for case in cases(&doc, "encode_tone") {
        let got = buzzer::encode_tone(arg(case, "frequency_hz"), arg(case, "duration_ms"));
        check_encode(case, got);
    }
    for case in cases(&doc, "encode_stop") {
        check_encode(case, Ok(buzzer::encode_stop()));
    }
}

#[test]
fn vibro_vectors() {
    let doc = load("vibro", &["encode_on", "encode_off"]);
    for case in cases(&doc, "encode_on") {
        check_encode(
            case,
            vibro::encode_on(arg(case, "duration_ms"), arg(case, "power")),
        );
    }
    for case in cases(&doc, "encode_off") {
        check_encode(case, Ok(vibro::encode_off()));
    }
}

#[test]
fn knob_vectors() {
    let doc = load("knob", &["decode"]);
    for case in cases(&doc, "decode") {
        let got = knob::decode(&bytes(&case["input"]));
        let want = &case["expect"];
        assert_eq!(
            (got.count as i64, got.pressed),
            (int(&want["count"]), flag(want, "pressed")),
            "{case:?}"
        );
    }
}

#[test]
fn joystick_vectors() {
    let doc = load("joystick", &["decode", "to_state"]);
    for case in cases(&doc, "decode") {
        let got = joystick::decode(&bytes(&case["input"]));
        let want = &case["expect"];
        assert_eq!(
            (got.x as i64, got.y as i64, got.pressed),
            (int(&want["x"]), int(&want["y"]), flag(want, "pressed")),
            "{case:?}"
        );
    }
    for case in cases(&doc, "to_state") {
        let raw = JoystickRaw {
            x: arg(case, "x"),
            y: arg(case, "y"),
            pressed: flag(&case["args"], "pressed"),
        };
        let got = joystick::to_state(raw, arg(case, "deadzone"));
        let want = &case["expect"];
        assert_eq!(
            (got.x as i64, got.y as i64, got.pressed),
            (int(&want["x"]), int(&want["y"]), flag(want, "pressed")),
            "{case:?}"
        );
    }
}

#[test]
fn opto_relay_vectors() {
    let doc = load("opto_relay", &["encode", "decode"]);
    for case in cases(&doc, "encode") {
        check_encode(case, Ok(opto_relay::encode(flag(&case["args"], "on"))));
    }
    for case in cases(&doc, "decode") {
        let got = opto_relay::decode(&bytes(&case["input"]));
        assert_eq!(got, case["expect"].as_bool().expect("bool"), "{case:?}");
    }
}

#[test]
fn latch_relay_vectors() {
    let doc = load("latch_relay", &["encode", "decode"]);
    for case in cases(&doc, "encode") {
        check_encode(case, Ok(latch_relay::encode(flag(&case["args"], "on"))));
    }
    for case in cases(&doc, "decode") {
        let want = match case["expect"].as_str().expect("state name") {
            "on" => Some(true),
            "off" => Some(false),
            "unknown" => None,
            other => panic!("unknown state {other:?}"),
        };
        assert_eq!(
            latch_relay::decode(&bytes(&case["input"])),
            want,
            "{case:?}"
        );
    }
}

#[test]
fn pixels_vectors() {
    let doc = load("pixels", &["brightness_from_percent", "encode"]);
    for case in cases(&doc, "brightness_from_percent") {
        let got = pixels::brightness_from_percent(arg(case, "percent"));
        match case.get("error").and_then(Value::as_str) {
            Some("out_of_range") => assert_eq!(got, Err(OutOfRange), "{case:?}"),
            Some(other) => panic!("unknown error {other:?}"),
            None => assert_eq!(got.map(i64::from), Ok(int(&case["expect"])), "{case:?}"),
        }
    }
    for case in cases(&doc, "encode") {
        let mut leds = [Led::default(); LEDS];
        for led in case["leds"].as_array().expect("leds") {
            let byte = |k: &str| u8::try_from(int(&led[k])).expect("byte");
            leds[usize::try_from(int(&led["index"])).expect("index")] = Led {
                color: Color::new(byte("r"), byte("g"), byte("b")),
                brightness: byte("brightness"),
            };
        }
        check_encode(case, pixels::encode(&leds));
    }
}
