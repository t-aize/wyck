//! The colors of the picker, and the conversions between hexadecimal text, RGB and HSV.

/// The colors of the theme offered to click: a row of grays, then eight hues in four shades.
pub const PRESETS: [u32; 40] = [
    0xffffff, 0xd1d4dc, 0x9598a1, 0x787b86, 0x5d606b, 0x434651, 0x2a2e39, 0x000000, 0xf23645,
    0xff9800, 0xffeb3b, 0x4caf50, 0x089981, 0x00bcd4, 0x2962ff, 0x9c27b0, 0xfccbcd, 0xffe0b2,
    0xfff9c4, 0xc8e6c9, 0xace5dc, 0xb2ebf2, 0xbbd9fb, 0xe1bee7, 0xf7525f, 0xffb74d, 0xfff176,
    0x81c784, 0x22ab94, 0x4dd0e1, 0x5b9cf6, 0xba68c8, 0xb22833, 0xf57c00, 0xfbc02d, 0x388e3c,
    0x056656, 0x0097a7, 0x1848cc, 0x7b1fa2,
];

/// The hue (0 to 360), saturation and value (0 to 1) of a color.
pub fn rgb_to_hsv(color: u32) -> (f32, f32, f32) {
    let r = ((color >> 16) & 0xff) as f32 / 255.0;
    let g = ((color >> 8) & 0xff) as f32 / 255.0;
    let b = (color & 0xff) as f32 / 255.0;
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
pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> u32 {
    let h = hue.rem_euclid(360.0) / 60.0;
    let s = saturation.clamp(0.0, 1.0);
    let v = value.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |channel: f32| ((channel + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

/// `#rrggbb` as text.
pub fn hex_text(color: u32) -> String {
    format!("{:06x}", color & 0xff_ffff)
}

/// A color written as `rgb`, `#rgb`, `rrggbb` or `#rrggbb`.
pub fn parse_hex(text: &str) -> Option<u32> {
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
