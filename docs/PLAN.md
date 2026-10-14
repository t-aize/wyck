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
| 1 | Casser le cycle chart / drawings / indicators dans le crate unique | [!] fait, à valider |
| 2 | Squelette du workspace (manifeste virtuel, dépendances centralisées, lints, profils) | [!] fait, à valider |
| 3 | CI/CD refaite, xtask, réglages GitHub proposés | [!] CI et essai à blanc de la release verts sur GitHub ; réglages GitHub à décider |
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
| 2026-10-14 | 1 | Déplacement mécanique (commit 1aeec46) : `defaults`, `format`, `text_scale`, `intern`, `math`, `flex`, `appearance` -> module `wyck_core` ; `Timeframe` -> `domain::market` ; `atr_stop` -> `domain::trading` ; `shortcuts` et `chart_export` -> `app`. 88 fichiers touchés par réécriture de chemins, mêmes tests. |
| 2026-10-14 | 1 | Fuites de couches corrigées (commit 5bb98ec) : `kit::hints` n'importe plus `Tool` (la table touche -> outil est dans le rail de dessin du multichart), les 3 tests de scripts passent par le registre du domaine au lieu de `app::scripts::Library` (avec un `registry::insert` réservé aux tests, qui évite aussi qu'ils s'écrasent entre eux), `NewOrderType` passe de `infra::ctrader` à `domain::trading` (la macro `number_enum!` reste interne au domaine trading), le test qui cherche les couleurs en dur passe de `app::appearance` à `ui::kit`, plus de lien de doc vers une couche plus haute. |
| 2026-10-14 | 1 | Relecture des fichiers déplacés (commit 3888397) : panique latente corrigée dans `AtrStop::value` (liste de barres vide avec dernière barre ouverte, test ajouté) ; `Slices` et la liste d'options quittent `intern` pour `indicators::custom::library` (un seul utilisateur) ; `text_scale` réutilise les bornes de `appearance` ; `appearance` : un seul `impl Colors`, dérivations serde inutiles retirées, tests de `luminance`/`contrast` placés avec le code ; `chart_export` : imports et `to_lowercase` hoisté. Sonde de code mort (`pub` -> `pub(crate)` sur les 11 fichiers) : aucun élément mort, seul `theme::luminance` n'était utilisé que par des tests. |
| 2026-10-14 | 1 | Résultat : 1 189 tests unitaires (2 de plus), 28 doc-tests (1 de moins : l'exemple de `AccountBook` qui citait le client cTrader est devenu du texte), clippy `-D warnings` et rustdoc `-D warnings` propres. Graphe de modules (production) : les domaines forment un DAG `market <- trading <- drawings <- indicators <- chart`, `wyck_core` n'importe rien, aucune flèche ne remonte de couche. Reste un cycle interne à `app` (`alerts <-> drawings <-> workspace`), traité en Phase 5. |
| 2026-10-14 | 2 | Manifeste racine virtuel (`resolver = "3"`, `[workspace.package]`, `[workspace.dependencies]`, `[workspace.lints]`, profils) ; le package est déplacé tel quel dans `crates/wyck/` avec `src`, `assets`, `examples` et `build.rs` (historique git conservé). Toutes les dépendances sont déclarées une seule fois à la racine, le crate `wyck` les reprend en `.workspace = true` et liste lui-même ses fonctionnalités. `default-features = false` n'est posé que là où il l'était déjà (futures-util, reqwest, rodio) : le retirer ailleurs n'apporte rien de mesurable (les fonctionnalités s'unifient dans le binaire) et risque un changement silencieux (par exemple les magasins de secrets de `keyring`). Décision à rouvrir crate par crate en Phase 4 avec la preuve ci-dessous. |
| 2026-10-14 | 2 | Preuve que rien n'a bougé : `cargo tree -e normal,build,dev -f "{p} [{f}]"` donne exactement les mêmes 902 paquets avec les mêmes fonctionnalités avant et après, et `Cargo.lock` est inchangé. Tests : 1 189 unitaires et 28 doc-tests, clippy et rustdoc `-D warnings` propres dans la nouvelle disposition. |
| 2026-10-14 | 2 | Lints : `[workspace.lints]` porte la liste complète validée en Phase 0 pour les crates extraits ; le crate `wyck` garde l'ancienne liste dans son propre `[lints]` (il n'hérite pas) jusqu'à être vidé. `rustfmt.toml` (édition 2024, 100 colonnes, fins de ligne Unix), `clippy.toml` (`allow-print-in-tests`), `deny.toml` (`allow-wildcard-paths`, non vérifié en local car `cargo deny` n'est pas installé : première vérification par la CI). |
| 2026-10-14 | 2 | Un test ignoré (`indicator_families_preview`) écrivait une image dans `<crate>/target/`, dossier qui n'existe plus après le déplacement : il écrit maintenant dans le dossier temporaire. `.gitignore` : `target/` n'est plus ancré à la racine. Scripts d'assets Python repointés vers `crates/wyck/assets` ; `audit.yml` surveille `**/Cargo.toml` ; README : section Layout mise à jour (réécriture complète en Phase 9). |
| 2026-10-14 | 2 | Déplacé en Phase 3 (ils vont avec leur outil, pour pouvoir les tester en local et en CI) : `.cargo/config.toml` (alias `xtask`), `.config/nextest.toml`, `.taplo.toml`, `_typos.toml`. **À vérifier au tout premier essai de la CI en Phase 3** : les chemins de `cargo-packager` après le déplacement (`license-file = "../../LICENSE"` et `icons` relatifs à `crates/wyck`) ; la release n'a pas été exécutée. |
| 2026-10-14 | 3 | `xtask` écrit avec `clap` 4.6.7 (dernière version, demandée) : `cargo xtask check [--full]` (format, clippy, tests, rustdoc, règles de dépendances ; en `--full` aussi cargo-deny, cargo-machete, typos, taplo, zizmor, actionlint, ceux qui manquent sont sautés avec la commande d'installation) et `cargo xtask deps`. `deps` impose que chaque crate prenne version, métadonnées et lints de la racine (`x.workspace = true`, aucune version/chemin/git dans un crate), qu'il ait sa place dans le tableau `LAYERS` (crates internes permis, crates externes interdits) et que chaque dépendance de la racine serve à quelqu'un. Preuve : un `anyhow = "1"` glissé dans `crates/wyck/Cargo.toml` fait échouer la commande. 4 tests unitaires. |
| 2026-10-14 | 3 | Décision : le layering vit dans `xtask/src/deps.rs` (tableau `LAYERS`) et non dans `deny.toml` (`wrappers`) : on ne sait pas si `wrappers` s'applique aux crates du workspace, et le tableau gère aussi les arêtes internes que cargo-deny ne voit pas. `deny.toml` reste pour les avis de sécurité, licences, sources et doublons. |
| 2026-10-14 | 3 | Décision : pas de nextest (voir tableau A/B), donc pas de `.config/nextest.toml`. Mesure faite avec la config réelle : `cargo nextest run --workspace` = 22,6 s, `cargo test --workspace` = 3 à 6 s. nextest a été désinstallé de ta machine après le test. |
| 2026-10-14 | 3 | Outils installés en local avec ton accord (`cargo install --locked`) : taplo-cli, typos-cli, cargo-machete, cargo-deny, zizmor ; actionlint 1.7.12 téléchargé depuis les releases de rhysd/actionlint dans un dossier du scratchpad (pas dans le PATH). Configs ajoutées : `.taplo.toml` (100 colonnes, indentation de `.editorconfig`, fixtures exclues), `_typos.toml` (le plan français et les assets exclus ; `PnL`, `clos`, `caf`, `BA`, `momen` sont voulus), `.github/zizmor.yml`. Résultats : `cargo xtask check --full` passe en entier (11 étapes) ; cargo-deny : avis, bans, licences et sources OK (la clé `allow-wildcard-paths` est donc valide) ; machete : aucune dépendance inutile ; zizmor en ligne et hors ligne : 0 problème ; actionlint : 0 problème sur les 3 workflows. |
| 2026-10-14 | 3 | Workflows réécrits (validés en local seulement, jamais exécutés sur GitHub) : action composite `.github/actions/setup-rust` (bibliothèques Linux, toolchain lue dans `rust-toolchain.toml`, cache optionnel) ; `ci.yml` = `changes` (liste blanche de chemins « docs », inconnu = code), `lint` (fmt, taplo, typos, machete, `xtask deps`), `workflows` (zizmor), `check` en 3 OS (clippy + `cargo test`), `docs`, `release-build`, `ci-ok` (reste le seul check requis ; échoue aussi si `changes` échoue) ; toutes les actions épinglées par SHA avec commentaire de version ; étapes nommées ; résumé de job avec le détail des tests ; `audit.yml` via `cargo deny` installé par install-action (plus d'action tierce dédiée) et surveillance de `**/Cargo.toml`. |
| 2026-10-14 | 3 | `release.yml` : tag `vX.Y.Z` strict ; `validate` vérifie la version du workspace, que le commit est sur `main` et **attend que `ci-ok` soit vert sur ce commit** (jusqu'à 45 min : lors de la release 0.2.1 le tag est parti 5 s après le push sur main, CI encore en cours) ; job `package` dans l'Environment `release` pour un tag et `release-dry-run` (sans secret) pour un lancement manuel ; **aucun cache** dans la release (un cache pourrait glisser autre chose que les sources dans un installeur signé) ; un lancement manuel est un essai à blanc avec une clé de signature jetable (`cargo packager signer generate --ci`) ; macOS Intel construit sur le runner Apple silicon (2 028 s contre 1 297 s auparavant, à valider par un essai à blanc) ; après publication, `gh attestation verify` sur chaque installeur. Abandonné : le workflow de préchauffage du cache de `cargo-packager` (gain d'environ 2 min 30 sur 18 à 34 min, pas assez pour un workflow de plus) et la règle CODEOWNERS (un seul propriétaire, elle ne change rien). |
| 2026-10-14 | 3 | Reportés à leur phase : `xtask docs-check` (Phase 7, quand les `AGENTS.md` existent), `xtask tasks` (Phase 10), la vérification d'une section de `CHANGELOG` à la release (Phase 9, quand le fichier existe). Zizmor demande la syntaxe `uses: $/.github/actions/...` (GitHub la supporte depuis juillet 2026) : désactivée pour l'instant dans `.github/zizmor.yml`, `./` marche partout et actionlint le connaît. Dependabot : cooldown par défaut remonté de 5 à 7 jours (demande de zizmor) et dossier des actions composites ajouté. |
| 2026-10-14 | 3 | Scripts Python de release portés en Rust (ta demande, commit ead2106) : `cargo xtask release name-assets` et `cargo xtask release manifest` remplacent `prepare_release_assets.py` et `generate_release_manifest.py` (supprimés). Une seule table de paquets au lieu de deux listes ; tag validé par la crate `semver` (stable `vX.Y.Z`), dépôt validé en `owner/name`, `sha2` pour `SHA256SUMS` (relu par `sha256sum -c`), `percent-encoding` pour les URL ; rien n'est écrit si un paquet ou une signature manque ; la commande est rejouable sans erreur. 9 tests. Les clés de plateforme (`macos-aarch64`, `windows-x86_64`...) et le champ `format` ont été vérifiés dans le code de `cargo-packager-updater` (le format n'est pas dans la clé, `macos` et non `darwin`). Il reste deux scripts Python, `make_brand_assets.py` et `make_sounds.py` (génération des icônes et des sons à la main, hors pipeline). |
| 2026-10-14 | 3 | Branche poussée avec ton accord (`refactor/workspace`, sans PR). Un push de branche ne lance aucun workflow (`ci.yml` écoute `main` et les PR), donc CI, audit et essai à blanc de la release sont lancés à la main (`gh workflow run ... --ref refactor/workspace`). Audit : vert en 20 s. |
| 2026-10-14 | 3 | **Premier essai à blanc de la release (run 38541841866) : échec utile.** Les 4 plateformes se construisent et se signent avec les nouveaux chemins (le déplacement dans `crates/wyck` et le `cargo-packager` marchent), mais l'étape de nommage ne trouve aucun installeur : `cargo-packager` lit sa config dans `crates/wyck` et prend un `--out-dir` relatif depuis ce dossier (les fichiers étaient dans `crates/wyck/dist/...`). Correction : dossier de sortie absolu (`github.workspace/dist/<plateforme>`) partagé par les étapes de paquet, de nommage et de dépôt. |
| 2026-10-14 | 3 | **Deuxième essai à blanc (run 38545272837) : succès.** Paquets de 13 à 47 Mo pour les 4 plateformes, nommés et vérifiés par `cargo xtask release name-assets`. Temps : Windows 1 126 s, Linux 908 s, macOS arm 798 s, macOS Intel **777 s** construit sur le runner Apple silicon (2 028 s auparavant sur `macos-15-intel`). La release complète passe d'environ 34 min à **19 min** (plus long job : Windows). Le job `publish` est bien sauté en essai à blanc. |
| 2026-10-14 | 3 | **Temps de la CI.** À froid sans cache : 25 min 52 (Windows 1 536 s, Linux 882 s, macOS 727 s ; clippy 698 s + tests 812 s sur Windows : les dépendances de `gpui` compilées en `opt-level = 3` coûtent cher). À froid avec écriture du cache : 17 min 38. **À chaud (run 38570514604)** : Windows 251 s, macOS 170 s, Linux 104 s, documentation 49 s, lint 46 s, build release 144 s, soit environ 4 min 10 de temps mural, **contre 3 min 40 avant** (Windows 215 s, macOS 174 s, Linux 107 s). **Conclusion honnête : à chaud, rien n'a été gagné** (+35 s sous Windows, ce qui est dans la variation des runners) : la CI d'avant était déjà bien réglée. Les gains de cette refonte sont ailleurs : une PR qui ne touche que la documentation saute les builds (filtre `changes` testé sur 10 listes de fichiers, un fichier inconnu compte comme du code), les tâches rapides (format, TOML, fautes, dépendances) tournent à part en moins d'une minute, la release est presque deux fois plus rapide, et tout est épinglé et audité. |
| 2026-10-14 | 3 | Piste de gain à chaud, pour la fin de la Phase 4 : quand le code sera en plusieurs crates, mettre `cache-workspace-crates: true` dans `rust-cache`. Aujourd'hui le crate unique de 127 000 lignes est recompilé en entier à chaque run (clippy 113 s et tests 70 s sous Windows) ; après le découpage, seuls le crate modifié et ses dépendants se recompileront. Le profil `ci` et le Dev Drive Windows ne valent plus la peine d'être mesurés avant : ils jouent surtout à froid, et le froid n'arrive qu'après un changement de `Cargo.lock`. |
| 2026-10-14 | 3 | **Réglages GitHub appliqués avec ton accord** (vérifiés ensuite par l'API) : signalement privé de vulnérabilités **activé** (le canal annoncé par `SECURITY.md`, `CODE_OF_CONDUCT.md` et le formulaire d'issues existe enfin) ; suppression automatique des branches fusionnées **activée**. Non appliqués, à ta demande : ruleset des tags `v*` et releases immuables. Toujours en attente : Environment `release` (tu dois y remettre toi-même la clé et le mot de passe de signature, GitHub ne les rend jamais à la lecture), `sha_pinning_required` (après la fusion dans `main`), scan de code. |
| 2026-10-14 | 3 | Reste à décider ou à faire : les **réglages GitHub**, un par un (signalement privé de vulnérabilités, Environment `release` avec les deux secrets de signature, `sha_pinning_required`, ruleset des tags `v*`, releases immuables, scan de code, suppression auto des branches fusionnées) ; le **profil `dist` (LTO fin)** : la release en LTO épais prend 19 min, le LTO fin la raccourcirait mais n'a pas été essayé ; une **PR de test** pour voir le filtre de documentation en vrai (il a été testé en local seulement). Abandonnés faute de gain à chaud : profil `ci` et Dev Drive. |

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
| Tests | `cargo test` (doc-tests inclus), pas nextest | Mesuré : nextest démarre un processus par test, 22,6 s contre 3,3 s pour les 1 189 mêmes tests sous Windows. Gain nul ici (un seul gros binaire de tests rapides) |
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