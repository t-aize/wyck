use super::*;
#[test]
fn a_color_goes_to_hsv_and_back() {
    for color in [
        0x000000, 0xffffff, 0xff0000, 0x00ff00, 0x0000ff, 0x4f8dff, 0x123456,
    ] {
        let (h, s, v) = rgb_to_hsv(color);
        assert_eq!(hsv_to_rgb(h, s, v), color, "{color:06x}");
    }
    for color in PRESETS {
        let (h, s, v) = rgb_to_hsv(color);
        assert_eq!(hsv_to_rgb(h, s, v), color, "{color:06x}");
    }
}

#[test]
fn primary_colors_have_their_hues() {
    assert_eq!(rgb_to_hsv(0xff0000).0, 0.0);
    assert_eq!(rgb_to_hsv(0x00ff00).0, 120.0);
    assert_eq!(rgb_to_hsv(0x0000ff).0, 240.0);
    assert_eq!(rgb_to_hsv(0x808080).1, 0.0, "a gray has no saturation");
    assert_eq!(hsv_to_rgb(60.0, 1.0, 1.0), 0xffff00);
}

#[test]
fn hex_is_read_in_its_short_and_long_forms() {
    assert_eq!(parse_hex("#ff8800"), Some(0xff8800));
    assert_eq!(parse_hex("FF8800"), Some(0xff8800));
    assert_eq!(parse_hex("f80"), Some(0xff8800));
    assert_eq!(parse_hex(" #abc "), Some(0xaabbcc));
    assert_eq!(parse_hex("ff88"), None);
    assert_eq!(parse_hex("gg0000"), None);
    assert_eq!(parse_hex(""), None);
    assert_eq!(hex_text(0x00ff88), "00ff88");
}

#[test]
fn a_channel_replaces_only_its_own_part() {
    assert_eq!(Field::Red.parse("18", 0x112233), Some(0x122233));
    assert_eq!(Field::Green.parse("255", 0x112233), Some(0x11ff33));
    assert_eq!(Field::Blue.parse("0", 0x112233), Some(0x112200));
    assert_eq!(Field::Blue.parse("256", 0x112233), None);
    assert_eq!(Field::Red.parse("", 0x112233), None);
    assert_eq!(Field::Hex.parse("#000", 0x112233), Some(0));
    assert_eq!(Field::Green.text(0x112233), "34");
}

#[test]
fn the_presets_have_no_repeats() {
    let mut seen = std::collections::HashSet::new();
    assert!(PRESETS.iter().all(|c| seen.insert(*c)));
}
