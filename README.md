# aurum

Panel de trading en terminal pour **XAUUSD**, écrit en TypeScript + [Bun](https://bun.sh), avec une
UI [OpenTUI](https://opentui.com) et connecté en direct au serveur MCP de cTrader. Tout se pilote par une ligne de
commande façon shell : `trade`, `modify`, `cancel`. SL/TP/direction sont donnés à la main, la taille de position se
calcule automatiquement à partir du risque en % de l'équity.

Projet perso, privé, pensé pour un usage solo — pas d'objectif de distribution publique pour l'instant.

## ⚠️ À garder en tête

Ce panel passe de **vrais ordres** via le MCP officiel de cTrader (`mcp.ctrader.com`). Toujours tester sur un **compte
démo** avant un compte réel.

## Fonctionnalités

- **Prix en direct** — bid/ask XAUUSD avec sparkline (mini-graphe des dernières valeurs), solde et équity du compte.
- **Positions & ordres** — positions ouvertes et ordres en attente, P&L latent calculé en direct.
- **Calendrier économique** — publications ForexFactory de la semaine, filtrées/annotées pour leur pertinence sur
  l'or (impact, direction anticipée forecast vs previous).
- **Tendance multi-timeframe (SMC)** — biais M5/M15/H1 : structure (HH/HL vs LH/LL), événements BOS/CHoCH avec règle
  de confirmation CHoCH→BOS, liquidity sweep, prochains niveaux de structure encore surveillés, et deux filtres
  classiques de contexte (ADX + EMA stack — explicitement pas du SMC). Détails dans `src/domain/smc/trend.ts`. Passer
  un `trade` qui va à l'encontre du biais H1 confirmé déclenche un avertissement non bloquant.
- **Commandes CLI-style** — `trade`, `modify`, `cancel`, `risk`, `settings`, `refresh`, `clear`, `help` (cf.
  [Commandes](#commandes) ci-dessous), chaque envoi d'ordre passant par une popup de confirmation.
- **Raccourcis terminal** — surligner du texte le copie directement (OSC 52) ; `Ctrl+C` demande confirmation avant de
  quitter (armé 2s), ou vide la ligne de commande en cours si elle n'est pas vide.

## Stack

Bun (runtime + bundler + compilation), TypeScript, React 19 + OpenTUI (`@opentui/core`, `@opentui/react`) pour le
rendu terminal, [Effect](https://effect.website) (`effect`, `@effect/platform`, `@effect/platform-bun`) pour les
effets/erreurs typées et l'accès fichier, [MCP TypeScript SDK](https://github.com/modelcontextprotocol/typescript-sdk)
(`StreamableHTTPClientTransport`) pour parler au serveur MCP cTrader, Zod v4 pour la validation. Biome (lint +
format) et Husky (pre-commit) pour la qualité de code.

## Installation

```bash
bun install
```

## Configuration

Pas de `.env` : au premier lancement (dev ou `.exe` compilé), l'app affiche un écran de configuration en 2 étapes,
toutes obligatoires :

1. **URL du serveur MCP** — l'URL cTrader (`https://mcp.ctrader.com/trading/mcp` par défaut).
2. **Token MCP** — dans **cTrader Web** → **Settings** → **Remote MCP** :
   ```json
   {
     "url": "https://mcp.ctrader.com/trading/mcp",
     "headers": {
       "Authorization": "Bearer <TOKEN>"
     }
   }
   ```
   La connexion est vérifiée en direct avant d'être acceptée.

Une fois les deux validées, la config est enregistrée (chiffrée) dans `~/.aurum/config.json` — indépendant du
dossier de lancement, donc valable aussi bien en `dev` que pour le binaire compilé déplacé n'importe où. La commande
`settings` dans l'app permet de tout reconfigurer (URL/token) sans réinstaller — les deux champs sont toujours
redemandés et réécrits ensemble, jamais un sous-ensemble, pour ne jamais perdre un champ en reconfigurant l'autre.

Le risque d'un trade se calcule toujours en % de l'équity (pas de mode "montant fixe"). Le symbole (`XAUUSD`) est une
constante fixée dans `src/constants.ts` — pas de config, ce projet ne trade que XAUUSD.

Le token est lié à une session cTrader Web active : s'il expire (401), régénère-le depuis les mêmes réglages puis
lance `settings` dans l'app.

## Commandes

Toutes les commandes se tapent dans la barre en bas de l'écran. `help` liste les commandes, `help <commande>` détaille
l'usage d'une commande précise.

| Commande | Usage | Description |
|---|---|---|
| `trade` | `trade [<risque%>] <entrée\|market> <sl> <tp>` | Direction déduite du SL/TP, taille de position dérivée du risque%. `risque%` optionnel si un défaut est défini avec `risk`. Popup de confirmation avant envoi. |
| `modify` | `modify <id> [--sl <prix>] [--tp <prix>]` | Modifie le SL et/ou le TP d'un ordre en attente. |
| `cancel` | `cancel <id> [id...]` ou `cancel all` | Annule un ou plusieurs ordres en attente. |
| `risk` | `risk <risque%>` | Règle un risque% par défaut pour `trade` (valable pour la session, jamais persisté). |
| `settings` | `settings` | Reconfigure l'URL/le token MCP. |
| `refresh` | `refresh` | Force une actualisation immédiate du marché, du calendrier et de la tendance. |
| `clear` | `clear` | Efface le message de feedback. |
| `help` | `help [commande]` | Liste les commandes, ou détaille l'usage d'une commande précise. |

## Développement

```bash
bun run dev        # démarre l'app en mode watch
bun run typecheck  # tsc --noEmit
bun run lint       # biome check .
bun run lint:fix   # biome check --write .
bun run test       # bun test
bun run verify     # typecheck + lint + test
bun run build      # binaire standalone compilé (bun build --compile)
```

Husky (`.husky/`) fait tourner typecheck + lint avant chaque commit, et les tests avant chaque push.

CI (GitHub Actions, `.github/workflows/ci.yml`) : typecheck, lint, tests et vérification que le build compile, sur
chaque push sur `main` et chaque pull request. Release (`.github/workflows/release.yml`) : déclenchée par un tag
`vX.Y.Z` (doit correspondre à la `version` de `package.json`), rejoue la même vérification puis publie le binaire
Windows compilé (avec checksum SHA-256) en release GitHub.

## Ressources

- OpenTUI : https://opentui.com/docs/getting-started
- Effect : https://effect.website/docs
- cTrader Remote MCP (setup) : https://help.ctrader.com/ctrader-ai-agent-connect/remote-mcp/setup/
- cTrader Remote MCP (trading) : https://help.ctrader.com/ctrader-ai-agent-connect/remote-mcp/trading/
- MCP TypeScript SDK : https://github.com/modelcontextprotocol/typescript-sdk
- Bun — exécutables standalone : https://bun.com/docs/bundler/executables
