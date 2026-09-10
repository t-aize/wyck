/**
 * `@aurum/ctrader` — client MCP et schémas d'échange avec le serveur cTrader.
 *
 * Six domaines, volontairement séparés :
 *
 * 1. **protocol** — vocabulaire commun : enums ({@link OrderType}, {@link TradeSide}…),
 *    timeframes ({@link TRENDBAR_PERIODS}), record JSON permissif. Aucune idée de MCP.
 * 2. **account** — `get_balance` ({@link GetBalanceResult}).
 * 3. **catalog** — ce qui se trade : symboles, assets, spots, bougies.
 * 4. **book** — ce qui est ouvert : positions ({@link CtraderPosition}) et ordres
 *    ({@link CtraderOrder}). Lecture seule.
 * 5. **trading** — params sortants ({@link CreateOrderParams}…) et résultats
 *    d'écriture. Jamais un schéma strict sur un write : un parse qui échoue
 *    transformerait un ordre *réussi* en échec UI.
 * 6. **client** — {@link CtraderClient} : transport MCP, retry sur les lectures,
 *    **pas** de retry sur les écritures.
 *
 * Convention Params / Results :
 *
 * - **Params** (sortant) — `interface` TS, jamais validées à l'exécution :
 *   construites par l'app à partir de valeurs déjà typées par `tsc`.
 * - **Results** (entrant) — schémas zod, `safeParse` dans le client. Un payload
 *   inattendu devient {@link CtraderMcpError}, pas un plantage plus loin.
 *
 * L'app n'importe que cette façade. Les chemins internes (`client/client.ts`,
 * etc.) ne font pas partie de l'API.
 *
 * @packageDocumentation
 */

export type { GetBalanceResult } from "./account/schemas.ts";
export { GetBalanceResultSchema } from "./account/schemas.ts";
export type { CtraderOrder } from "./book/order.ts";
export { CtraderOrderSchema } from "./book/order.ts";
export type { AmendablePosition, ClosablePosition, CtraderPosition } from "./book/position.ts";
export { CtraderPositionSchema } from "./book/position.ts";
export type { GetPendingOrdersResult, GetPositionsResult } from "./book/schemas.ts";
export { GetPendingOrdersResultSchema, GetPositionsResultSchema } from "./book/schemas.ts";
export type {
  CtraderAsset,
  CtraderSpotPrice,
  CtraderSymbol,
  CtraderTrendbar,
  GetAssetsResult,
  GetSpotPricesParams,
  GetSpotPricesResult,
  GetSymbolsResult,
  GetTrendbarsParams,
  GetTrendbarsResult,
} from "./catalog/schemas.ts";
export {
  CtraderAssetSchema,
  CtraderSpotPriceSchema,
  CtraderSymbolSchema,
  CtraderTrendbarSchema,
  GetAssetsResultSchema,
  GetSpotPricesResultSchema,
  GetSymbolsResultSchema,
  GetTrendbarsResultSchema,
} from "./catalog/schemas.ts";
export { CtraderClient, type CtraderClientConfig } from "./client/client.ts";
export { CtraderMcpError } from "./client/error.ts";
export type { HistoricalOrderType, OrderType, TimeInForce, TradeSide } from "./protocol/enums.ts";
export {
  HistoricalOrderTypeSchema,
  OrderTypeSchema,
  TimeInForceSchema,
  TradeSideSchema,
} from "./protocol/enums.ts";
export { TRENDBAR_PERIOD_MS, TRENDBAR_PERIODS, type TrendbarPeriod } from "./protocol/period.ts";
export type {
  AmendOrderParams,
  AmendPositionParams,
  CancelOrderParams,
  ClosePositionParams,
  CreateOrderParams,
  OrderPriceFields,
} from "./trading/params.ts";
export type {
  AmendOrderResult,
  AmendPositionResult,
  CancelOrderResult,
  ClosePositionResult,
  CreateOrderResult,
} from "./trading/results.ts";
export {
  AmendOrderResultSchema,
  AmendPositionResultSchema,
  CancelOrderResultSchema,
  ClosePositionResultSchema,
  CreateOrderResultSchema,
} from "./trading/results.ts";
