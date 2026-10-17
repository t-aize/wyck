//! The colors of a script: a name, `#rrggbb` or `rrggbb` in, a number out, and back to text.

/// Colors by name, for `color: "orange"` in the options of a plot.
const NAMED: [(&str, u32); 14] = [
    ("white", 0xff_ffff),
    ("black", 0x00_0000),
    ("gray", 0x78_7b86),
    ("red", 0xef_5350),
    ("green", 0x26_a69a),
    ("blue", 0x29_62ff),
    ("orange", 0xff_9800),
    ("yellow", 0xff_eb3b),
    ("purple", 0x7e_57c2),
    ("teal", 0x00_bcd4),
    ("pink", 0xe9_1e63),
    ("lime", 0x9c_cc65),
    ("aqua", 0x00_e5ff),
    ("silver", 0xb2_b5be),
];

/// A color: `"#2962ff"`, `"2962ff"` or a name.
pub(crate) fn parse_color(text: &str) -> Option<u32> {
    let text = text.trim();
    if let Some((_, rgb)) = NAMED
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(text))
    {
        return Some(*rgb);
    }
    let hex = text.strip_prefix('#').unwrap_or(text);
    (hex.len() == 6 && hex.is_ascii())
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

/// The text of a color, `#rrggbb`.
pub(crate) fn color_text(color: u32) -> String {
    format!("#{color:06x}")
}
