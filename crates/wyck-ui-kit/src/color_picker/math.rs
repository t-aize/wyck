//! The colors of the picker, and the conversions between hexadecimal text, RGB and HSV.

use wyck_core::math::f32_to_u32;

/// The colors of the theme offered to click: a row of grays, then eight hues in four shades.
pub(crate) const PRESETS: [u32; 40] = [
    0xff_ffff, 0xd1_d4dc, 0x95_98a1, 0x78_7b86, 0x5d_606b, 0x43_4651, 0x2a_2e39, 0x00_0000,
    0xf2_3645, 0xff_9800, 0xff_eb3b, 0x4c_af50, 0x08_9981, 0x00_bcd4, 0x29_62ff, 0x9c_27b0,
    0xfc_cbcd, 0xff_e0b2, 0xff_f9c4, 0xc8_e6c9, 0xac_e5dc, 0xb2_ebf2, 0xbb_d9fb, 0xe1_bee7,
    0xf7_525f, 0xff_b74d, 0xff_f176, 0x81_c784, 0x22_ab94, 0x4d_d0e1, 0x5b_9cf6, 0xba_68c8,
    0xb2_2833, 0xf5_7c00, 0xfb_c02d, 0x38_8e3c, 0x05_6656, 0x00_97a7, 0x18_48cc, 0x7b_1fa2,
];

/// The hue (0 to 360), saturation and value (0 to 1) of a color.
#[expect(
    clippy::float_cmp,
    reason = "`max` is one of `r`, `g` and `b` picked with `f32::max`, so the comparison is exact"
)]
pub(crate) fn rgb_to_hsv(color: u32) -> (f32, f32, f32) {
    let [_, red, green, blue] = color.to_be_bytes();
    let r = f32::from(red) / 255.0;
    let g = f32::from(green) / 255.0;
    let b = f32::from(blue) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta < 1e-6 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max < 1e-6 { 0.0 } else { delta / max };
    (hue, saturation, max)
}

/// The color of a hue (0 to 360), saturation and value (0 to 1).
pub(crate) fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> u32 {
    let sector = hue.rem_euclid(360.0) / 60.0;
    let saturation = saturation.clamp(0.0, 1.0);
    let value = value.clamp(0.0, 1.0);
    let chroma = value * saturation;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let floor = value - chroma;
    let (r, g, b) = match f32_to_u32(sector) {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let byte = |channel: f32| f32_to_u32(((channel + floor) * 255.0).round().clamp(0.0, 255.0));
    (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

/// `#rrggbb` as text.
pub(crate) fn hex_text(color: u32) -> String {
    format!("{:06x}", color & 0xff_ffff)
}

/// A color written as `rgb`, `#rgb`, `rrggbb` or `#rrggbb`.
pub(crate) fn parse_hex(text: &str) -> Option<u32> {
    let digits = text.trim().trim_start_matches('#');
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match digits.len() {
        3 => {
            let mut color = 0;
            for c in digits.chars() {
                let d = c.to_digit(16)?;
                color = (color << 8) | (d << 4) | d;
            }
            Some(color)
        }
        6 => u32::from_str_radix(digits, 16).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color_picker::Field;

    #[test]
    fn a_color_goes_to_hsv_and_back() {
        for color in [
            0x00_0000, 0xff_ffff, 0xff_0000, 0x00_ff00, 0x00_00ff, 0x4f_8dff, 0x12_3456,
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
        assert_eq!(rgb_to_hsv(0xff_0000).0, 0.0);
        assert_eq!(rgb_to_hsv(0x00_ff00).0, 120.0);
        assert_eq!(rgb_to_hsv(0x00_00ff).0, 240.0);
        assert_eq!(rgb_to_hsv(0x80_8080).1, 0.0, "a gray has no saturation");
        assert_eq!(hsv_to_rgb(60.0, 1.0, 1.0), 0xff_ff00);
    }

    #[test]
    fn hex_is_read_in_its_short_and_long_forms() {
        assert_eq!(parse_hex("#ff8800"), Some(0xff_8800));
        assert_eq!(parse_hex("FF8800"), Some(0xff_8800));
        assert_eq!(parse_hex("f80"), Some(0xff_8800));
        assert_eq!(parse_hex(" #abc "), Some(0xaa_bbcc));
        assert_eq!(parse_hex("ff88"), None);
        assert_eq!(parse_hex("gg0000"), None);
        assert_eq!(parse_hex(""), None);
        assert_eq!(hex_text(0x00_ff88), "00ff88");
    }

    #[test]
    fn a_channel_replaces_only_its_own_part() {
        assert_eq!(Field::Red.parse("18", 0x11_2233), Some(0x12_2233));
        assert_eq!(Field::Green.parse("255", 0x11_2233), Some(0x11_ff33));
        assert_eq!(Field::Blue.parse("0", 0x11_2233), Some(0x11_2200));
        assert_eq!(Field::Blue.parse("256", 0x11_2233), None);
        assert_eq!(Field::Red.parse("", 0x11_2233), None);
        assert_eq!(Field::Hex.parse("#000", 0x11_2233), Some(0));
        assert_eq!(Field::Green.text(0x11_2233), "34");
    }
}
