//! The setups of the ticket saved under a name, and the time stop: reading the ticket into a
//! [`PlanTemplate`], and filling the ticket from one. What a template holds is described in
//! [`super::prefs`].

use super::prefs::{Distance, PlanTemplate, TimeStopPrefs};
use super::*;

impl OrderTicket {
    /// The time stop as typed now.
    pub(super) fn time_stop_now(&self, cx: &App) -> TimeStopPrefs {
        let mut stop = self.time_stop;
        stop.amount = Self::read(&self.time_stop_amount, cx).unwrap_or(stop.amount);
        stop.normalized()
    }

    /// Changes the time stop (its switch, its unit, which positions), and remembers it.
    pub(super) fn edit_time_stop(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut TimeStopPrefs),
    ) {
        let mut stop = self.time_stop_now(cx);
        change(&mut stop);
        self.time_stop = stop;
        self.settings_changed(cx);
        cx.notify();
    }

    /// The ticket as it is now, under a name. A protection given as a price is not kept, and
    /// neither is an ATR stop: they hold for one moment and one chart.
    pub(super) fn template(&self, name: &str, cx: &App) -> PlanTemplate {
        let distance = |on: bool, unit: Offset, state: &Entity<InputState>| {
            if !on || unit == Offset::Price {
                return None;
            }
            Self::read(state, cx).map(|value| Distance { unit, value })
        };
        PlanTemplate {
            name: name.to_owned(),
            size_mode: self.size_mode,
            size: Self::read(&self.size, cx).unwrap_or(0.0),
            stop: distance(
                self.stop_on && !self.stop_atr,
                self.stop_unit,
                &self.stop_loss,
            ),
            target: distance(self.target_on, self.target_unit, &self.take_profit),
            exits: self.exits_now(cx),
            time_stop: self.time_stop_now(cx),
        }
        .normalized()
    }

    /// Saves the ticket under the name typed. Says what happened.
    pub(super) fn save_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = prefs::clean_name(&self.plan_name.read(cx).value());
        let template = self.template(&name, cx);
        let mut prefs = self.prefs(cx);
        match prefs.save_plan(template) {
            Some(replaced) => {
                self.plans = prefs.plans;
                self.write(&self.plan_name.clone(), String::new(), window, cx);
                self.settings_changed(cx);
                crate::ui::kit::toast::Toast::success(
                    if replaced {
                        "Plan updated"
                    } else {
                        "Plan saved"
                    },
                    format!("\"{name}\" is in the list of saved plans."),
                )
                .sticky(false)
                .show(cx);
            }
            None if name.is_empty() => {
                crate::ui::kit::toast::Toast::warning("Name the plan", "Type a name for it first.")
                    .sticky(false)
                    .show(cx);
            }
            None => {
                crate::ui::kit::toast::Toast::warning(
                    "The list of plans is full",
                    format!(
                        "At most {} plans can be saved. Delete one, or use the name of one to replace it.",
                        prefs::MAX_PLANS
                    ),
                )
                .show(cx);
            }
        }
        cx.notify();
    }

    /// Forgets a saved plan.
    pub(super) fn delete_template(&mut self, name: &str, cx: &mut Context<Self>) {
        let mut prefs = self.prefs(cx);
        if prefs.remove_plan(name) {
            self.plans = prefs.plans;
        }
        self.settings_changed(cx);
        cx.notify();
    }

    /// Fills the ticket from a saved plan: the size, the stop loss and take profit as distances,
    /// the exits and the time stop. The symbol, the side, the kind of order and its price are left
    /// as they are.
    pub(super) fn apply_template(
        &mut self,
        template: &PlanTemplate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A ticket that follows a drawing takes its prices from it.
        if self.link.is_some() {
            crate::ui::kit::toast::Toast::info(
                "A drawing is followed",
                "Release the position drawing to apply a plan.",
            )
            .sticky(false)
            .show(cx);
            return;
        }
        let template = template.clone().normalized();
        self.size_mode = template.size_mode;
        self.write(
            &self.size.clone(),
            number::format(
                template.size,
                if template.size_mode == SizeMode::Units {
                    0
                } else {
                    2
                },
            ),
            window,
            cx,
        );
        self.stop_atr = false;
        match template.stop {
            Some(d) => {
                self.stop_on = true;
                self.stop_unit = d.unit;
                self.write(
                    &self.stop_loss.clone(),
                    format_offset(d.unit, d.value),
                    window,
                    cx,
                );
            }
            None => {
                self.stop_on = false;
                self.trailing = false;
                self.guaranteed = false;
                self.write(&self.stop_loss.clone(), String::new(), window, cx);
            }
        }
        match template.target {
            Some(d) => {
                self.target_on = true;
                self.target_unit = d.unit;
                self.write(
                    &self.take_profit.clone(),
                    format_offset(d.unit, d.value),
                    window,
                    cx,
                );
            }
            None => {
                self.target_on = false;
                self.write(&self.take_profit.clone(), String::new(), window, cx);
            }
        }
        // The exits: the legs go into their inputs, the rest into the plan.
        for (index, (share, target)) in self.leg_inputs.iter().enumerate() {
            let leg = template
                .exits
                .legs
                .get(index)
                .copied()
                .unwrap_or(plan::Leg {
                    share: 0.0,
                    target_r: 0.0,
                });
            let (share, target) = (share.clone(), target.clone());
            self.write(&share, number::format(leg.share, 2), window, cx);
            self.write(&target, number::format(leg.target_r, 2), window, cx);
        }
        self.write(
            &self.be_offset.clone(),
            number::format(template.exits.break_even.offset_pips, 2),
            window,
            cx,
        );
        self.write(
            &self.oco_pips.clone(),
            number::format(template.exits.oco_pips, 2),
            window,
            cx,
        );
        self.exits = template.exits.clone();
        self.time_stop = template.time_stop;
        self.write(
            &self.time_stop_amount.clone(),
            number::format(template.time_stop.amount, 0),
            window,
            cx,
        );
        self.apply_steps(window, cx);
        self.settings_changed(cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::prefs::{Distance, PlanTemplate, TicketPrefs, TimeStopPrefs};
    use super::*;
    use crate::trading::plan::Only;

    fn plan(name: &str) -> PlanTemplate {
        PlanTemplate {
            name: name.to_owned(),
            size_mode: SizeMode::RiskBalance,
            size: 1.0,
            stop: Some(Distance {
                unit: Offset::Pips,
                value: 20.0,
            }),
            target: Some(Distance {
                unit: Offset::Ratio,
                value: 2.0,
            }),
            ..PlanTemplate::default()
        }
    }

    #[test]
    fn a_plan_is_saved_replaced_and_removed_by_name() {
        let mut prefs = TicketPrefs::default();
        assert_eq!(prefs.save_plan(plan("Scalp")), Some(false));
        assert_eq!(prefs.save_plan(plan("Swing")), Some(false));
        // The same name in another case replaces it, in place.
        let mut again = plan("scalp");
        again.size = 2.0;
        assert_eq!(prefs.save_plan(again), Some(true));
        assert_eq!(prefs.plans.len(), 2);
        assert_eq!(prefs.plans[0].name, "scalp");
        assert_eq!(prefs.plans[0].size, 2.0);
        // No name, no plan.
        assert_eq!(prefs.save_plan(plan("   ")), None);
        assert!(prefs.remove_plan("Swing"));
        assert!(!prefs.remove_plan("Swing"));
        assert_eq!(prefs.plans.len(), 1);
    }

    #[test]
    fn the_list_of_plans_has_a_limit() {
        let mut prefs = TicketPrefs::default();
        for n in 0..prefs::MAX_PLANS {
            assert_eq!(prefs.save_plan(plan(&format!("plan {n}"))), Some(false));
        }
        assert_eq!(prefs.save_plan(plan("one more")), None);
        // A name already there still replaces.
        assert_eq!(prefs.save_plan(plan("PLAN 3")), Some(true));
        assert_eq!(prefs.plans.len(), prefs::MAX_PLANS);
    }

    #[test]
    fn plans_from_a_file_are_repaired() {
        let mut prefs = TicketPrefs::default();
        let mut wild = plan("  Wide   name  ");
        wild.stop = Some(Distance {
            unit: Offset::Price,
            value: 1.1,
        });
        wild.target = Some(Distance {
            unit: Offset::Pips,
            value: f64::NAN,
        });
        wild.size = -3.0;
        prefs.plans = vec![wild, plan("Wide name"), plan(""), plan(&"x".repeat(200))];
        let prefs = prefs.normalized();
        assert_eq!(
            prefs.plans.len(),
            2,
            "a repeated name and an empty one are dropped"
        );
        assert_eq!(prefs.plans[0].name, "Wide name");
        assert_eq!(prefs.plans[0].stop, None, "a price is not kept");
        assert_eq!(prefs.plans[0].target, None);
        assert_eq!(prefs.plans[0].size, 0.1);
        assert_eq!(prefs.plans[1].name.chars().count(), prefs::MAX_PLAN_NAME);
    }

    #[test]
    fn a_stop_that_depends_on_the_volume_cannot_size_the_volume_by_risk() {
        let mut template = plan("Risky");
        template.stop = Some(Distance {
            unit: Offset::Money,
            value: 50.0,
        });
        assert_eq!(
            template.normalized().stop.map(|d| d.unit),
            Some(Offset::Pips)
        );
    }

    #[test]
    fn a_time_stop_is_kept_in_whole_units_and_within_a_year() {
        let mut stop = TimeStopPrefs {
            on: true,
            amount: 2.6,
            ..TimeStopPrefs::default()
        };
        assert_eq!(stop.normalized().amount, 3.0);
        assert_eq!(stop.normalized().rule().map(|r| r.minutes), Some(180));
        stop.amount = f64::NAN;
        assert_eq!(stop.normalized().amount, 4.0);
        stop.amount = 1.0e12;
        stop.span = prefs::Span::Days;
        assert_eq!(
            stop.normalized().minutes(),
            crate::trading::plan::MAX_TIME_STOP_MINUTES
        );
        assert_eq!(TimeStopPrefs::default().rule(), None);
        stop.only = Only::Losing;
        assert_eq!(stop.rule().map(|r| r.only), Some(Only::Losing));
    }

    #[test]
    fn a_plan_reads_in_a_line() {
        let mut template = plan("Swing");
        template.exits.on = true;
        template.time_stop = TimeStopPrefs {
            on: true,
            amount: 4.0,
            span: prefs::Span::Hours,
            only: Only::Winning,
        };
        assert_eq!(
            template.summary(),
            "1% risk, stop 20 pips, target 2R, 3 exits, close after 4 h if in profit"
        );
    }

    #[test]
    fn prefs_from_before_plans_load_with_none() {
        let old: TicketPrefs = toml::from_str("size = 0.5").unwrap();
        assert!(old.plans.is_empty());
        assert!(!old.time_stop.on);
        let with: TicketPrefs = toml::from_str(
            &toml::to_string(&{
                let mut p = TicketPrefs::default();
                p.save_plan(plan("Scalp"));
                p.time_stop.on = true;
                p
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(with.plans.len(), 1);
        assert!(with.time_stop.on);
    }
}
