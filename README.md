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

Pas de `.env` : au premier lancement (dev ou `.exe` compilé), l'app affiche un écran de configuration qui demande
l'URL et le token du serveur MCP — dans **cTrader Web** → **Settings** → **Remote MCP** :

```json
{
  "url": "https://mcp.ctrader.com/trading/mcp",
  "headers": {
    "Authorization": "Bearer <TOKEN>"
  }
}
```

Une fois la connexion validée, la config est enregistrée (token chiffré) dans `~/.aurum/config.json` — indépendant du
dossier de lancement, donc valable aussi bien en `dev` que pour le binaire compilé déplacé n'importe où. La commande
`settings` dans l'app permet de la changer (URL/token) sans réinstaller.

Le risque d'un trade se calcule toujours en % de l'équity (pas de mode "montant fixe"). Le symbole (`XAUUSD`) est une
constante fixée dans `src/constants.ts` — pas de config, ce projet ne trade que XAUUSD.

Le token est lié à une session cTrader Web active : s'il expire (401), régénère-le depuis les mêmes réglages puis
lance `settings` dans l'app.

## Contexte macro (COT / dollar / taux réel)

Le panneau MACRO affiche le positionnement des gros spéculateurs sur l'or (COT, rapport
hebdomadaire de la CFTC — aucune clé requise) ainsi que le dollar index et le taux réel 10 ans
(FRED, données quotidiennes). Ces deux derniers demandent une clé API FRED, gratuite :

1. Crée un compte sur [fred.stlouisfed.org](https://fred.stlouisfed.org) puis génère une clé ici :
   https://fred.stlouisfed.org/docs/api/api_key.html
2. Dans l'app : `fred <ta clé>`

La clé est enregistrée (chiffrée) dans `~/.aurum/config.json`, comme l'URL/le token MCP. Sans
clé, le panneau affiche quand même le COT — seuls DXY et le taux réel restent vides.
Indicatif, pas un signal de trading : ni le COT ni le dollar/taux réel ne prédisent un sens,
ils donnent juste du contexte.

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
