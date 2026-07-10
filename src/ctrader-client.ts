/**
 * Client MCP typé pour le serveur cTrader (`mcp.ctrader.com`).
 *
 * Usage :
 * ```ts
 * const client = new CtraderClient();
 * await client.connect();
 * const { balance, equity } = await client.getBalance();
 * await client.close();
 * ```
 *
 * `CtraderOrder` et `CtraderDeal` ont été vérifiés contre de vrais payloads
 * (get_order_history / get_deals, compte réel, symbole XAUUSD). `CtraderPosition`
 * et `GetPositionDetailsResult` restent des types marqués « non vérifié » :
 * aucune position n'était ouverte lors du dernier test (10/07/2026) et il n'y a
 * pas de compte démo pour en ouvrir une sans risque. Les outils d'écriture
 * (amendPosition/closePosition/createOrder/amendOrder/cancelOrder) restent eux
 * aussi jamais exercés sur le compte réel. Les paramètres, eux, sont fidèles au
 * JSON Schema exposé par le serveur.
 */

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { CallToolResultSchema } from "@modelcontextprotocol/sdk/types.js";
import type { TrendbarPeriod } from "./ctrader-types.ts";
import { env } from "./env.ts";

export type { TrendbarPeriod } from "./ctrader-types.ts";
export { TRENDBAR_PERIODS } from "./ctrader-types.ts";

const CLIENT_INFO = { name: "aurum", version: "0.1.0" };

// ─── Enums ───────────────────────────────────────────────────────────────

export type OrderType = "MARKET" | "LIMIT" | "STOP" | "MARKET_RANGE" | "STOP_LIMIT";
export type TradeSide = "BUY" | "SELL";
export type TimeInForce = "GOOD_TILL_CANCEL" | "GOOD_TILL_DATE" | "IMMEDIATE_OR_CANCEL";

// ─── Paramètres (fidèles au JSON Schema exposé par le serveur) ─────────────

export interface GetSpotPricesParams {
  /** IDs des symboles */
  symbolId: number[];
}

export interface GetTrendbarsParams {
  symbolId: number;
  period: TrendbarPeriod;
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

export interface GetVersionResult {
  service: string;
  version: string;
  springBootVersion: string;
  javaVersion: string;
  buildTime: string;
}

export interface GetBalanceResult {
  /** Valeur entière à l'échelle `moneyDigits` (ex: moneyDigits=2 → centimes) */
  balance: number;
  equity: number;
  freeMargin: number;
  balanceVersion: number;
  moneyDigits: number;
  depositAssetId: number;
}

export interface CtraderAsset {
  assetId: number;
  name: string;
  displayName: string;
}

export interface GetAssetsResult {
  assets: CtraderAsset[];
}

export interface CtraderSymbol {
  symbolId: number;
  symbolName: string;
  enabled: boolean;
  baseAssetId: number;
  quoteAssetId: number;
  symbolCategoryId: number;
  description: string;
}

export interface GetSymbolsResult {
  symbols: CtraderSymbol[];
}

export interface CtraderSpotPrice {
  symbolId: number;
  /** Prix à l'échelle x10^5 (ex: 410177000 → 4101.77) */
  bid: number;
  ask: number;
  high: number;
  low: number;
  sessionClose: number;
  timestamp: number;
}

export interface GetSpotPricesResult {
  prices: CtraderSpotPrice[];
}

export interface CtraderTrendbar {
  timestamp: number;
  /** Prix à l'échelle x10^5 */
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export interface GetTrendbarsResult {
  trendbars: CtraderTrendbar[];
  symbolId: number;
  period: TrendbarPeriod;
}

/** Structure non vérifiée : aucune position ouverte au moment des tests (pas de compte démo disponible). */
export type CtraderPosition = Record<string, unknown>;

/**
 * Type d'ordre tel que renvoyé par get_order_history, en plus de `OrderType` : le
 * serveur y inclut aussi les ordres SL/TP générés automatiquement pour une position.
 */
export type HistoricalOrderType = OrderType | "STOP_LOSS_TAKE_PROFIT";

/**
 * Vérifié via get_order_history (10 ordres réels, XAUUSD). Les champs propres aux
 * ordres *en attente* (label, comment, timeInForce, expirationTimestamp, statut)
 * restent non vérifiés : aucun ordre pending observé lors du test.
 */
export interface CtraderOrder {
  orderId: number;
  symbolId: number;
  orderType: HistoricalOrderType;
  tradeSide: TradeSide;
  /** 1/100 d'unité d'actif de base, comme dans CreateOrderParams */
  volume: number;
  /** Prix affiché, présent sur LIMIT/STOP_LIMIT */
  limitPrice?: number;
  /** Prix affiché, présent sur STOP/STOP_LIMIT */
  stopPrice?: number;
  stopLoss?: number;
  takeProfit?: number;
}

/** Vérifié via get_deals (4 deals réels, XAUUSD). */
export interface CtraderDeal {
  dealId: number;
  orderId: number;
  positionId: number;
  symbolId: number;
  tradeSide: TradeSide;
  volume: number;
  filledVolume: number;
  /** Prix affiché (pas à l'échelle x10^5, contrairement à CtraderSpotPrice/CtraderTrendbar) */
  executionPrice: number;
  /** Epoch ms */
  executionTimestamp: number;
  /** Seule valeur observée : "FILLED". Les autres statuts possibles ne sont pas vérifiés. */
  dealStatus: string;
  /** Valeur signée à l'échelle moneyDigits (négatif = coût) */
  commission: number;
}

export interface GetPositionsResult {
  positions: CtraderPosition[];
  orders: CtraderOrder[];
}

export interface GetPendingOrdersResult {
  orders: CtraderOrder[];
  hasMore: boolean;
}

/** Structure non vérifiée en détail (jamais appelé, aucune position disponible). */
export type GetPositionDetailsResult = Record<string, unknown>;

export interface GetOrderHistoryResult {
  orders: CtraderOrder[];
  hasMore: boolean;
}

export interface GetDealsResult {
  deals: CtraderDeal[];
  hasMore: boolean;
}

/**
 * Résultats non vérifiés : ces outils modifient un compte réel et n'ont
 * volontairement jamais été exercés pendant l'implémentation.
 */
export type AmendPositionResult = Record<string, unknown>;
export type ClosePositionResult = Record<string, unknown>;
export type CreateOrderResult = Record<string, unknown>;
export type AmendOrderResult = Record<string, unknown>;
export type CancelOrderResult = Record<string, unknown>;

// ─── Client ──────────────────────────────────────────────────────────────

export class CtraderMcpError extends Error {
  constructor(
    message: string,
    readonly toolName: string,
  ) {
    super(message);
    this.name = "CtraderMcpError";
  }
}

/**
 * `Client#callTool` returns a union that also covers the legacy `toolResult`-shaped
 * response (pre-MCP-2025 servers). The cTrader server only ever returns the modern
 * `content`-array shape, so this narrows to it and rejects anything else.
 */
function extractText(result: unknown): string | undefined {
  if (typeof result !== "object" || result === null || !("content" in result)) return undefined;

  const content = result.content;
  if (!Array.isArray(content)) return undefined;

  const block = content.find(
    (item): item is { type: "text"; text: string } =>
      typeof item === "object" && item !== null && (item as { type?: unknown }).type === "text",
  );
  return block?.text;
}

export class CtraderClient {
  readonly #client: Client;
  readonly #transport: StreamableHTTPClientTransport;
  #connected = false;

  constructor() {
    this.#transport = new StreamableHTTPClientTransport(new URL(env.CTRADER_MCP_URL), {
      requestInit: {
        headers: { Authorization: `Bearer ${env.CTRADER_MCP_TOKEN}` },
      },
    });
    this.#client = new Client(CLIENT_INFO);
  }

  async connect(): Promise<void> {
    if (this.#connected) return;
    await this.#client.connect(this.#transport);
    this.#connected = true;
  }

  async close(): Promise<void> {
    if (!this.#connected) return;
    await this.#client.close();
    this.#connected = false;
  }

  /** Disponible seulement après connect(). */
  get serverVersion() {
    return this.#client.getServerVersion();
  }

  async #call<T>(name: string, args: object = {}): Promise<T> {
    const result = await this.#client.callTool(
      { name, arguments: args as Record<string, unknown> },
      CallToolResultSchema,
    );
    const text = extractText(result);

    if (result.isError) {
      throw new CtraderMcpError(text ?? `L'outil ${name} a échoué`, name);
    }
    if (text === undefined) {
      throw new CtraderMcpError(`L'outil ${name} n'a renvoyé aucun contenu`, name);
    }

    try {
      return JSON.parse(text) as T;
    } catch {
      throw new CtraderMcpError(`L'outil ${name} a renvoyé un contenu non-JSON : ${text}`, name);
    }
  }

  // --- Compte -----------------------------------------------------------

  getVersion(): Promise<GetVersionResult> {
    return this.#call("get_version");
  }

  getBalance(): Promise<GetBalanceResult> {
    return this.#call("get_balance");
  }

  // --- Référentiel --------------------------------------------------------

  getSymbols(): Promise<GetSymbolsResult> {
    return this.#call("get_symbols");
  }

  getAssets(): Promise<GetAssetsResult> {
    return this.#call("get_assets");
  }

  getSpotPrices(params: GetSpotPricesParams): Promise<GetSpotPricesResult> {
    return this.#call("get_spot_prices", params);
  }

  getTrendbars(params: GetTrendbarsParams): Promise<GetTrendbarsResult> {
    return this.#call("get_trendbars", params);
  }

  // --- Positions & ordres (lecture) ---------------------------------------

  getPositions(): Promise<GetPositionsResult> {
    return this.#call("get_positions");
  }

  getPositionDetails(params: GetPositionDetailsParams): Promise<GetPositionDetailsResult> {
    return this.#call("get_position_details", params);
  }

  getPendingOrders(): Promise<GetPendingOrdersResult> {
    return this.#call("get_pending_orders");
  }

  getOrderHistory(params: GetOrderHistoryParams): Promise<GetOrderHistoryResult> {
    return this.#call("get_order_history", params);
  }

  getDeals(params: GetDealsParams): Promise<GetDealsResult> {
    return this.#call("get_deals", params);
  }

  // --- Trading (écriture — ordres réels) ----------------------------------

  createOrder(params: CreateOrderParams): Promise<CreateOrderResult> {
    return this.#call("create_order", params);
  }

  amendOrder(params: AmendOrderParams): Promise<AmendOrderResult> {
    return this.#call("amend_order", params);
  }

  cancelOrder(params: CancelOrderParams): Promise<CancelOrderResult> {
    return this.#call("cancel_order", params);
  }

  amendPosition(params: AmendPositionParams): Promise<AmendPositionResult> {
    return this.#call("amend_position", params);
  }

  closePosition(params: ClosePositionParams): Promise<ClosePositionResult> {
    return this.#call("close_position", params);
  }
}
