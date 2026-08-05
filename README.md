# aurum

Panel de trading en terminal pour **XAUUSD**, écrit en TypeScript + [Bun](https://bun.sh), avec une
UI [OpenTUI](https://opentui.com) et connecté en direct au serveur MCP de cTrader. Tout se pilote par une ligne de
commande façon shell : `trade`, `modify`, `cancel`. SL/TP/direction sont donnés à la main, la taille de position se
calcule automatiquement à partir du risque en % de l'équity.

Projet perso, privé, pensé pour un usage solo — pas d'objectif de distribution publique pour l'instant.

## ⚠️ À garder en tête

Ce panel passe de **vrais ordres** via le MCP officiel de cTrader (`mcp.ctrader.com`). Toujours tester sur un **compte
démo** avant un compte réel.

## Stack

Bun (runtime + bundler + compilation), TypeScript, OpenTUI (`@opentui/core`) pour le rendu
terminal, [MCP TypeScript SDK](https://github.com/modelcontextprotocol/typescript-sdk) (`StreamableHTTPClientTransport`)
pour parler au serveur MCP cTrader, Zod pour la validation.

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

## Structure SMC et prochain BOS/CHoCH

Le panneau SMC MTF STRUCTURE calcule, pour chaque timeframe (5M/15M/1H/4H/D1), le biais (higher high/higher low vs.
lower high/lower low), le dernier signal réalisé (BOS = cassure dans le sens de la tendance, CHoCH = cassure qui
l'inverse) et le **prochain** niveau encore surveillé de chaque côté (colonne NEXT), avec ce que sa cassure
produirait. Le biais global (ancrage D1+4H, confirmations 1H/15M/5M, avertissement calendrier) affiche aussi le
prochain niveau 4H à surveiller.

## Découvrir les tools MCP disponibles

La doc cTrader décrit son MCP sous forme de prompts en langage naturel, pas d'un schéma figé — comme le panel appelle
les tools directement (sans LLM), `scripts/list-tools.ts` liste ce qui est réellement exposé (même config que l'app,
lue dans `~/.aurum/config.json`) :

```bash
bun run mcp:tools
```

## Ressources

- OpenTUI : https://opentui.com/docs/getting-started
- cTrader Remote MCP (setup) : https://help.ctrader.com/ctrader-ai-agent-connect/remote-mcp/setup/
- cTrader Remote MCP (trading) : https://help.ctrader.com/ctrader-ai-agent-connect/remote-mcp/trading/
- MCP TypeScript SDK : https://github.com/modelcontextprotocol/typescript-sdk
- Bun — exécutables standalone : https://bun.com/docs/bundler/executables
