# Releasing wyck

Releases are built by GitHub Actions from tags named `vX.Y.Z`. The workflow rejects a tag that
does not match the version in `Cargo.toml`.

## Create a release

1. Change `version` in `Cargo.toml` and run `cargo check` so `Cargo.lock` follows.
2. Run the checks:

   ```sh
   cargo fmt --check
   cargo clippy --locked --all-targets -- -D warnings
   cargo test --locked
   cargo deny check
   cargo build --release --locked
   ```

3. Commit and push the version change.
4. Tag and push: `git tag -s v0.3.0` then `git push origin v0.3.0`.
5. Wait for the four package jobs and the publish job. Do not create the release by hand.

The workflow builds a `.tar.gz` (Linux, macOS) or `.zip` (Windows) per platform with the binary,
`LICENSE` and `README.md`, a `SHA256SUMS` file, and a GitHub artifact attestation. It creates a
draft release, uploads everything, then publishes it.

## Verify a published release

```sh
gh release download v0.3.0 --repo t-aize/wyck --dir wyck-release
cd wyck-release
sha256sum -c SHA256SUMS
gh attestation verify wyck_0.3.0_linux-x86_64.tar.gz --repo t-aize/wyck
```

The binaries are not signed for macOS Gatekeeper or Windows SmartScreen, which may warn on first
run.
