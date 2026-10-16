//! Ready-made candle colors, offered on the Charts page of the settings.

/// A name, and the color of a rising and of a falling candle.
pub const CANDLE_SETS: &[(&str, u32, u32)] = &[
    ("Classic", 0x26_a69a, 0xef_5350),
    ("Green and red", 0x22_c55e, 0xef_4444),
    ("Blue and orange", 0x3b_82f6, 0xf9_7316),
    ("Blue and red", 0x29_62ff, 0xf2_3645),
    ("Teal and pink", 0x14_b8a6, 0xec_4899),
    ("Gold and violet", 0xea_b308, 0x8b_5cf6),
    ("Monochrome", 0xe5_e5e5, 0x6b_6b6b),
    ("Neon", 0x00_ff9c, 0xff_2e63),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_candle_sets_have_two_different_colors() {
        for (name, up, down) in CANDLE_SETS {
            assert_ne!(up, down, "{name}");
        }
    }

    #[test]
    fn every_candle_set_has_its_own_name() {
        let mut names: Vec<&str> = CANDLE_SETS.iter().map(|(name, _, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CANDLE_SETS.len());
    }
}
