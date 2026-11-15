//! The public enums that can grow are `#[non_exhaustive]`: each match below
//! names every variant and still needs its `_` arm. Without the attribute
//! the `_` arm is an unreachable pattern, which `just check` rejects.
//! (`WriteLen` is covered by the match in `service_test.rs`.)

use modulino_driver::chips::vl53l4::Variant;
use modulino_driver::scan::Found;
use modulino_driver::spec::ChipId;

#[test]
fn found_needs_a_wildcard() {
    let name = |f: Found| match f {
        Found::Node(..) => "node",
        Found::Chip(..) => "chip",
        Found::Hub(_) => "hub",
        Found::Unknown(_) => "unknown",
        _ => "other",
    };
    let a = modulino_driver::Address::new(0x70).unwrap();
    assert_eq!(name(Found::Hub(a)), "hub");
}

#[test]
fn variant_needs_a_wildcard() {
    let name = |v: Variant| match v {
        Variant::Cd => "cd",
        Variant::Ed => "ed",
        _ => "other",
    };
    assert_eq!(name(Variant::Ed), "ed");
}

#[test]
fn chip_id_needs_a_wildcard() {
    let name = |id: ChipId| match id {
        ChipId::Register8 { .. } => "register8",
        ChipId::Register16 { .. } => "register16",
        ChipId::Sht3xStatus { .. } => "sht3x",
        ChipId::None => "none",
        _ => "other",
    };
    assert_eq!(name(ChipId::None), "none");
}
