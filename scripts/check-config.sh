#!/usr/bin/env bash
# Everything that keeps the wyck-config crate honest, in the order CI runs it.
#
#   scripts/check-config.sh           format, lints, tests, docs
#   scripts/check-config.sh doctor    look over the config of this machine (changes nothing)
#   scripts/check-config.sh doctor --dir ./wyck-data
#
# The tests never touch the real config: they work in temporary folders.
set -euo pipefail
cd "$(dirname "$0")/.."

CRATE=wyck-config

case "${1:-check}" in
  check)
    cargo fmt --all --check
    cargo clippy -p "$CRATE" --all-targets -- -D warnings
    cargo test -p "$CRATE"
    RUSTDOCFLAGS="-D warnings" cargo doc -p "$CRATE" --no-deps
    echo "$CRATE: all checks passed"
    ;;
  doctor)
    shift
    cargo run -q -p "$CRATE" --example config_doctor -- "$@"
    ;;
  *)
    echo "usage: $0 [check | doctor [--dir <folder>]]" >&2
    exit 2
    ;;
esac
