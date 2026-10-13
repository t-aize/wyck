//! Ready-made candle colors, offered on the Charts page of the settings.

/// A name, and the color of a rising and of a falling candle.
pub const CANDLE_SETS: &[(&str, u32, u32)] = &[
    ("Classic", 0x26a69a, 0xef5350),
    ("Green and red", 0x22c55e, 0xef4444),
    ("Blue and orange", 0x3b82f6, 0xf97316),
    ("Blue and red", 0x2962ff, 0xf23645),
    ("Teal and pink", 0x14b8a6, 0xec4899),
    ("Gold and violet", 0xeab308, 0x8b5cf6),
    ("Monochrome", 0xe5e5e5, 0x6b6b6b),
    ("Neon", 0x00ff9c, 0xff2e63),
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
