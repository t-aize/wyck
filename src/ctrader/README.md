# `src/ctrader`

Client MCP cTrader et types d'échange.

## Pas de Zod

Le SDK MCP valide déjà l'enveloppe (`content[]`, `isError`, bloc texte). Le JSON
*métier* est typé en TypeScript (`enum` / `interface` / `type`) et lu tel quel.
Zod ici recopiait le contrat du serveur sans l'améliorer.

Seule exception : `mapPosition` — un **mapper** (`positionId` → `id`,
`entryPrice` → `entry`), pas un schéma. Un objet pourri donne des `undefined`,
jamais un crash du panneau.

Un fichier par enum, interface ou type.

## Architecture

```
src/ctrader/
  protocol/                     vocabulaire filaire (enums string)
    TradeSide.ts
    OrderType.ts
    TimeInForce.ts
    HistoricalOrderType.ts
    TrendbarPeriod.ts           + TRENDBAR_PERIODS / _MS
  account/GetBalanceResult.ts
  catalog/                      ce qui se trade
    CtraderSymbol.ts
    CtraderAsset.ts
    CtraderSpotPrice.ts
    CtraderTrendbar.ts
    Get*Params.ts / Get*Result.ts
  book/                         ce qui est ouvert
    CtraderPosition.ts          interface + mapPosition
    AmendablePosition.ts
    ClosablePosition.ts
    CtraderOrder.ts
    GetPositionsResult.ts
    GetPendingOrdersResult.ts
  trading/                      mutation du book
    OrderPriceFields.ts
    CreateOrderParams.ts / Result.ts
    Amend* / Cancel* / Close*
    WriteResult.ts
  client/
    CtraderClientConfig.ts
    CtraderMcpError.ts
    CtraderClient.ts            retry lectures, pas de retry writes
```

Les tests collent au dossier (`book/mapPosition.test.ts`).

## Usage

```ts
import { CtraderClient } from "./ctrader/client/CtraderClient.ts";
import { TradeSide } from "./ctrader/protocol/TradeSide.ts";
import type { CreateOrderParams } from "./ctrader/trading/CreateOrderParams.ts";

const client = new CtraderClient({ url, token });
await client.connect();
const { equity } = await runtime.runPromise(client.getBalance());
```
