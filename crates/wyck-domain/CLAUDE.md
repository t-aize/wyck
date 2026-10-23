@AGENTS.md

This crate is the domain of a trading app. Before you say a change here is done, run
`cargo test -p wyck-domain` and `cargo xtask deps`, and say in your answer whether the change:

- alters a saved form (a `Tool` or `ChartKind` code, an indicator or study field, a drawing field, a
  default, `Timeframe::code`, a serde field name), because the user's files are read with it;
- can alter the size, the price or the protection of an order (`trading`: sizing, margin, the guard);
- widens what a script can reach (`indicators::custom`), or the cost of a frame on a large series
  (`chart`);
- changes a signature of `base`, which everything above uses.
