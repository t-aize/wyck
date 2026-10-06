# Risk register

| Risk | Probability | Impact | Mitigation | Fallback |
|---|---|---|---|---|
| Wire serde types (`Order`, `Position`, `Deal`) make the extraction of `wyck-trading` circular or invasive | medium | high | T-045 is a spike: move pure calculations first (`contract`, `guard`, `plan`), serde types second | Keep the types in `wyck-openapi`, record an ADR and a documented `arch-check` exception |
| Silent regression of the 50 studies during the move | medium | high | T-005 snapshots before any move; rerun after every study related task | `git revert` the move commit |
| Loss of saved user settings (study JSON, layouts, preferences) | medium | medium | Versioned migration and a read test on a captured old file (T-042, T-052, T-100) | Keep reading the old format |
| gpui-pre 0.3.5 or gpui-kit 0.6.6 changes under us (Dependabot proposes 0.7.0) | medium | medium | No bump during milestones; skill `bump-gpui` afterwards | Milestone tags |
| Splitting giant UI files breaks behavior with no GPUI test | high | medium | Extract pure logic first, add tests, take captures, one file per task | Revert per file |
| `cargo xtask check` too slow for editor hooks | low | low | Hooks run fmt and `cargo check -p` only; full check in CI | None needed |
| A real order sent during a test | low | very high | Live tests stay `#[ignore]` and need `WYCK_OPENAPI_ALLOW_LIVE_TRADING=1`; no automated test uses a real account | None |
| Behavior change hidden in a cleanup (for example `dash_pattern`, `compact`) | medium | low | Decide explicitly in the task, say so in the commit, snapshots cover studies | Revert the commit |
| Duplication and unused deps not yet measured | certain | low | T-003 and T-004 | None |
| Solo maintainer time | high | medium | Milestones of 1 to 3 weeks, S and M tasks, every milestone shippable | Stop after M4: the main gain is already in |
| Stricter live confirmation annoys during demo use | medium | low | Demo behavior unchanged (T-063) | Make the live rule a setting |
| CI red on a floating `stable` toolchain (clippy lint `approx_constant` already fails on 1.99.0, `extras.rs:594`) | certain | medium | Fix first in T-010; consider pinning the toolchain in `rust-toolchain.toml` and bumping it on purpose | Pin to 1.98.1 until the fix lands |
| Transient `PermissionDenied` on Windows when two threads replace or read the same config document (`fs_util.rs` rename), about 4 % of stress runs | medium | medium | T-033 retries on Windows only; the data is never torn, only a save or load returns an error | Revert the retry |
