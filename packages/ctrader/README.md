# `@aurum/ctrader`

Paquet **privé** : client MCP cTrader et schémas d'échange (params sortants,
results entrants).

Rien ici n'est public. `private: true` empêche un `npm publish` accidentel.
`license: "UNLICENSED"` (identifiant SPDX) signifie **aucune licence accordée à
autrui** — c'est la convention npm pour du code propriétaire, à côté du
`LICENSE` racine (« tous droits réservés »). Les deux se complètent : `private`
bloque la publication, `UNLICENSED` documente l'absence de droit de réutilisation.

## Convention Params / Results

Deux mondes, pas un mélange accidentel :

- **Params** (sortant) — `interface` TS, jamais validées à l'exécution. Construites
  par l'app à partir de valeurs déjà typées par `tsc`.
- **Results** (entrant) — schémas zod, `safeParse` dans `CtraderClient`. Un payload
  inattendu devient `CtraderMcpError`, pas un plantage plus loin dans l'UI.
- **Positions et writes** restent des records permissifs. Un schéma strict qui se
  trompe planterait l'affichage, ou transformerait un ordre *réussi* en échec
  apparent. Les lectures confirmées (symboles, spots, bougies, ordres, balance)
  sont des `z.object` stricts.

Les lectures (`get_*`) ont un timeout 10 s et un retry exponentiel, uniquement
sur un échec de transport. Les écritures n'ont **pas** de retry : rejouer un
`create_order` après un timeout risquerait de dupliquer l'ordre.

## Architecture

```
src/
  index.ts                 façade publique (réexport uniquement)
  protocol/                vocabulaire — aucune idée de MCP
    enums.ts               OrderType, TradeSide, TimeInForce, HistoricalOrderType
    period.ts              TRENDBAR_PERIODS / TRENDBAR_PERIOD_MS
    record.ts              record JSON permissif + lecteurs
  account/                 get_balance
    schemas.ts
  catalog/                 ce qui se trade
    schemas.ts             symboles, assets, spots, bougies
  book/                    ce qui est ouvert (lecture)
    position.ts            CtraderPosition (permissif + transform)
    order.ts               CtraderOrder (forme confirmée)
    schemas.ts             get_positions / get_pending_orders
  trading/                 mutation du book
    params.ts              CreateOrderParams, Amend*, Cancel*, Close*
    results.ts             résultats d'écriture (record permissif)
  client/
    error.ts               CtraderMcpError (retryable)
    client.ts              CtraderClient — MCP, retry lectures
```

Les tests collent au dossier qu'ils couvrent (`book/position.test.ts`).

## Usage (côté app)

```ts
import { CtraderClient, type CreateOrderParams } from "@aurum/ctrader";

const client = new CtraderClient({ url, token });
await client.connect();
const { equity, moneyDigits } = await runtime.runPromise(client.getBalance());
const { symbols } = await runtime.runPromise(client.getSymbols());
await client.close();
```
