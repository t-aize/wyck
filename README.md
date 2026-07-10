# aurum

Panel de trading en terminal pour **XAUUSD**, écrit en TypeScript + [Bun](https://bun.sh), avec une
UI [OpenTUI](https://opentui.com) et connecté en direct au serveur MCP de cTrader. Trois champs à remplir — **entrée**,*
*direction**, **risque** — le reste (SL sur l'ATR, TP en RR, taille de position) est calculé automatiquement.

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

## Utilisation

```bash
bun run dev     # hot reload
bun run start   # normal
```

| Touche  | Action                                     |
|---------|--------------------------------------------|
| `Tab`   | Naviguer entre les champs                  |
| `Enter` | Calculer SL/TP/volume et afficher le récap |
| `Y`     | Confirmer et envoyer l'ordre               |
| `Esc`   | Annuler                                    |
| `Q`     | Quitter                                    |

## Compilation

```bash
bun build ./src/index.ts --compile --minify --outfile ./dist/aurum
```

`.env` doit rester à côté du binaire au lancement (il n'est pas embarqué dedans). Pour cross-compiler, ajouter
`--target=bun-linux-x64` / `bun-darwin-arm64` / `bun-windows-x64`.

## Découvrir les tools MCP disponibles

La doc cTrader décrit son MCP sous forme de prompts en langage naturel, pas d'un schéma figé — comme le panel appelle
les tools directement (sans LLM), un petit script pour lister ce qui est réellement exposé est utile avant d'implémenter
le client :

```ts
// scripts/list-tools.ts
import {Client} from "@modelcontextprotocol/sdk/client/index.js";
import {StreamableHTTPClientTransport} from "@modelcontextprotocol/sdk/client/streamableHttp.js";

const transport = new StreamableHTTPClientTransport(
    new URL(process.env.CTRADER_MCP_URL!),
    {requestInit: {headers: {Authorization: `Bearer ${process.env.CTRADER_MCP_TOKEN}`}}},
);

const client = new Client({name: "aurum", version: "0.1.0"});
await client.connect(transport);

const {tools} = await client.listTools();
for (const tool of tools) {
    console.log(tool.name, "→", tool.description);
    console.log(JSON.stringify(tool.inputSchema, null, 2));
}

await transport.close();
```

```bash
bun run scripts/list-tools.ts
```

## Roadmap

- [ ] Formulaire (entrée / direction / risque) + validation
- [ ] ATR + SL/TP + sizing
- [ ] Écran de confirmation
- [ ] Tableau des positions ouvertes avec P&L live
- [ ] RR configurable par trade (pas juste en `.env`)
- [ ] Historique local des ordres passés

## Ressources

- OpenTUI : https://opentui.com/docs/getting-started
- cTrader Remote MCP (setup) : https://help.ctrader.com/ctrader-ai-agent-connect/remote-mcp/setup/
- cTrader Remote MCP (trading) : https://help.ctrader.com/ctrader-ai-agent-connect/remote-mcp/trading/
- MCP TypeScript SDK : https://github.com/modelcontextprotocol/typescript-sdk
- Bun — exécutables standalone : https://bun.com/docs/bundler/executables
