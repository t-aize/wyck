//! The timeframes of the header: the favorites shown as buttons and the ones the user added to
//! the menu.

use super::Preferences;
use wyck_market::{QUICK, Timeframe};

pub(super) fn default_favorite_timeframes() -> Vec<String> {
    QUICK.iter().map(|timeframe| timeframe.code()).collect()
}

impl Preferences {
    /// Repairs both lists: each custom timeframe once under its own code, shortest first, and a
    /// favorite that is neither offered nor added becomes an added one, so its star has a home.
    pub(super) fn normalize_timeframes(&mut self) {
        let mut favorites: Vec<Timeframe> = self
            .favorite_timeframes
            .iter()
            .filter_map(|code| Timeframe::from_code(code))
            .collect();
        let mut customs: Vec<Timeframe> = self
            .custom_timeframes
            .iter()
            .filter_map(|code| Timeframe::from_code(code))
            .chain(favorites.iter().copied())
            .filter(|timeframe| !timeframe.is_builtin())
            .collect();
        for list in [&mut favorites, &mut customs] {
            list.sort_by_key(|timeframe| timeframe.sort_key());
            list.dedup();
        }
        if favorites.is_empty() {
            favorites = QUICK.to_vec();
        }
        self.favorite_timeframes = favorites.iter().map(|t| t.code()).collect();
        self.custom_timeframes = customs.iter().map(|t| t.code()).collect();
    }

    /// The favorite timeframes, in menu order.
    pub fn favorites(&self) -> Vec<Timeframe> {
        self.favorite_timeframes
            .iter()
            .filter_map(|code| Timeframe::from_code(code))
            .collect()
    }

    pub fn is_favorite_timeframe(&self, timeframe: Timeframe) -> bool {
        self.favorite_timeframes.contains(&timeframe.code())
    }

    /// Adds the timeframe to the favorites, or removes it. The last favorite cannot be removed:
    /// the header would be left with no buttons at all.
    pub fn toggle_favorite_timeframe(&mut self, timeframe: Timeframe) {
        let code = timeframe.code();
        if let Some(at) = self.favorite_timeframes.iter().position(|c| *c == code) {
            if self.favorite_timeframes.len() > 1 {
                self.favorite_timeframes.remove(at);
            }
        } else {
            self.favorite_timeframes.push(code);
            *self = std::mem::take(self).normalized();
        }
    }

    /// The timeframes the user added, shortest first.
    pub fn customs(&self) -> Vec<Timeframe> {
        self.custom_timeframes
            .iter()
            .filter_map(|code| Timeframe::from_code(code))
            .collect()
    }

    /// Adds a timeframe to the menu, unless it is there already.
    pub fn add_custom_timeframe(&mut self, timeframe: Timeframe) {
        if timeframe.is_builtin() || self.customs().contains(&timeframe) {
            return;
        }
        self.custom_timeframes.push(timeframe.code());
        *self = std::mem::take(self).normalized();
    }

    /// Takes an added timeframe out of the menu, and off the header. The last favorite stays,
    /// as with [`Self::toggle_favorite_timeframe`].
    pub fn remove_custom_timeframe(&mut self, timeframe: Timeframe) {
        let code = timeframe.code();
        if self.is_favorite_timeframe(timeframe) {
            if self.favorite_timeframes.len() == 1 {
                return;
            }
            self.favorite_timeframes.retain(|c| *c != code);
        }
        self.custom_timeframes.retain(|c| *c != code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyck_market::Period;

    #[test]
    fn favorite_timeframes_keep_menu_order_and_never_run_out() {
        let mut prefs = Preferences {
            favorite_timeframes: vec!["D1".into(), "M1".into()],
            ..Preferences::default()
        }
        .normalized();
        assert_eq!(prefs.favorite_timeframes, vec!["M1", "D1"]);

        prefs.toggle_favorite_timeframe(Timeframe::Seconds(5));
        assert_eq!(prefs.favorite_timeframes, vec!["S5", "M1", "D1"]);
        prefs.toggle_favorite_timeframe(Timeframe::Bars(Period::M1));
        prefs.toggle_favorite_timeframe(Timeframe::Seconds(5));
        assert_eq!(prefs.favorite_timeframes, vec!["D1"]);
        prefs.toggle_favorite_timeframe(Timeframe::Bars(Period::D1));
        assert_eq!(
            prefs.favorite_timeframes,
            vec!["D1"],
            "the last one stays, so the header keeps a button"
        );
        assert!(prefs.is_favorite_timeframe(Timeframe::Bars(Period::D1)));
    }

    #[test]
    fn custom_timeframes_are_kept_once_and_can_be_starred() {
        let mut prefs = Preferences::default().normalized();
        let h2 = Timeframe::from_code("H2").unwrap();
        let m7 = Timeframe::from_code("M7").unwrap();
        prefs.add_custom_timeframe(m7);
        prefs.add_custom_timeframe(Timeframe::from_code("120m").unwrap());
        prefs.add_custom_timeframe(h2);
        // H2 is offered by the menu already; M7 is the one added.
        assert_eq!(prefs.custom_timeframes, vec!["M7"]);
        prefs.add_custom_timeframe(Timeframe::from_code("M9").unwrap());
        assert_eq!(prefs.custom_timeframes, vec!["M7", "M9"]);
        prefs.toggle_favorite_timeframe(m7);
        assert!(prefs.favorite_timeframes.contains(&"M7".to_owned()));
        // Saved and read back, it keeps its place in the header and the menu.
        let text = toml::to_string(&prefs).unwrap();
        let back: Preferences = toml::from_str(&text).unwrap();
        let back = back.normalized();
        assert_eq!(
            back.customs(),
            vec![m7, Timeframe::from_code("M9").unwrap()]
        );
        assert!(back.favorites().contains(&m7));
        // Taking it out of the menu takes it off the header too.
        prefs.remove_custom_timeframe(m7);
        assert_eq!(prefs.custom_timeframes, vec!["M9"]);
        assert!(!prefs.is_favorite_timeframe(m7));
        // A favorite from nowhere (a file written by hand) is added to the menu.
        let prefs = Preferences {
            favorite_timeframes: vec!["M1".into(), "M13".into()],
            ..Preferences::default()
        }
        .normalized();
        assert_eq!(prefs.custom_timeframes, vec!["M13"]);
    }

    #[test]
    fn favorites_of_nothing_fall_back_to_the_defaults() {
        let prefs = Preferences {
            favorite_timeframes: vec![],
            ..Preferences::default()
        };
        assert_eq!(prefs.normalized().favorites(), QUICK.to_vec());
    }
}
