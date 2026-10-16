@AGENTS.md

This crate is where an order leaves the interface. Before you say a change here is done, run
`cargo test -p wyck-ui-trading` and `cargo xtask deps`, and say whether the path from a click to
`place_batch` (the plan, the safety check, the confirmation) is the same as before.
