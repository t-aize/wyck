# aurum

Panel de trading terminal (TUI) pour **XAUUSD**, en TypeScript + [Bun](https://bun.sh) + [OpenTUI](https://opentui.com), connecté au MCP cTrader. Tout se pilote au clavier : `trade`, `modify`, `cancel`, etc. SL/TP/direction donnés à la main (ou dérivés de l'ATR), taille de position calculée depuis le risque en %.

Projet perso, privé, solo — pas de distribution publique prévue.

## ⚠️ Vrais ordres

Passe par le MCP officiel de cTrader (`mcp.ctrader.com`). Teste sur un **compte démo** avant un compte réel.

## Installation

```bash
bun install
```

## Configuration

Pas de `.env` : au premier lancement, écran de config en 2 étapes (redemandable avec `settings` dans l'app) :

1. URL du serveur MCP (`https://mcp.ctrader.com/trading/mcp` par défaut)
2. Token MCP — cTrader Web → **Settings** → **Remote MCP**

Stocké (chiffré) dans `~/.aurum/config.json`. Si le token expire (401), régénère-le puis relance `settings`.

Risque toujours en % de l'équity. Symbole fixé à `XAUUSD` (`src/constants.ts`), pas configurable.

## Ressources

- [OpenTUI](https://opentui.com/docs/getting-started)
- [Effect](https://effect.website/docs)
- [cTrader Remote MCP](https://help.ctrader.com/ctrader-ai-agent-connect/remote-mcp/setup/)
