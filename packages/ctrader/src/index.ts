/**
 * `@aurum/ctrader` — client MCP et types d'échange avec le serveur cTrader.
 *
 * Un fichier par enum / interface / type. Pas de Zod : le SDK MCP valide
 * l'enveloppe (`content[]`, `isError`) ; le JSON métier est typé en TypeScript
 * et lu tel quel. Seule exception : {@link mapPosition}, un mapper (renommage
 * de champs), pas un schéma.
 *
 * Six domaines :
 *
 * 1. **protocol** — {@link OrderType}, {@link TradeSide}, {@link TimeInForce},
 *    {@link HistoricalOrderType}, {@link TrendbarPeriod}.
 * 2. **account** — {@link GetBalanceResult}.
 * 3. **catalog** — symboles, assets, spots, bougies.
 * 4. **book** — positions ({@link CtraderPosition}) et ordres ({@link CtraderOrder}).
 * 5. **trading** — params sortants et {@link WriteResult}.
 * 6. **client** — {@link CtraderClient} : retry lectures, **pas** de retry writes.
 *
 * L'app n'importe que cette façade.
 *
 * @packageDocumentation
 */

export type { GetBalanceResult } from "./account/GetBalanceResult.ts";
export type { AmendablePosition } from "./book/AmendablePosition.ts";
export type { ClosablePosition } from "./book/ClosablePosition.ts";
export type { CtraderOrder } from "./book/CtraderOrder.ts";
export { type CtraderPosition, mapPosition } from "./book/CtraderPosition.ts";
export type { GetPendingOrdersResult } from "./book/GetPendingOrdersResult.ts";
export { type GetPositionsResult, mapGetPositionsResult } from "./book/GetPositionsResult.ts";
export type { CtraderAsset } from "./catalog/CtraderAsset.ts";
export type { CtraderSpotPrice } from "./catalog/CtraderSpotPrice.ts";
export type { CtraderSymbol } from "./catalog/CtraderSymbol.ts";
export type { CtraderTrendbar } from "./catalog/CtraderTrendbar.ts";
export type { GetAssetsResult } from "./catalog/GetAssetsResult.ts";
export type { GetSpotPricesParams } from "./catalog/GetSpotPricesParams.ts";
export type { GetSpotPricesResult } from "./catalog/GetSpotPricesResult.ts";
export type { GetSymbolsResult } from "./catalog/GetSymbolsResult.ts";
export type { GetTrendbarsParams } from "./catalog/GetTrendbarsParams.ts";
export type { GetTrendbarsResult } from "./catalog/GetTrendbarsResult.ts";
export { CtraderClient } from "./client/CtraderClient.ts";
export type { CtraderClientConfig } from "./client/CtraderClientConfig.ts";
export { CtraderMcpError } from "./client/CtraderMcpError.ts";
export { HistoricalOrderType } from "./protocol/HistoricalOrderType.ts";
export { OrderType } from "./protocol/OrderType.ts";
export { TimeInForce } from "./protocol/TimeInForce.ts";
export { TradeSide } from "./protocol/TradeSide.ts";
export { TRENDBAR_PERIOD_MS, TRENDBAR_PERIODS, TrendbarPeriod } from "./protocol/TrendbarPeriod.ts";
export type { AmendOrderParams } from "./trading/AmendOrderParams.ts";
export type { AmendOrderResult } from "./trading/AmendOrderResult.ts";
export type { AmendPositionParams } from "./trading/AmendPositionParams.ts";
export type { AmendPositionResult } from "./trading/AmendPositionResult.ts";
export type { CancelOrderParams } from "./trading/CancelOrderParams.ts";
export type { CancelOrderResult } from "./trading/CancelOrderResult.ts";
export type { ClosePositionParams } from "./trading/ClosePositionParams.ts";
export type { ClosePositionResult } from "./trading/ClosePositionResult.ts";
export type { CreateOrderParams } from "./trading/CreateOrderParams.ts";
export type { CreateOrderResult } from "./trading/CreateOrderResult.ts";
export type { OrderPriceFields } from "./trading/OrderPriceFields.ts";
export type { WriteResult } from "./trading/WriteResult.ts";
