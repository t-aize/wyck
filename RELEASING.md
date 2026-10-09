# Releasing Wyck

Stable releases are built by GitHub Actions from tags named `vX.Y.Z`. The workflow rejects a tag
unless every workspace crate has the same `X.Y.Z` version. The first release made by this system
is `v0.3.0`; the existing `v0.2.0` tag is not changed.

## One-time repository setup

The updater signing pair is generated with:

```sh
cargo packager signer generate
```

Commit only the public key in `assets/update.pubkey`. Store the encoded private key
and its password as these GitHub Actions secrets:

- `CARGO_PACKAGER_SIGN_PRIVATE_KEY`
- `CARGO_PACKAGER_SIGN_PRIVATE_KEY_PASSWORD`

The release jobs stop before packaging when either secret is absent. Keep a secure backup of the
private key and password. Replacing the public key makes existing installations unable to accept
new updates signed by the old key.

Enable immutable releases in the GitHub repository settings when available. The workflow creates
a draft, uploads and attests all assets, and only then publishes it. This follows GitHub's
[immutable release guidance](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases).

## Create a release

1. Change `[workspace.package].version` and every internal dependency version in `Cargo.toml`.
2. Run `cargo check --workspace` so `Cargo.lock` records the new workspace versions.
3. Run the checks below and inspect the About page in both debug and release builds.
4. Commit and push the version change.
5. Create and push the matching tag, for example `git tag -s v0.3.0` followed by
   `git push origin v0.3.0`.
6. Wait for all four package jobs and the final publish job to finish. Do not create the release
   by hand for the same tag.

Required checks:

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo deny check
cargo build --release --locked
```

The package matrix produces NSIS for Windows x86_64, DMG and updater archives for both macOS
architectures, and AppImage plus deb for Linux x86_64. `latest.json` points only to updater-safe
packages. It never points a deb installation at an in-place replacement.

## Verify a published release

Download all files, then verify their hashes:

```sh
gh release download v0.3.0 --repo t-aize/wyck --dir wyck-release
cd wyck-release
sha256sum -c SHA256SUMS
gh attestation verify . --repo t-aize/wyck
```

Also test a clean installation and an installation over an existing config on each operating
system. The installer replaces application files only. `AppPaths` data and the current schema
version stay unchanged.

For an updater test, serve a signed manifest from a temporary endpoint and check these cases:

- the current version produces no notification;
- a stable newer version is offered and a prerelease is ignored;
- a bad signature is refused;
- a valid update saves pending state, installs and restarts;
- deb and raw Linux binaries open the GitHub release instead of replacing themselves.

## OS code signing integration

The initial release is intentionally unsigned at the operating-system level. cargo-packager update
signatures still protect the automatic updater, but they do not remove SmartScreen or Gatekeeper
warnings.

For Windows, add an Authenticode certificate secret and configure `certificate-thumbprint`,
`digest-algorithm = "sha256"` and a timestamp URL in
`[package.metadata.packager.windows]`. Import the certificate into the release runner before the
package step. Microsoft recommends signing public installers.

For macOS, add the Developer ID certificate as `APPLE_CERTIFICATE` and its password as
`APPLE_CERTIFICATE_PASSWORD`, set `signing-identity` in `[package.metadata.packager.macos]`, and
provide notarization credentials supported by cargo-packager. Apple recommends Developer ID
signing and notarization for software distributed outside the App Store.

Do not publish signed installers until both signing paths have been tested on clean machines.
