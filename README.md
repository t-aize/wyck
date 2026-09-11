# aurum

Panel de trading terminal (TUI) multi-symboles, en TypeScript + [Bun](https://bun.sh) + [OpenTUI](https://opentui.com), connecté au MCP cTrader. Tout se pilote au clavier : `trade`, `amend`, `close`, etc. SL/TP/direction donnés à la main (ou dérivés de l'ATR), taille de position calculée depuis le risque en %.

Projet perso, privé, solo — pas de distribution publique prévue.

## ⚠️ Vrais ordres

Passe par le MCP officiel de cTrader (`mcp.ctrader.com`). Teste sur un **compte démo** avant un compte réel.

## Installation

```bash
bun install
```

Le calendrier économique vit dans `src/news`. Le client MCP cTrader vit dans `src/ctrader`.

## Configuration

Pas de `.env` : au premier lancement, l'app se rend même sans identifiants. Ensuite, dans le CommandBar :

1. `settings url` — URL du serveur MCP (`https://mcp.ctrader.com/trading/mcp` par défaut)
2. `settings token <token>` — cTrader Web → **Settings** → **Remote MCP**

Stocké (token chiffré) dans `~/.aurum/settings.json`. Si le token expire (401), régénère-le puis relance `settings token`.

Risque toujours en % de l'équity.

## Symbole

N'importe quel symbole exposé par le compte cTrader (XAUUSD, US100, BTCUSD, EURUSD…).

- Clic sur le nom du symbole (en-tête, à côté de **AURUM**) → liste filtrable
- `settings symbol <nom>` — ex. `settings symbol US100`

Le calendrier économique (ForexFactory) se filtre et calcule un biais en fonction de la classe d'actif (forex, métal, indice, crypto, énergie).

## Ressources

- [OpenTUI](https://opentui.com/docs/getting-started)
- [Effect](https://effect.website/docs)
- [cTrader Remote MCP](https://help.ctrader.com/ctrader-ai-agent-connect/remote-mcp/setup/)
