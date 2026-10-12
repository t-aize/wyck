# Releasing Wyck

Stable releases are built by GitHub Actions from tags named `vX.Y.Z`. The workflow rejects a tag
unless the package version in `Cargo.toml` is the same `X.Y.Z`. The first release made by this system
is `v0.2.0` (the version `Cargo.toml` already has); the older `v0.1.x` releases came from another
build chain.

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

1. Change `version` in the `[package]` table of `Cargo.toml`.
2. Run `cargo check` so `Cargo.lock` records the new version.
3. If a saved document changed layout since the last release, check that its `SCHEMA_VERSION`
   was raised and its step added in `src/app/migrations.rs` (a test fails otherwise), and that a
   fixture of the previous release's file is in `tests/fixtures`.
4. Run the checks below and inspect the About page in both debug and release builds (the version,
   the folders, and "Open the log folder").
5. Commit and push the version change.
6. Create and push the matching tag, for example `git tag -s v0.2.0` followed by
   `git push origin v0.2.0`.
7. Wait for all four package jobs and the final publish job to finish. Do not create the release
   by hand for the same tag.

Required checks:

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo deny check
cargo build --release --locked
```

The package matrix produces NSIS for Windows x86_64, DMG and updater archives for both macOS
architectures, and AppImage plus deb for Linux x86_64. `latest.json` points only to updater-safe
packages. It never points a deb installation at an in-place replacement.

## Verify a published release

Download all files, then verify their hashes:

```sh
gh release download v0.2.0 --repo t-aize/wyck --dir wyck-release
cd wyck-release
sha256sum -c SHA256SUMS
gh attestation verify . --repo t-aize/wyck
```

Also test a clean installation and an installation over an existing config on each operating
system. The installer replaces application files only. `AppPaths` data stays where it is; a
document saved by an older release is upgraded at the first start (see below).

For an updater test, serve a signed manifest from a temporary endpoint and check these cases:

- the current version produces no notification;
- a stable newer version is offered and a prerelease is ignored;
- a bad signature is refused;
- a valid update saves pending state, keeps a "Before an update" copy (Settings > Data and
  backup), installs and restarts;
- deb and raw Linux binaries open the GitHub release instead of replacing themselves;
- a download cut halfway, and a full disk, end in "Update check failed" with the old version
  still working.

For the data, start the new build on an install made by the previous release, and by the oldest
release still in use (a jump over several releases), and check:

- every document loads, nothing is reset, and `<name>.toml.v<n>.bak` holds the old text of each
  document that changed;
- starting again changes nothing (no new `.bak`, no new upgrade in the log);
- going back to the previous build sets the newer documents aside as `<name>.toml.from-v<n>`
  instead of reading them wrong, and restoring "Before an update" brings the old files back.

The log of the day (Settings > About > "Open the log folder") says what was upgraded.

With the data moved to a synced folder (Settings > Folders > "Use OneDrive"), check that:

- an update and a restart leave every kind where it was, and the update copy "Before an update"
  still lands in the backups folder in use;
- a second computer that picks the same folder uses what is in it and keeps its own files in
  `<name>.moved-<date>`;
- with the folder unplugged (pause the sync, or rename it), the app starts on the standard folders,
  says so, and goes back to the synced one when it is there again;
- a development build refuses the `Wyck` folder, and the installed app refuses `Wyck (dev)`.

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
