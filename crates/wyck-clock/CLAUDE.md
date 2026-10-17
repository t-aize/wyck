@AGENTS.md

Before you say a change here is done, run `cargo test -p wyck-clock` and `cargo xtask deps`, and
say in your answer whether the change can make a time read differ in production (the default clock
must stay the system's).
