/**
 * Schémas zod des réponses du serveur MCP cTrader, et types dérivés (`z.infer`).
 *
 * `CtraderOrder`/`CtraderDeal` ont été vérifiés contre de vrais payloads
 * (get_order_history / get_deals, compte réel, symbole XAUUSD). `CtraderPosition`,
 * `GetPositionDetailsResult` et les résultats d'écriture restent volontairement
 * `z.record(z.string(), z.unknown())` : aucune position n'était ouverte lors du
 * dernier test (10/07/2026) et il n'y a pas de compte démo pour en ouvrir une sans
 * risque ; les outils d'écriture n'ont jamais été exercés sur le compte réel.
 *
 * `CtraderPosition` reste délibérément permissif plutôt que d'être verrouillé sur la
 * base du seul recoupement avec le proto Open API (cf. ctrader/mappers.ts) : un schéma
 * strict qui se trompe ferait *planter* l'affichage des positions (échec `safeParse` →
 * `CtraderMcpError`), ce qui est pire pour un panel de trading que des valeurs
 * possiblement fausses mais visibles — cf. l'avertissement affiché par
 * `PositionsPanel.tsx` quand le mapping échoue. Les schémas d'écriture, eux, sont bas
 * risque même non verrouillés : `useOrderActions.ts` ne lit jamais leurs champs, il ne
 * regarde que succès/échec de la promesse.
 */

import { z } from "zod";
import { TRENDBAR_PERIODS } from "../constants.ts";

// ─── Enums ───────────────────────────────────────────────────────────────

export const OrderTypeSchema = z.enum(["MARKET", "LIMIT", "STOP", "MARKET_RANGE", "STOP_LIMIT"]);
export type OrderType = z.infer<typeof OrderTypeSchema>;

export const TradeSideSchema = z.enum(["BUY", "SELL"]);
export type TradeSide = z.infer<typeof TradeSideSchema>;

export const TimeInForceSchema = z.enum([
  "GOOD_TILL_CANCEL",
  "GOOD_TILL_DATE",
  "IMMEDIATE_OR_CANCEL",
]);
export type TimeInForce = z.infer<typeof TimeInForceSchema>;

/**
 * Type d'ordre tel que renvoyé par get_order_history, en plus de `OrderType` : le
 * serveur y inclut aussi les ordres SL/TP générés automatiquement pour une position.
 */
export const HistoricalOrderTypeSchema = z.union([
  OrderTypeSchema,
  z.literal("STOP_LOSS_TAKE_PROFIT"),
]);
export type HistoricalOrderType = z.infer<typeof HistoricalOrderTypeSchema>;

// ─── Paramètres (fidèles au JSON Schema exposé par le serveur) ─────────────
// Jamais validés à l'exécution : construits par ce client lui-même, pas reçus du réseau.

export interface GetSpotPricesParams {
  /** IDs des symboles */
  symbolId: number[];
}

export interface GetTrendbarsParams {
  symbolId: number;
  period: (typeof TRENDBAR_PERIODS)[number];
  /**
   * Combinaisons valides : (count) → N dernières bougies ; (toTimestamp, count) →
   * N bougies se terminant à toTimestamp ; (fromTimestamp, toTimestamp) → toutes
   * les bougies sur la plage (≤ 720h). En pratique seule la 3e combinaison s'est
   * montrée fiable lors des tests — les deux autres ont renvoyé une erreur 400
   * côté serveur malgré une requête conforme au schéma annoncé.
   */
  fromTimestamp?: string;
  toTimestamp?: string;
  count?: number;
}

export interface GetPositionDetailsParams {
  positionId: number;
}

export interface GetOrderHistoryParams {
  /** Epoch ms ou ISO-8601. Plage plafonnée à 720h (30 jours) côté serveur. */
  fromTimestamp: string;
  toTimestamp: string;
}

export interface GetDealsParams {
  fromTimestamp: string;
  toTimestamp: string;
  /** Défaut serveur : 50 */
  maxRows?: number;
}

export interface AmendPositionParams {
  positionId: number;
  /** Nouveau SL en prix affiché (pas en pipettes) ; omis = inchangé */
  stopLoss?: number;
  /** Nouveau TP en prix affiché (pas en pipettes) ; omis = inchangé */
  takeProfit?: number;
  trailingStopLoss?: boolean;
}

export interface ClosePositionParams {
  positionId: number;
  /** Volume à clôturer en 1/100 d'unité d'actif de base (volume = lots × lotSize × 100) */
  volume: number;
}

export interface CreateOrderParams {
  symbolId: number;
  orderType: OrderType;
  tradeSide: TradeSide;
  /**
   * Volume en 1/100 d'unité d'actif de base (volume = lots × lotSize × 100).
   * lotSize dépend de la classe d'actif : forex = 100000, métaux = 100 (XAUUSD :
   * 1 lot = 10 000), indices/crypto = 1. Ne pas réutiliser la valeur forex pour
   * les autres classes.
   */
  volume: number;
  /** Requis pour LIMIT, STOP_LIMIT */
  limitPrice?: number;
  /** Requis pour STOP, STOP_LIMIT */
  stopPrice?: number;
  /** Prix absolu ; supporté sur LIMIT/STOP/STOP_LIMIT, PAS sur MARKET/MARKET_RANGE */
  stopLoss?: number;
  /** Prix absolu ; supporté sur LIMIT/STOP/STOP_LIMIT, PAS sur MARKET/MARKET_RANGE */
  takeProfit?: number;
  /** Distance en points depuis le prix d'exécution ; requis pour MARKET/MARKET_RANGE. Exclusif avec stopLoss. */
  relativeStopLoss?: number;
  /** Distance en points depuis le prix d'exécution ; requis pour MARKET/MARKET_RANGE. Exclusif avec takeProfit. */
  relativeTakeProfit?: number;
  comment?: string;
  label?: string;
  timeInForce?: TimeInForce;
  baseSlippagePrice?: number;
  slippageInPoints?: number;
  /** Epoch ms (entier uniquement, pas d'ISO-8601 ici) */
  expirationTimestamp?: number;
}

export interface AmendOrderParams {
  orderId: number;
  volume?: number;
  limitPrice?: number;
  stopPrice?: number;
  /** Exclusif avec relativeStopLoss */
  stopLoss?: number;
  /** Exclusif avec relativeTakeProfit */
  takeProfit?: number;
  /** Exclusif avec stopLoss */
  relativeStopLoss?: number;
  /** Exclusif avec takeProfit */
  relativeTakeProfit?: number;
  expirationTimestamp?: number;
}

export interface CancelOrderParams {
  orderId: number;
}

// ─── Résultats ───────────────────────────────────────────────────────────

/** Payload non vérifié contre un payload réel — cf. commentaire en tête de fichier. */
const UnverifiedPayloadSchema = z.record(z.string(), z.unknown());

export const GetVersionResultSchema = z.object({
  service: z.string(),
  version: z.string(),
  springBootVersion: z.string(),
  javaVersion: z.string(),
  buildTime: z.string(),
});
export type GetVersionResult = z.infer<typeof GetVersionResultSchema>;

export const GetBalanceResultSchema = z.object({
  /** Valeur entière à l'échelle `moneyDigits` (ex: moneyDigits=2 → centimes) */
  balance: z.number(),
  equity: z.number(),
  freeMargin: z.number(),
  balanceVersion: z.number(),
  moneyDigits: z.number(),
  depositAssetId: z.number(),
});
export type GetBalanceResult = z.infer<typeof GetBalanceResultSchema>;

export const CtraderAssetSchema = z.object({
  assetId: z.number(),
  name: z.string(),
  displayName: z.string(),
});
export type CtraderAsset = z.infer<typeof CtraderAssetSchema>;

export const GetAssetsResultSchema = z.object({ assets: z.array(CtraderAssetSchema) });
export type GetAssetsResult = z.infer<typeof GetAssetsResultSchema>;

export const CtraderSymbolSchema = z.object({
  symbolId: z.number(),
  symbolName: z.string(),
  enabled: z.boolean(),
  baseAssetId: z.number(),
  quoteAssetId: z.number(),
  symbolCategoryId: z.number(),
  description: z.string(),
});
export type CtraderSymbol = z.infer<typeof CtraderSymbolSchema>;

export const GetSymbolsResultSchema = z.object({ symbols: z.array(CtraderSymbolSchema) });
export type GetSymbolsResult = z.infer<typeof GetSymbolsResultSchema>;

export const CtraderSpotPriceSchema = z.object({
  symbolId: z.number(),
  /** Prix à l'échelle x10^5 (ex: 410177000 → 4101.77) */
  bid: z.number(),
  ask: z.number(),
  high: z.number(),
  low: z.number(),
  sessionClose: z.number(),
  timestamp: z.number(),
});
export type CtraderSpotPrice = z.infer<typeof CtraderSpotPriceSchema>;

export const GetSpotPricesResultSchema = z.object({ prices: z.array(CtraderSpotPriceSchema) });
export type GetSpotPricesResult = z.infer<typeof GetSpotPricesResultSchema>;

export const CtraderTrendbarSchema = z.object({
  timestamp: z.number(),
  /** Prix à l'échelle x10^5 */
  open: z.number(),
  high: z.number(),
  low: z.number(),
  close: z.number(),
  volume: z.number(),
});
export type CtraderTrendbar = z.infer<typeof CtraderTrendbarSchema>;

export const GetTrendbarsResultSchema = z.object({
  trendbars: z.array(CtraderTrendbarSchema),
  symbolId: z.number(),
  period: z.enum(TRENDBAR_PERIODS),
});
export type GetTrendbarsResult = z.infer<typeof GetTrendbarsResultSchema>;

// TODO(payload): jamais exercé contre un payload réel — cf. ctrader/mappers.ts pour les
// noms de champs à haute confiance déduits du proto Open API en attendant.
export const CtraderPositionSchema = UnverifiedPayloadSchema;
export type CtraderPosition = z.infer<typeof CtraderPositionSchema>;

/**
 * Vérifié via get_order_history (10 ordres réels, XAUUSD). Les champs propres aux
 * ordres *en attente* (label, comment, timeInForce, statut) restent non vérifiés :
 * aucun ordre pending observé lors du test. expirationTimestamp est repris malgré
 * tout (même nom que dans CreateOrderParams/AmendOrderParams) — sans lui, un amend
 * (manuel ou auto ATR, cf. useModifyConfirm.ts/useAtrOrderTracking.ts) ne peut pas
 * le renvoyer et cTrader l'efface silencieusement.
 */
export const CtraderOrderSchema = z.object({
  orderId: z.number(),
  symbolId: z.number(),
  orderType: HistoricalOrderTypeSchema,
  tradeSide: TradeSideSchema,
  /** 1/100 d'unité d'actif de base, comme dans CreateOrderParams */
  volume: z.number(),
  /** Prix affiché, présent sur LIMIT/STOP_LIMIT */
  limitPrice: z.number().optional(),
  /** Prix affiché, présent sur STOP/STOP_LIMIT */
  stopPrice: z.number().optional(),
  stopLoss: z.number().optional(),
  takeProfit: z.number().optional(),
  /** Epoch ms, comme dans CreateOrderParams/AmendOrderParams */
  expirationTimestamp: z.number().optional(),
});
export type CtraderOrder = z.infer<typeof CtraderOrderSchema>;

/** Vérifié via get_deals (4 deals réels, XAUUSD). */
export const CtraderDealSchema = z.object({
  dealId: z.number(),
  orderId: z.number(),
  positionId: z.number(),
  symbolId: z.number(),
  tradeSide: TradeSideSchema,
  volume: z.number(),
  filledVolume: z.number(),
  /** Prix affiché (pas à l'échelle x10^5, contrairement à CtraderSpotPrice/CtraderTrendbar) */
  executionPrice: z.number(),
  /** Epoch ms */
  executionTimestamp: z.number(),
  /** Seule valeur observée : "FILLED". Les autres statuts possibles ne sont pas vérifiés. */
  dealStatus: z.string(),
  /** Valeur signée à l'échelle moneyDigits (négatif = coût) */
  commission: z.number(),
});
export type CtraderDeal = z.infer<typeof CtraderDealSchema>;

export const GetPositionsResultSchema = z.object({
  positions: z.array(CtraderPositionSchema),
  orders: z.array(CtraderOrderSchema),
});
export type GetPositionsResult = z.infer<typeof GetPositionsResultSchema>;

export const GetPendingOrdersResultSchema = z.object({
  orders: z.array(CtraderOrderSchema),
  hasMore: z.boolean(),
});
export type GetPendingOrdersResult = z.infer<typeof GetPendingOrdersResultSchema>;

// TODO(payload): structure non vérifiée en détail (jamais appelé, aucune position disponible).
export const GetPositionDetailsResultSchema = UnverifiedPayloadSchema;
export type GetPositionDetailsResult = z.infer<typeof GetPositionDetailsResultSchema>;

export const GetOrderHistoryResultSchema = z.object({
  orders: z.array(CtraderOrderSchema),
  hasMore: z.boolean(),
});
export type GetOrderHistoryResult = z.infer<typeof GetOrderHistoryResultSchema>;

export const GetDealsResultSchema = z.object({
  deals: z.array(CtraderDealSchema),
  hasMore: z.boolean(),
});
export type GetDealsResult = z.infer<typeof GetDealsResultSchema>;

/**
 * Résultats non vérifiés : ces outils modifient un compte réel et n'ont
 * volontairement jamais été exercés pendant l'implémentation.
 */
// TODO(payload): idem, jamais exercé sur le compte réel.
export const AmendPositionResultSchema = UnverifiedPayloadSchema;
export type AmendPositionResult = z.infer<typeof AmendPositionResultSchema>;
// TODO(payload): idem, jamais exercé sur le compte réel.
export const ClosePositionResultSchema = UnverifiedPayloadSchema;
export type ClosePositionResult = z.infer<typeof ClosePositionResultSchema>;
// TODO(payload): idem, jamais exercé sur le compte réel.
export const CreateOrderResultSchema = UnverifiedPayloadSchema;
export type CreateOrderResult = z.infer<typeof CreateOrderResultSchema>;
// TODO(payload): idem, jamais exercé sur le compte réel.
export const AmendOrderResultSchema = UnverifiedPayloadSchema;
export type AmendOrderResult = z.infer<typeof AmendOrderResultSchema>;
// TODO(payload): idem, jamais exercé sur le compte réel.
export const CancelOrderResultSchema = UnverifiedPayloadSchema;
export type CancelOrderResult = z.infer<typeof CancelOrderResultSchema>;
