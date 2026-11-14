//! Runs the language-neutral vectors in spec/vectors/ against the codecs.
//!
//! Each file holds one table per codec operation. A case has `input` or
//! `args`, and either `expect` or `error`. An encode case whose bytes start
//! with a firmware command prefix must say `node = "reserved_prefix"`, and
//! no other case may.

use modulino_driver::codec::joystick::{self, JoystickRaw};
use modulino_driver::codec::led_matrix::{self, COLS, ROWS};
use modulino_driver::codec::motors::{self, DecayMode};
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

/// Runs every file in spec/vectors/ through the runner for its kind, so a
/// file with no runner fails here rather than being skipped.
#[test]
fn every_vector_file_runs() {
    let dir = format!("{}/spec/vectors", env!("CARGO_MANIFEST_DIR"));
    let mut kinds: Vec<String> = std::fs::read_dir(&dir)
        .expect("spec/vectors")
        .map(|e| e.expect("entry").file_name().into_string().expect("utf-8"))
        .filter_map(|name| name.strip_suffix(".toml").map(str::to_owned))
        .collect();
    kinds.sort_unstable();
    assert!(!kinds.is_empty(), "no vector files in {dir}");
    for kind in kinds {
        let run: fn() = match kind.as_str() {
            "buttons" => buttons_vectors,
            "buzzer" => buzzer_vectors,
            "joystick" => joystick_vectors,
            "knob" => knob_vectors,
            "latch_relay" => latch_relay_vectors,
            "led_matrix" => led_matrix_vectors,
            "motors" => motors_vectors,
            "opto_relay" => opto_relay_vectors,
            "pixels" => pixels_vectors,
            "vibro" => vibro_vectors,
            other => panic!("spec/vectors/{other}.toml has no runner"),
        };
        run();
    }
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

fn led_matrix_vectors() {
    let doc = load(
        "led_matrix",
        &[
            "decode_mode",
            "encode_mode_command",
            "encode_mono",
            "encode_gray",
        ],
    );
    let mode = |name: &str| match name {
        "mono" => led_matrix::Mode::Mono,
        "grayscale" => led_matrix::Mode::Grayscale,
        other => panic!("unknown mode {other:?}"),
    };
    // A picture starting "GS4" or "MON" must be marked, and only those.
    let check_mode_marking = |case: &Value, frame: &[u8]| {
        let marked = case.get("device").and_then(Value::as_str) == Some("mode_command");
        assert_eq!(
            led_matrix::is_mode_command(frame),
            marked,
            "mode marking: {case:?}"
        );
    };
    for case in cases(&doc, "decode_mode") {
        let got = led_matrix::decode_mode(&bytes(&case["input"]));
        let want = match case["expect"].as_str().expect("mode name") {
            "unknown" => None,
            name => Some(mode(name)),
        };
        assert_eq!(got, want, "{case:?}");
    }
    for case in cases(&doc, "encode_mode_command") {
        let target = mode(case["args"]["target"].as_str().expect("target"));
        match arg::<usize>(case, "len") {
            12 => check_encode(case, Ok(led_matrix::encode_mode_command::<12>(target))),
            48 => check_encode(case, Ok(led_matrix::encode_mode_command::<48>(target))),
            other => panic!("no mode has length {other}"),
        }
    }
    for case in cases(&doc, "encode_mono") {
        let mut pixels = [[false; COLS]; ROWS];
        for p in case["pixels"].as_array().expect("pixels") {
            let at = |k: &str| usize::try_from(int(&p[k])).expect("index");
            pixels[at("row")][at("col")] = true;
        }
        let frame = led_matrix::encode_mono(&pixels);
        check_mode_marking(case, &frame);
        check_encode(case, Ok(frame));
    }
    for case in cases(&doc, "encode_gray") {
        let mut levels = [[0u8; COLS]; ROWS];
        for p in case["levels"].as_array().expect("levels") {
            let at = |k: &str| usize::try_from(int(&p[k])).expect("index");
            levels[at("row")][at("col")] = u8::try_from(int(&p["level"])).expect("level");
        }
        let got = led_matrix::encode_gray(&levels);
        if let Ok(frame) = &got {
            check_mode_marking(case, frame);
        }
        check_encode(case, got);
    }
}

fn motors_vectors() {
    let doc = load(
        "motors",
        &[
            "encode_mode",
            "encode_dc_speed",
            "encode_dc_frequency",
            "encode_stepper",
            "encode_decay",
            "encode_step_mode",
            "encode_half_full_scale",
            "decode_status",
            "current_ma",
        ],
    );
    let mode = |name: &str| match name {
        "dc" => motors::Mode::Dc,
        "stepper" => motors::Mode::Stepper,
        other => panic!("unknown mode {other:?}"),
    };
    let decay = |name: &str| match name {
        "slow" => DecayMode::Slow,
        "mixed_30_fast_70_slow" => DecayMode::Mixed30Fast70Slow,
        "mixed_60_fast_40_slow" => DecayMode::Mixed60Fast40Slow,
        "fast" => DecayMode::Fast,
        other => panic!("unknown decay {other:?}"),
    };
    let name = |case: &Value, key: &str| -> String {
        case["args"][key].as_str().expect("name").to_owned()
    };
    for case in cases(&doc, "encode_mode") {
        check_encode(case, Ok(motors::encode_mode(mode(&name(case, "mode")))));
    }
    for case in cases(&doc, "encode_dc_speed") {
        check_encode(
            case,
            motors::encode_dc_speed(arg(case, "a"), arg(case, "b")),
        );
    }
    for case in cases(&doc, "encode_dc_frequency") {
        check_encode(case, motors::encode_dc_frequency(arg(case, "hz")));
    }
    for case in cases(&doc, "encode_stepper") {
        let got = motors::encode_stepper(
            arg(case, "steps"),
            arg(case, "period_ticks"),
            arg(case, "release_delay_ms"),
        );
        check_encode(case, got);
    }
    for case in cases(&doc, "encode_decay") {
        check_encode(case, Ok(motors::encode_decay(decay(&name(case, "mode")))));
    }
    for case in cases(&doc, "encode_step_mode") {
        check_encode(
            case,
            Ok(motors::encode_step_mode(flag(&case["args"], "half_step"))),
        );
    }
    for case in cases(&doc, "encode_half_full_scale") {
        let got = motors::encode_half_full_scale(flag(&case["args"], "enabled"));
        check_encode(case, Ok(got));
    }
    for case in cases(&doc, "decode_status") {
        let got = motors::decode_status(&bytes(&case["input"]));
        let want = &case["expect"];
        let text = |k: &str| want[k].as_str().expect("name").to_owned();
        assert_eq!(
            (
                got.current_a_raw as i64,
                got.current_b_raw as i64,
                got.busy,
                got.mode,
                got.half_step,
                got.half_full_scale,
                got.decay,
                got.release_on_complete,
            ),
            (
                int(&want["current_a_raw"]),
                int(&want["current_b_raw"]),
                flag(want, "busy"),
                mode(&text("mode")),
                flag(want, "half_step"),
                flag(want, "half_full_scale"),
                decay(&text("decay")),
                flag(want, "release_on_complete"),
            ),
            "{case:?}"
        );
    }
    for case in cases(&doc, "current_ma") {
        let got = motors::current_ma(arg(case, "raw"), flag(&case["args"], "half_full_scale"));
        let want = case["expect"].as_float().expect("float");
        assert!(
            (got as f64 - want).abs() < 1e-3 * want.max(1.0),
            "{case:?}: {got}"
        );
    }
}
