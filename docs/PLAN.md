# Plan de refonte de Wyck

Document de travail de la refonte (workspace Cargo par domaine, CI/CD, configuration, docs, agents, suivi).
Il se modifie à la main : coche les cases, ajoute des notes. Je le mets à jour à la fin de chaque phase.

Légende : `[ ]` à faire, `[~]` en cours, `[x]` fait, `[!]` à décider ou bloqué.

Règle : je m'arrête à la fin de chaque phase (et de chaque sous-phase 4a à 4f) et je demande l'autorisation avant de continuer.
Rien n'est poussé, aucune PR n'est ouverte et aucun réglage GitHub n'est changé sans accord explicite.

## État

| Phase | Contenu | État |
|---|---|---|
| 0 | Base, mesures, décisions chiffrées, reliquats | [!] fait, à valider |
| 1 | Casser le cycle chart / drawings / indicators dans le crate unique | [ ] |
| 2 | Squelette du workspace (manifeste virtuel, dépendances centralisées, lints, profils) | [ ] |
| 3 | CI/CD refaite, xtask, réglages GitHub proposés | [ ] |
| 4a | Extraction : core, market, trading | [ ] |
| 4b | Extraction : drawings, indicators | [ ] |
| 4c | Extraction : chart | [ ] |
| 4d | Extraction : platform, storage, ctrader | [ ] |
| 4e | Extraction : ui-kit, app | [ ] |
| 4f | Extraction : ui-chart, ui-trading, ui-editor, ui-shell, binaire wyck | [ ] |
| 5 | Séparer les mélanges à l'intérieur des crates | [ ] |
| 6 | Tests et docs : allègement modéré | [ ] |
| 7 | AGENTS.md et CLAUDE.md (racine, crates, règles, skills) | [ ] |
| 8 | Éditeurs : .vscode et .zed | [ ] |
| 9 | Tous les .md revus, sécurité contre le vol | [ ] |
| 10 | Suivi de projet : ROADMAP, tâches, ADR | [ ] |

## Crates

| Crate | Extrait | Relu (grille) | AGENTS.md / CLAUDE.md |
|---|---|---|---|
| wyck-core | [ ] | [ ] | [ ] |
| wyck-market | [ ] | [ ] | [ ] |
| wyck-trading | [ ] | [ ] | [ ] |
| wyck-drawings | [ ] | [ ] | [ ] |
| wyck-indicators | [ ] | [ ] | [ ] |
| wyck-chart | [ ] | [ ] | [ ] |
| wyck-platform | [ ] | [ ] | [ ] |
| wyck-storage | [ ] | [ ] | [ ] |
| wyck-ctrader | [ ] | [ ] | [ ] |
| wyck-app | [ ] | [ ] | [ ] |
| wyck-ui-kit | [ ] | [ ] | [ ] |
| wyck-ui-chart | [ ] | [ ] | [ ] |
| wyck-ui-trading | [ ] | [ ] | [ ] |
| wyck-ui-editor | [ ] | [ ] | [ ] |
| wyck-ui-shell | [ ] | [ ] | [ ] |
| wyck (binaire) | [ ] | [ ] | [ ] |
| xtask | [ ] | [ ] | [ ] |

## Journal des mesures et des décisions

Rempli au fil des phases (date, mesure ou décision, source).

| Date | Phase | Entrée |
|---|---|---|
| 2026-10-14 | 0 | Départ : branche `refactor/workspace` depuis `main` (6cddc21). |
| 2026-10-14 | 0 | Tests de base : 1 188 tests unitaires (1 187 exécutés + 1 ignoré), 32 doc-tests (29 exécutés + 3 ignorés). Recompilation à chaud des tests : 21 s. |
| 2026-10-14 | 0 | Build à froid du profil actuel (18 cœurs) : 519 s (8 min 39) pour compiler et lier les tests, `target/` de 3,8 Go ; exécution des tests de la lib : 3 s ; recompilation après modification d'un fichier : 18 s. |
| 2026-10-14 | 0 | Décision : le comparatif profil `ci` contre profil actuel est abandonné (ta demande). Le profil `ci` sera essayé directement en CI en Phase 3, où les temps sont gratuits. |
| 2026-10-14 | 0 | Correction : `proptest` est utilisé (storage), seules `syn` et `proc-macro2` sont retirées. Aucune dépendance de `Cargo.lock` n'est supprimée (elles restent tirées par d'autres crates). |
| 2026-10-14 | 0 | Reliquats corrigés : `README.md` (plus de `--test ctrader` ni de « a test checks it »), `.env.example`, `.gitignore` (seuls les fichiers personnels de `.claude/` sont ignorés). |
| 2026-10-14 | 0 | Recensement clippy (92 lints sondés, dont des lints très bruyants ajoutés exprès pour mesurer) : voir la section « Lints proposés ». Environ la moitié des occurrences sont des doublons, car la lib est analysée deux fois (normale et tests). |
| 2026-10-14 | 0 | Ajustement du plan : `shortcuts` -> `wyck-app` (au lieu de `core`), `LineId` reste dans `chart`. |

---

# Plan détaillé

## Règles de travail (nouvelles, issues de tes retours)

1. **Une autorisation à chaque étape.** Je m'arrête à la fin de chaque phase (et de chaque sous-phase d'extraction 4a à 4f), je montre ce qui a changé et
   ce que j'ai mesuré, et je te demande avant de continuer. Aucun enchaînement automatique. Je ne pousse rien, n'ouvre aucune PR et ne change aucun
   réglage GitHub sans ton accord explicite, chaque fois.
2. **Chaque fichier déplacé est relu et nettoyé.** Un déplacement = deux commits : (1) déplacement mécanique, compte de tests identique, (2) nettoyage relu.
   La grille de relecture (appliquée à chaque fichier) :
   - un fichier, une responsabilité (deux domaines dans un fichier = on scinde) ;
   - API minimale : `pub(crate)` par défaut, `pub` seulement pour ce que les crates voisins utilisent. C'est ce qui rend `dead_code` efficace (la lint rustc
     `unreachable_pub` aide à le trouver) ;
   - zéro code mort : plus de `#[allow(dead_code)]`, on supprime ; `cargo-workspace-unused-pub` (index SCIP de rust-analyzer) pour les `pub` inutilisés dans tout le workspace ;
   - zéro redondance : `cargo-dupes` (doublons AST de Rust) et relecture ; constantes magiques nommées ; helpers dupliqués fusionnés ;
   - simplicité d'un projet solo : concret d'abord, puis enum, puis `dyn`, puis générique ; pas de trait à implémentation unique (sauf double de test
     justifié) ; pas de paramètre générique stocké dans des champs ; builder seulement au-delà de ~4 paramètres optionnels ; pas de macro maison sous 3 usages ;
   - erreurs : enums `thiserror` typés dans les crates bibliothèque, jamais `Result<_, String>` ; `anyhow` seulement dans `wyck` (binaire) et `xtask` ; jamais de secret dans un message ;
   - robustesse : pas de `unwrap`/`expect` (déjà en lint), pas d'indexation qui panique sur une entrée externe, casts numériques justifiés ; états globaux
     (`text_scale`, `intern`) listés dans le `AGENTS.md` du crate avec leur raison ;
   - conventions : [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html) (nommage, conversions `as_/to_/into_`, traits communs dérivés) et
     [Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/guidelines/project/index.html) de Microsoft ;
   - docs : `//!` de module en 1 à 3 lignes (rôle + invariants), `///` seulement pour l'unité, l'encodage, la panique, le pourquoi ; rien qui répète le nom ;
   - clippy sans `#[allow]` non justifié (chaque `allow` porte `reason = "..."`).
   À la fin de chaque crate : un compte rendu court (supprimé, simplifié, laissé tel quel et pourquoi).
3. **Chaque refonte se décide A/B avec une recherche.** Pour chaque sujet, je liste les options, la recommandation pour un projet solo (par défaut la plus simple) et
   la source. Les choix déjà tranchés sont dans le tableau « Décisions A/B ». Je ne complexifie pas le code sans gain mesuré.
4. **Les revendications de sécurité dans les `.md` sont vérifiées dans le code** avant d'être écrites (on ne documente que ce que le code fait).

## Constats mesurés (dans le dépôt et sur GitHub)

- **Code** : 127 000 lignes, un seul package. Cycle `chart <-> drawings <-> indicators` (petits utilitaires rangés dans le mauvais domaine). Fuites : `storage::chart_export` ->
  `chart`, `ui::kit::hints` -> `drawings::Tool`, doc-test de `trading::book` -> `infra::ctrader`, un test de `indicators::custom` -> `app::scripts`, cycle interne `app::alerts <-> app::workspace`.
  Fichiers fourre-tout : `chart/scene.rs` 1 489 lignes, `drawings/book.rs` 1 467, `storage/relocate.rs` 1 140, `app/workspace/preferences.rs` 954 (réglages de tous les domaines), `ui/shell/dashboard/marks.rs` 701.
- **Tests / docs** : 1 181 tests, 21 800 lignes de test. Poids par rapport au code : `storage` 80 %, `chart` 50 %, `drawings` 41 %, `indicators` 28 %. Docs : `ctrader` 42 %, `storage` 39 %, `market` 39 %.
- **Reliquats de #42** : `AGENTS.md`, `CLAUDE.md`, `RELEASING.md` sont **déjà supprimés** (rien à effacer). `README.md` et `.env.example` citent encore `cargo test --test ctrader` et « a test checks it » (tests supprimés). Dev-deps mortes : `syn` et `proc-macro2` (retirées en Phase 0 ; `proptest` est en fait utilisé par `storage/crypto.rs` et `storage/names.rs`, il reste).
- **CI mesurée** (run vert du 14/10) : fmt 22 s ; check Linux 107 s, macOS 174 s, Windows 215 s (restauration du cache 38 s, clippy 66 s, tests 77 s). Temps mural ~3,7 min à cache chaud.
  **Release : 34 min** (macOS Intel 2 028 s, macOS arm 1 297 s, Windows 1 084 s, Linux 945 s). Sur Windows : compilation release 855 s, installation de `cargo-packager` depuis les sources 149 s à chaque fois.
- **Le « déroulé »** : étapes sans nom (`Run sudo apt-get update` affiché sur Windows, deux étapes « Install cargo-packager », « Install Linux packaging libraries » sur Windows), pas de résumé de job, pas de temps par phase. Illisible pour savoir où le temps part.
- **Défauts réels de la CI** : actions épinglées par tag (le réglage GitHub `sha_pinning_required` est à `false`) ; `audit.yml` ne surveille que `Cargo.toml` à la racine (après le passage en workspace, un `crates/*/Cargo.toml` ne le déclencherait plus) ; `release.yml` ne vérifie pas que le commit taggé a passé la CI ;
  pas de lint de workflows, ni de TOML, ni de fautes de frappe, ni de dépendances inutiles ; toute la matrice recompile les dépendances de `gpui` en `opt-level = 3` (réglage voulu pour `cargo run`, coûteux en CI).
- **Sécurité GitHub** : **le signalement privé de vulnérabilités est désactivé**, alors que `SECURITY.md`, `CODE_OF_CONDUCT.md` et `ISSUE_TEMPLATE/config.yml` y renvoient les rapporteurs. Les deux secrets de signature (`CARGO_PACKAGER_SIGN_PRIVATE_KEY` et son mot de passe) sont des secrets
  de dépôt, lisibles par n'importe quel workflow, aucun Environment n'existe. Pas de ruleset de tags, pas de scan de code. Bien : secret scanning + push protection, Dependabot sécurité, permissions de workflow par défaut en lecture seule.
  Le tagline du README/GitHub (« terminal trading panel ») ne correspond plus à une application de bureau.
- **Suivi** : 0 issue, 0 milestone, labels par défaut, pas de modèle de PR (health 85 %).

## Recherches web retenues

| Sujet | Sources | Ce que j'en retiens |
|---|---|---|
| Layout | [matklad](https://matklad.github.io/2021/08/22/large-rust-workspaces.html), [Microsoft](https://microsoft.github.io/rust-guidelines/guidelines/project/index.html) | Manifeste racine virtuel, `crates/*` à plat, dossier = nom du crate, frères en `.workspace = true`, jamais de crate imbriqué |
| Cargo | [Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html), [Lints](https://doc.rust-lang.org/beta/cargo/reference/lints.html) | `resolver = "3"` explicite (obligatoire en virtuel), `[workspace.package]`, `[workspace.dependencies]`, `[workspace.lints]` + `[lints] workspace = true`, lints `missing_lints_inheritance` et `unused_workspace_dependencies` |
| Dépendances | [cargo-autoinherit](https://github.com/mainmatter/cargo-autoinherit), [cargo-deny bans](https://embarkstudios.github.io/cargo-deny/checks/bans/cfg.html) | Centraliser avec `cargo autoinherit`, `default-features = false` à la racine, `workspace-default-features` / `external-default-features` pour le contrôler, `wrappers` pour borner qui peut dépendre de `gpui`, `tokio`... |
| Build | [Cargo build performance](https://doc.rust-lang.org/cargo/guide/build-performance.html), [corrode](https://corrode.dev/blog/tips-for-faster-ci-builds/) | `debug = "line-tables-only"`, pas de debug pour les dépendances, `CARGO_INCREMENTAL=0` en CI, `--locked`, nextest (~40 % mesuré en CI), lld par défaut sur Linux x86_64, mesurer chaque réglage |
| Cache | [rust-cache](https://github.com/Swatinem/rust-cache) | `save-if` sur main seulement, `shared-key`, nettoyage automatique, 10 Go de quota, les caches de PR lisent ceux de main |
| Windows | [Zed (ReFS)](https://git.secluded.site/zed/commit/2442c49048b71cc92a5d940c0745e84f6a5a7213), [uv](https://git.joshthomas.dev/mirrors/uv/src/commit/e7f596711114528d98af1d20c91ce05f8462c703/.github/workflows/setup-dev-drive.ps1) | Dev Drive/ReFS : gains annoncés ~25 % mais contestés (uv n'a rien vu sur runners standard) -> **à mesurer** sur Wyck avant de l'adopter |
| Tests | [nextest](https://nexte.st/docs/configuration/), [test-coverage](https://nexte.st/docs/integrations/test-coverage/) | Profil `ci` (`fail-fast = false`), doc-tests à part (`cargo test --doc`), JUnit possible, pas de retries (un test instable se corrige) |
| Jobs conditionnels | [Pants](https://www.pantsbuild.org/blog/2022/10/10/skipping-github-actions-jobs-without-breaking-branch-protection), [Wharton](https://jakewharton.com/fan-in-to-a-single-required-github-action/) | Jamais de `paths-ignore` au niveau du workflow si un check est requis : job `changes` + `if:` + agrégateur `if: always()` qui échoue sur `failure`/`cancelled` ; liste blanche de chemins « docs » (inconnu = code) |
| Sécurité CI | [GitHub hardening](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions), [zizmor](https://docs.zizmor.sh/audits/), pinact | SHA complets + commentaire de version (Dependabot les met à jour), `persist-credentials: false`, permissions minimales, Environment avec relecteurs pour les secrets, cooldown Dependabot, CODEOWNERS sur `.github/workflows` |
| Release | [Environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments), [immutable releases](https://github.blog/changelog/2025-10-28-immutable-releases-are-now-generally-available/), [gh attestation verify](https://cli.github.com/manual/gh_attestation_verify) | Secrets de signature dans un Environment `release` restreint aux tags `v*.*.*`, brouillon -> assets -> publication (compatible releases immuables), vérification de l'attestation après publication, ruleset sur les tags |
| Profil release | [Ruff](https://github.com/astral-sh/ruff), forum Rust | Thin LTO + `codegen-units = 16` : build bien plus court, performance proche -> profil `dist` à **mesurer** (temps de build vs temps de `chart::scene::build`) puis décider avec toi |
| Scan | [CodeQL Rust GA](https://github.blog/changelog/2025-10-14-codeql-scanning-rust-and-c-c-without-builds-is-now-generally-available) | Disponible pour Rust et Actions, « default setup » sans fichier de workflow (réglage à valider par toi) |
| Code mort / doublons | [cargo-workspace-unused-pub](https://github.com/cpg314/cargo-workspace-unused-pub), [cargo-dupes](https://gitblind.noratr.app/mpecan/cargo-dupes), cargo-machete, `unreachable_pub` | La lint `dead_code` ignore les `pub` : réduire l'API à `pub(crate)` puis mesurer ; outils workspace pour le reste |
| Lints | [Clippy](https://doc.rust-lang.org/clippy/), [coreyja](https://coreyja.com/til/clippy-pedantic-workspace) | `pedantic` avec `priority = -1` et liste d'exceptions **choisie par comptage** sur ce code (Phase 0), jamais le groupe `restriction` en bloc |
| Erreurs / abstractions | [anyhow/thiserror](https://docs.rs/anyhow), [Microsoft Rust patterns](https://microsoft.github.io/RustTraining/rust-patterns-book/ch01-generics-the-full-picture.html) | `thiserror` en bibliothèque, `anyhow` en binaire ; concret > enum > `dyn` > générique |
| Agents | [Claude memory](https://code.claude.com/docs/en/memory), [best practices](https://code.claude.com/docs/en/best-practices), [agents.md](https://agents.md/) | `CLAUDE.md` < 200 lignes ; si un `CLAUDE.md` existe, Claude **ne lit pas** `AGENTS.md` seul : chaque dossier a donc `AGENTS.md` (contenu) + `CLAUDE.md` (`@AGENTS.md` + 3 lignes propres à Claude), le plus proche prime, chargé à la demande ; règles par zone dans `.claude/rules/` avec `paths:` ; vérifications exécutables plutôt que prose |
| Éditeurs | [rust-analyzer config](https://rust-analyzer.github.io/book/configuration.html), [VS Code Rust](https://code.visualstudio.com/docs/languages/rust), [Zed Rust](https://zed.dev/docs/languages/rust), [Zed tasks](https://zed.dev/docs/tasks) | `check.command = clippy`, `cargo.targetDir` séparé (sinon `cargo run` bloque sur le verrou), débogage Windows avec `cppvsdbg` (CodeLLDB gère mal les PDB), `lldb` ailleurs |
| ADR | [MADR](https://adr.github.io/madr/) | `docs/adr/NNNN-titre.md`, gabarit minimal |
| Écartés | merge queue (réservée aux organisations, `t-aize/wyck` est un dépôt personnel), `cargo-hakari` (à mesurer d'abord), Cranelift / `-Zthreads` (nightly) | |

## Décisions A/B (solo)

| Sujet | Choix | Pourquoi |
|---|---|---|
| Granularité | 15 crates bibliothèque + `wyck` (binaire) + `xtask` | Ta décision ; le graphe réel le permet une fois les utilitaires remontés dans `wyck-core` |
| Version | une version unique via `[workspace.package]` | La validation de tag de `release.yml` (une seule version) reste valide ; `0.0.0` pour les internes n'apporte rien ici |
| Dépendances | **tout** dans `[workspace.dependencies]` à la racine (externes `default-features = false`, internes en `path`), membres en `.workspace = true`, features listées par le membre qui en a besoin | Ta décision ; vérifié par `cargo xtask deps` + `cargo machete` |
| Rhai (scripts) | reste dans `wyck-indicators` | `indicators::config` et `catalog` référencent `custom` : un crate séparé créerait un cycle |
| Tests | nextest + doc-tests, sans retries | Rapide, sorties lisibles, un test instable se corrige |
| Automatisation | `xtask` (Rust) | Pas de `make`/`just`/Python sur ta machine Windows ; recommandé par matklad |
| Suivi | dans le dépôt (ROADMAP + tâches + ADR) | Ta décision |
| LTO | thin vs fat : mesure en Phase 3, décision par toi | Gain de build attendu important, effet runtime à vérifier |
| macOS Intel | cross-compiler depuis le runner arm si le build passe | `macos-15-intel` coûte 34 min contre 22 min en arm |

## Architecture cible

```
Cargo.toml (virtuel)  rust-toolchain.toml  rustfmt.toml  clippy.toml  deny.toml  .taplo.toml  _typos.toml
.cargo/config.toml (alias xtask)   .config/nextest.toml
crates/
  wyck-core       vocabulaire minuscule: flex, format, defaults, text_scale, intern, math, appearance
  wyck-market     symboles, barres, ticks, cotations, horaires, Timeframe
  wyck-trading    ordres, plan, garde, marge, carnet, atr_stop
  wyck-drawings   outils de dessin, géométrie, carnet de dessins
  wyck-indicators indicateurs, calcul, catalogue, scripts Rhai, starters .rhai
  wyck-chart      données, scène (commandes de rendu), export, TPO/footprint/flow/volume, réglages
  wyck-platform   journal, infos de build, runtime tokio
  wyck-storage    chemins, documents, migrations, sauvegardes, relocalisation, secrets, crypto
  wyck-ctrader    client Open API (+ examples/, .env.example)
  wyck-app        états et cas d'usage GPUI (sign-in, compte, alertes, préférences, workspace, mises à jour)
  wyck-ui-kit     système de design (+ police, logo, marks)
  wyck-ui-chart   graphique et multi-graphique
  wyck-ui-trading ticket, panneau d'ordres
  wyck-ui-editor  éditeur d'indicateurs
  wyck-ui-shell   fenêtre, tableau de bord, réglages, connexion, raccourcis
  wyck            binaire (main + run, build.rs, packager, icônes)
xtask/            cargo xtask check | deps | docs-check | tasks
```
Flèches vers le bas uniquement : `core <- market <- trading <- drawings <- indicators <- chart` ; `platform <- storage` ; `ctrader -> core, market, trading` ;
`app -> chart, indicators, drawings, trading, market, storage, platform, ctrader, gpui` ; `ui-kit -> core, gpui, gpui-kit` ; `ui-chart -> ui-kit, app, chart` ; `ui-trading`, `ui-editor -> ui-chart` ; `ui-shell -> tous les ui-*` ; `wyck -> ui-shell, app, platform`.

Ce qui se déplace pour casser le cycle : `defaults`, `format`, `text_scale`, `math`, `intern`, `flex`, `appearance` -> `core` (`shortcuts` va dans `wyck-app`, seuls `app` et `ui-shell` l'utilisent ; `LineId` reste dans `chart`) ; `Timeframe` -> `market` (il dépend de `market::Period`) ;
`atr_stop` -> `trading` (il dépend de `market::Bar` et `core::math`) ; `storage::chart_export` -> `app` ; `ui::kit::hints` reçoit ses textes de l'appelant.
Règles (dans `deny.toml`, `AGENTS.md`) : les crates domaine n'ont ni `gpui`, ni `tokio`, ni `reqwest`, ni `keyring`, ni I/O ; seul `ui-kit` nomme `gpui-kit` ; seul `ctrader` fait du réseau cTrader ; seul `storage` écrit les fichiers de l'utilisateur ; `core` reste minuscule.
Les assets appartiennent au crate qui les embarque (police, logo, marks -> `ui-kit` ; sons et `update.pubkey` -> `app` ; starters `.rhai` -> `indicators` ; icônes -> `wyck`).

## Phases (chacune finit par un point d'arrêt)

### Phase 0 : base, mesures, décisions chiffrées
- Créer `docs/PLAN.md` (ce plan + statuts). Branche `refactor/workspace`.
- Relever : nombre de tests (`cargo test -- --list`), temps de build à froid et à chaud (profil dev actuel), taille de `target/`.
- **Expérience profil `ci`** : mesurer `cargo test` à froid avec le profil actuel (dépendances en `opt-level = 3`) contre un profil `[profile.ci]` (`inherits = "dev"`, dépendances en `opt-level = 0`, `debug = 0`, `incremental = false`). Vérifier aussi que `inherits` reprend bien les réglages `[profile.dev.package."*"]`.
- **Clippy** : lancer `pedantic` (+ quelques lints `nursery` utiles) sur le code actuel, compter par lint, proposer la liste finale (groupe en `warn`, exceptions justifiées).
- Corriger les reliquats : `--test ctrader`, « a test checks it », dev-deps mortes.
- `.gitignore` : aujourd'hui `.claude/` entier est ignoré, ce qui empêcherait de versionner `settings.json`, `rules/`, `skills/`. N'ignorer que `.claude/settings.local.json`, `CLAUDE.local.md`, `.agents/`.
- **Point d'arrêt** : je te présente les mesures et la liste de lints, tu valides.

### Phase 1 : casser le cycle dans le crate unique
- Déplacer les utilitaires listés plus haut, corriger les fuites (hints, chart_export, test de `custom`), rendre le graphe de modules acyclique (script de graphe PowerShell déjà utilisé pour l'analyse).
- Grille de relecture appliquée aux fichiers déplacés. Test count identique.
- **Point d'arrêt**.

### Phase 2 : squelette du workspace
- Manifeste racine virtuel : `resolver = "3"`, `[workspace.package]`, `[workspace.lints]` (liste de la Phase 0 + `unsafe_code = "forbid"`, `unreachable_pub`, `unwrap_used`/`expect_used` en deny), `[profile.dev|release|ci|dist]`, `[workspace.dependencies]` rempli avec `cargo autoinherit` puis nettoyé à la main (`default-features = false`).
- Déplacer le package actuel dans `crates/wyck/` (historique conservé), `.cargo/config.toml`, `rustfmt.toml`, `.config/nextest.toml`, `.taplo.toml`, `_typos.toml`, `deny.toml` mis à jour (`allow-wildcard-paths = true` pour les `path` internes).
- Pas de nouveau crate encore. Build, clippy et tests verts.
- **Point d'arrêt**.

### Phase 3 : CI/CD refaite (rapide, lisible, sûre)
Objectifs mesurables (à valider avec les chiffres de la Phase 0) : PR code à cache chaud sous ~3 min, PR docs seule sous 1 min, release sous ~22 min.
- **Action composite locale** `.github/actions/setup-rust` : paquets Linux (une seule fois), toolchain lue dans `rust-toolchain.toml` (pas de `dtolnay/rust-toolchain` redondant), `rust-cache`. Dependabot couvre `/.github/actions/*`.
- **`ci.yml`** : déclencheurs `push` main, `pull_request`, `workflow_dispatch` ; **job `changes`** (script `git diff`, liste blanche docs, inconnu = code, `push` sur main = tout) ; `lint` (fmt, taplo, typos, machete, `xtask deps`, `xtask docs-check`, `xtask tasks`) ; `actions-lint` (zizmor) ; `check` en matrice 3 OS avec le **profil `ci`** : clippy, `nextest run --profile ci`, `cargo test --doc` ; `docs` (`-D warnings`) ; `release-build` sur main ; **`ci-ok`** (`if: always()`, échoue sur `failure`/`cancelled`, tolère `skipped`) reste le seul check requis, son nom ne change pas.
  Chaque étape a un **nom lisible**, un **résumé de job** (`$GITHUB_STEP_SUMMARY` : temps par phase, nombre de tests, cache chaud/froid), `timeout-minutes` réalistes (15 à 25), `concurrency` annulant les runs obsolètes (sauf main), `permissions: {}` par défaut, `persist-credentials: false`, actions tierces épinglées par SHA + commentaire de version (trouvés via `gh api`), seulement `Swatinem/rust-cache` et `taiki-e/install-action` en tiers (cargo-deny, zizmor, nextest, taplo, typos, machete via install-action).
- **Expériences chronométrées** (`workflow_dispatch` sur la branche, aucune PR) : (a) ligne de base, (b) profil `ci`, (c) Dev Drive/ReFS sur Windows, (d) `sccache` contre `rust-cache` seul. Je garde ce qui gagne et j'écris le résultat dans `docs/PLAN.md`.
- **`audit.yml`** : `cargo deny check` via install-action, chemins surveillés `**/Cargo.toml`, `Cargo.lock`, `deny.toml`, `rust-toolchain.toml` ; passage hebdomadaire conservé. **Layering dans `deny.toml`** (`bans.deny` + `wrappers`) : à valider par une **violation volontaire** (ajouter `gpui` à `wyck-market` doit échouer), sinon repli sur `xtask deps`.
- **`release.yml`** : `workflow_dispatch` dry-run (build + paquet sans publier : seul moyen de valider les chemins `cargo-packager` après le passage dans `crates/wyck`) ; `validate` vérifie le format du tag, la version du workspace, la section du `CHANGELOG`, que le commit est sur `main` et que son check `ci-ok` est vert ; job `package` dans l'**Environment `release`** (secrets de signature, restreint aux tags `v*.*.*`) ; profil `dist` selon la mesure ; macOS Intel cross-compilé depuis le runner arm si ça passe ; cache de `cargo-packager` via un workflow de préchauffage hebdomadaire sur la branche par défaut (les caches de tags ne sont pas partagés) ; publication brouillon -> assets -> publication, puis `gh attestation verify` sur les fichiers publiés. Étapes nommées, un seul `Install cargo-packager`.
- Dependabot : `directories` pour les actions composites, cooldown conservé. CODEOWNERS : ajout d'une règle sur `.github/workflows/` et `deny.toml`.
- **`xtask`** (Rust, sans dépendance hors workspace) : `check` (fmt, clippy, nextest, doc-tests, docs ; avec `--full` : deny, machete, typos, taplo si installés), `deps` (aucun membre ne déclare version/path/git directement), `docs-check` (chaque crate a `AGENTS.md` et `CLAUDE.md`, sections attendues, ligne « Depends on » égale à `Cargo.toml`), `tasks` (liste/valide `docs/tasks`). Option à trancher : porter les deux scripts Python de release vers xtask (tests locaux sur Windows).
- **Réglages GitHub proposés, un par un, avec ton accord à chaque fois** : activer le signalement privé de vulnérabilités ; créer l'Environment `release` et y déplacer les deux secrets (puis supprimer ceux du dépôt) ; `sha_pinning_required` ; ruleset des tags `v*` (pas de mise à jour ni suppression) ; releases immuables ; scan de code par défaut (CodeQL Rust + Actions) ; suppression auto des branches fusionnées ; Scorecard (optionnel). Pour tester la CI il faudra pousser la branche : je te le demande à ce moment.
- **Point d'arrêt**.

### Phase 4 : extraction des crates (bas -> haut), sous-phases avec point d'arrêt chacune
- 4a `core`, `market`, `trading` · 4b `drawings`, `indicators` · 4c `chart` · 4d `platform`, `storage`, `ctrader` · 4e `ui-kit`, `app` · 4f `ui-chart`, `ui-trading`, `ui-editor`, `ui-shell`, binaire `wyck`.
- Méthode par crate : (1) table de réécriture des chemins appliquée par script (`crate::domain::chart::X` -> `wyck_chart::X`) ; (2) déplacement mécanique + tests, fixtures (`src/app/fixtures`), assets, `include_str!`, dépendances (`workspace = true`), compte de tests identique ; commit ; (3) passe de relecture selon la grille ; (4) `AGENTS.md` + `CLAUDE.md` du crate (gabarit ci-dessous) ; (5) rapport : lignes supprimées, code mort, doublons, `pub` réduits.
- `ctrader` emporte `examples/` et `.env.example` (`cargo run -p wyck-ctrader --example ctrader_prices`). `keymap_guard.rs` va dans `ui-shell` (repointé) ; sort ou reste en Phase 6.
- Les dépendances lourdes (`rhai`, `rodio`, `tokio`, `reqwest`, `keyring`) n'apparaissent que dans leur crate propriétaire.

### Phase 5 : séparer les mélanges à l'intérieur des crates
`preferences.rs` -> un module par domaine de réglage et cycle `alerts <-> workspace` cassé ; `scene.rs` (fonction `build` de 330 lignes) et `book.rs` découpés par responsabilité ; `marks.rs` -> un module par classe d'actif ; `relocate.rs`/`locations.rs`/`backup.rs` -> un sous-module par étape ; `chart` : l'« order flow » (tpo/footprint/flow/volume) isolé en sous-module clair, pas en crate. Règle d'arrivée : aucun fichier de production > 600 lignes sans raison écrite.
- **Point d'arrêt**.

### Phase 6 : tests et docs (allègement modéré, après le découpage)
Je te livre d'abord la **liste des tests et blocs de doc retirés par crate** (critères : détail privé, getter, doublon, casse à chaque refactor sans changement de comportement) ; je ne supprime qu'après ton accord. Gardés : garde-fous de trading, migrations, crypto/secrets, protocole cTrader, maths des indicateurs, sérialisation des documents, régressions connues. Cible ~-30 % de lignes de test et de doc, surtout `chart`, `drawings`, `storage`, `ctrader`. Pas de `missing_docs` imposé.
- **Point d'arrêt**.

### Phase 7 : fichiers pour les agents (`AGENTS.md` + `CLAUDE.md` partout)
- **Dans chaque crate** (créés à l'extraction, finalisés ici) : `AGENTS.md` avec les sections : Rôle · Depends on / Used by · Carte des modules · API publique (points d'entrée) · Invariants et pièges · États globaux · Tests (quoi, comment, commande) · Où ajouter X · Ne pas faire. `CLAUDE.md` = `@AGENTS.md` + 3 lignes propres à Claude.
- **À la racine** : `AGENTS.md` (< 200 lignes) = carte du workspace, commandes exactes, règles d'architecture, définition de « terminé », style d'écriture ASCII (seule règle de l'ancien fichier conservée), **index de tous les `.md`** (README, CONTRIBUTING, SECURITY, SUPPORT, CODE_OF_CONDUCT, CHANGELOG, ROADMAP, `docs/*`, ADR, tâches, `AGENTS.md` de chaque crate) ; `CLAUDE.md` = `@AGENTS.md` + consignes Claude (mode plan pour tout changement multi-crates, `cargo xtask check` avant de dire « terminé », lire `docs/tasks/` et `docs/PLAN.md`).
- `.claude/rules/*.md` avec `paths:` (domaine, ui, storage, ctrader, tests, rust), `.claude/settings.json` partagé (permissions cargo/git/gh en lecture, refus de lire `.env`), `.claude/skills/{new-crate,task}`. `cargo xtask docs-check` empêche la dérive.
- **Point d'arrêt**.

### Phase 8 : éditeurs
- `.vscode/` refait : `settings.json` (rust-analyzer en clippy, `cargo.targetDir` séparé, `check.workspace`, formatage Rust/TOML/YAML/JSON/MD, règles 100 colonnes, exclusions `target`/`dist`, schémas YAML des workflows), `extensions.json` (rust-analyzer, Even Better TOML, EditorConfig, CodeLLDB, C/C++ pour `cppvsdbg`, typos, GitHub Actions ; ids vérifiés sur le Marketplace), `tasks.json` (build, `xtask check`, test d'un crate, clippy, doc, deny), `launch.json` (lancer/déboguer `wyck` : `cppvsdbg` sous Windows, `lldb` ailleurs).
- `.zed/settings.json`, `.zed/tasks.json` (même cible), `.editorconfig` et `.gitattributes` revus.
- **Point d'arrêt**.

### Phase 9 : tous les `.md` revus, avec la sécurité contre le vol en tête
Menaces à couvrir (chacune vérifiée dans le code, sinon non écrite) : vol du jeton OAuth/de la clé API (trousseau système, repli chiffré argon2 + chacha20poly1305, `secrecy`/`zeroize`, jamais dans les journaux), ordres non voulus (garde de trading, démo d'abord), compromission de la mise à jour (clé publique épinglée, signature vérifiée) et de la chaîne de build (SHA épinglés, Environment `release`, attestations, releases immuables, `cargo deny`, Dependabot avec cooldown), vol de la clé de signature, fichiers locaux.
- `SECURITY.md` : canal qui fonctionne (une fois le signalement privé activé), périmètre précis, délais, comment **vérifier un téléchargement** (`SHA256SUMS` + `gh attestation verify --repo t-aize/wyck`), garanties et limites (binaires non signés Authenticode/Developer ID).
- `README.md` : court, juste (tagline corrigée), installation + vérification, build en 3 commandes, liens ; le reste va dans `docs/user-guide.md` et `docs/development.md`. `CONTRIBUTING.md` (vrai flux de travail : branche, `xtask check`, commits, checklist), `SUPPORT.md`, `CODE_OF_CONDUCT.md` (canal de signalement vérifié), `.github/pull_request_template.md`, `CHANGELOG.md` (Keep a Changelog), `docs/architecture.md`, `docs/conventions.md` (les règles de la grille, avec leurs sources), `docs/threat-model.md`, `docs/adr/`.
- **Point d'arrêt**.

### Phase 10 : suivi de projet (dans le dépôt)
`ROADMAP.md` (Maintenant / Ensuite / Plus tard, un jalon par version), `docs/tasks/NNNN-slug.md` (en-tête `status`, `area`, `milestone`, `depends`), `docs/tasks/README.md`, ADR 0001 (workspace par domaine, avec l'historique #24/#29) et 0002 (layering par cargo-deny/xtask), `cargo xtask tasks`. Ce chantier est enregistré comme tâches 0001 à 0010.
- **Point d'arrêt final**, puis, si tu veux, PR(s) : je te demande comment les découper.

## Fichiers critiques
À créer ou réécrire : `Cargo.toml`, `crates/*/{Cargo.toml,AGENTS.md,CLAUDE.md}`, `xtask/**`, `.cargo/config.toml`, `.config/nextest.toml`, `rustfmt.toml`, `.taplo.toml`, `_typos.toml`, `deny.toml`, `.gitignore`, `.github/{actions,workflows,dependabot.yml,CODEOWNERS,pull_request_template.md}`, `.claude/**`, `.vscode/**`, `.zed/**`, tous les `.md` de la racine, `docs/**`.
À lire d'abord : `src/lib.rs` (`run()`), `chart/{defaults,format,text_scale,timeframe}.rs`, `indicators/{atr_stop,intern,math,config}.rs`, `storage/chart_export.rs`, `ui/kit/{hints,theme}.rs`, `ui/assets.rs`.
Conservés : `clippy.toml`, `rust-toolchain.toml`, la logique de signature/attestation de `release.yml`, les formulaires d'issues (bug, fonctionnalité).

## Vérification
- À chaque étape : `cargo check --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, nextest + doc-tests, comparaison du nombre de tests au relevé de la Phase 0 (jusqu'à la Phase 6), `cargo xtask check`.
- Graphe : `cargo tree -p <crate> -e normal` et la **violation volontaire** (ajouter `gpui` à `wyck-market`) doivent échouer en CI.
- Application : `cargo run -p wyck` (graphique, dessin, éditeur d'indicateurs, réglages), sans compte réel.
- CI : branche poussée avec ton accord, tous les jobs verts, `ci-ok` seul check requis ; PR docs seule < 1 min ; chronos avant/après dans `docs/PLAN.md`. Release : dry-run sur les 4 plateformes, puis seulement un vrai tag si tu le décides.
- Docs et agents : `cargo doc --workspace -D warnings`, liens relatifs vérifiés, `cargo xtask docs-check`, `/context` dans une session neuve pour voir `CLAUDE.md` racine + celui du crate chargés.

## Hors périmètre (à décider plus tard)
Portage des scripts d'assets (`make_*.py`), automatisation de version (`release-plz`, `git-cliff`), couverture de code, `cargo-hakari`, merge queue (si le dépôt passe en organisation), réglages de branche en ruleset complet.

## Lints proposés (résultat du recensement de la Phase 0)

Méthode : `cargo clippy --all-targets` avec `pedantic`, `nursery` et une vingtaine de lints `restriction` activés pour compter, sur tout le code actuel.
Total : 15 257 avertissements, 92 lints. Les plus gros (`indexing_slicing` 2 753, `as_conversions` 1 768, `arithmetic_side_effects` 1 549, `cast_*`, `must_use_candidate` 1 568, `unreadable_literal` 887 qui sont surtout des couleurs hexadécimales) sont du bruit pour du code graphique et numérique : **non retenus**.

Choix : liste explicite (cherry-pick), pas le groupe `pedantic` entier. Les groupes évoluent à chaque version de clippy, une liste explicite garde la CI stable.

| Lint | Occurrences (x2 environ) | Pourquoi |
|---|---|---|
| `unsafe_code = forbid`, `unwrap_used`, `expect_used` | déjà en place | Existant |
| `unreachable_pub` (rustc) | 164 | C'est l'outil de la règle « API minimale » : `pub` inutile -> `pub(crate)`, puis `dead_code` devient efficace |
| `allow_attributes_without_reason`, `allow_attributes` | 20, 18 | Tout `allow` doit porter un `reason` ; `#[expect]` casse quand le lint ne se déclenche plus (pas de `allow` périmé) |
| `dbg_macro`, `todo`, `unimplemented` | 0 | Garde-fou gratuit |
| `print_stdout`, `print_stderr`, `exit` | 6, 3, 2 | Le code passe par `tracing` ; seuls `examples/` et le journal de démarrage ont des exceptions justifiées |
| `match_same_arms` | 57 | Redondance (bras identiques) |
| `redundant_clone` | 45 | Travail inutile |
| `needless_pass_by_value` | 36 | Qualité d'API |
| `unused_self` | 94 | Méthode qui n'utilise pas `self` : candidate à une fonction libre ou à la suppression |
| `unnecessary_wraps` | 9 | `Result`/`Option` inutiles |
| `derive_partial_eq_without_eq` | 74 | Dérivations `Eq` manquantes (API Guidelines) |
| `suspicious_operation_groupings` | 22 | Détecte de vraies fautes de frappe dans des expressions |
| `case_sensitive_file_extension_comparisons` | 9 | Vrai bug possible sous Windows (`.RHAI` contre `.rhai`) |
| `used_underscore_binding`, `unused_async`, `assigning_clones` | 22, 2, 60 | Petits gains, corrections automatiques pour la plupart |

Retenu pour plus tard seulement si tu veux : `missing_debug_implementations` (152, utile aux crates bibliothèque seulement), `clone_on_ref_ptr` (86), `semicolon_if_nothing_returned` (202, style), `too_many_lines` (192).

Mise en place sans casser la CI : `[workspace.lints]` porte la liste complète dès la Phase 2. Le crate historique `wyck` garde l'ancienne liste dans son propre `[lints]` (il n'hérite pas) tant qu'il contient du code non relu ; chaque crate extrait en Phase 4 hérite de la liste complète et doit passer sans avertissement (c'est la grille de relecture). Le binaire `wyck` hérite à la fin.