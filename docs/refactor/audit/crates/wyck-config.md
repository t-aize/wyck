# wyck-config

- Real role: native configuration, encrypted credential storage, document store and portable backups. Matches the name; two cTrader and chart specific leaks (below).
- Type: lib, edition 2024, no features, examples `config_backup` and `config_doctor`.
- Internal dependencies: none. Used by `wyck` and `wyck-chart`.
- External dependencies: `argon2`, `chacha20poly1305`, `directories`, `getrandom`, `keyring`, `secrecy`, `serde`, `serde_json`, `thiserror`, `toml`, `tracing`, `uuid`, `zeroize`; dev: `proptest`, `tempfile`. None looks unused (not machine checked).
- Size: 20 files, 6,756 lines (src 6,069). Largest: `backup.rs` 1565 (about 1037 before tests), `lib.rs` 880 (about 520 before tests), `secret/file_store.rs` 451, `documents.rs` 436, `scripts.rs` 387.
- Modules: `lib.rs` (`WyckConfig`: profiles, secrets, last symbol, builder, diagnose, transactional `commit()`), `app_config.rs` (versioned TOML), `profile.rs`, `secret/{mod,keyring_store,file_store}.rs`, `crypto.rs` (Argon2id and ChaCha20-Poly1305 with AAD), `sealed.rs`, `documents.rs`, `scripts.rs`, `backup.rs`, `doctor.rs`, `fs_util.rs` (`atomic_write`), `paths.rs`, `names.rs`, `error.rs`, `tokens.rs`.
- Misplaced responsibilities: `ProfileConfig` fields `client_id`, `callback_port`, `account_id` (`profile.rs:51-59`) while its doc says it knows nothing about cTrader (`:34-40`); `WyckConfig::set_openapi_profile` (`lib.rs:342-364`), `CLIENT_SECRET`, `OAUTH_TOKENS` (`:126-128`), `OpenApiTokens` (`tokens.rs`) are cTrader specific; `AppConfig.last_symbol` and `set_last_symbol` (`app_config.rs:31-33`, `lib.rs:395`) is a UI preference that belongs in a `DocumentStore` document; `scripts.rs` and `BackupScript` (`backup.rs:122`) know the chart indicator folder and `.rhai` extension; `paths.rs:165-176` `pictures_dir` and `documents_dir` serve chart export.
- Redundancy: `OpenApiTokens { access_token, refresh_token, expires_at }` (`tokens.rs:12-19`) duplicates `wyck_openapi::auth::TokenSet`; the app converts in `wyck/src/services/token_store.rs` and the conversion loses the real obtained-at instant, so the rebuilt expiry is approximate. The `TokenStore` trait is async, `OpenApiTokenStorage` is sync.
- Dead code: none found. `#[allow]`: none.
- Quality: no `unwrap`, `expect` or `panic!` outside tests; AAD bound to the key; path traversal rejected in `names.rs`; schema version newer than known is refused (`app_config.rs:84-99`); `atomic_write` tested under concurrency (`fs_util.rs:205-215`).
- Errors: `thiserror` (`error.rs`).
- Concurrency: none (synchronous).
- Docs: doc plus comment about 28 % of code lines; mostly accurate rationale for crash safety and cryptography, some restating. Has a README.
- Tests: about 130 tests, proptest in `crypto.rs:294` and `names.rs:127`, `tests/lifecycle.rs` 10 tests, `backup.rs` 18, `file_store.rs` 11. No ignored tests found.
- Verdict: keep. Clean comments (T-022), isolate the cTrader pieces (T-052), split `backup.rs` (T-032), move `last_symbol` to a document.
