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
 */

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { CallToolResultSchema } from "@modelcontextprotocol/sdk/types.js";
import type { z } from "zod";
import { env } from "../env.ts";
import {
  type AmendOrderParams,
  type AmendOrderResult,
  AmendOrderResultSchema,
  type AmendPositionParams,
  type AmendPositionResult,
  AmendPositionResultSchema,
  type CancelOrderParams,
  type CancelOrderResult,
  CancelOrderResultSchema,
  type ClosePositionParams,
  type ClosePositionResult,
  ClosePositionResultSchema,
  type CreateOrderParams,
  type CreateOrderResult,
  CreateOrderResultSchema,
  type GetAssetsResult,
  GetAssetsResultSchema,
  type GetBalanceResult,
  GetBalanceResultSchema,
  type GetDealsParams,
  type GetDealsResult,
  GetDealsResultSchema,
  type GetOrderHistoryParams,
  type GetOrderHistoryResult,
  GetOrderHistoryResultSchema,
  type GetPendingOrdersResult,
  GetPendingOrdersResultSchema,
  type GetPositionDetailsParams,
  type GetPositionDetailsResult,
  GetPositionDetailsResultSchema,
  type GetPositionsResult,
  GetPositionsResultSchema,
  type GetSpotPricesParams,
  type GetSpotPricesResult,
  GetSpotPricesResultSchema,
  type GetSymbolsResult,
  GetSymbolsResultSchema,
  type GetTrendbarsParams,
  type GetTrendbarsResult,
  GetTrendbarsResultSchema,
  type GetVersionResult,
  GetVersionResultSchema,
} from "./schemas.ts";

export type {
  AmendOrderParams,
  AmendOrderResult,
  AmendPositionParams,
  AmendPositionResult,
  CancelOrderParams,
  CancelOrderResult,
  ClosePositionParams,
  ClosePositionResult,
  CreateOrderParams,
  CreateOrderResult,
  CtraderAsset,
  CtraderDeal,
  CtraderOrder,
  CtraderPosition,
  CtraderSpotPrice,
  CtraderSymbol,
  CtraderTrendbar,
  GetAssetsResult,
  GetBalanceResult,
  GetDealsParams,
  GetDealsResult,
  GetOrderHistoryParams,
  GetOrderHistoryResult,
  GetPendingOrdersResult,
  GetPositionDetailsParams,
  GetPositionDetailsResult,
  GetPositionsResult,
  GetSpotPricesParams,
  GetSpotPricesResult,
  GetSymbolsResult,
  GetTrendbarsParams,
  GetTrendbarsResult,
  GetVersionResult,
  HistoricalOrderType,
  OrderType,
  TimeInForce,
  TradeSide,
} from "./schemas.ts";

const CLIENT_INFO = { name: "aurum", version: "0.1.0" };

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
  get serverVersion(): ReturnType<Client["getServerVersion"]> {
    return this.#client.getServerVersion();
  }

  // `args` accepts any params interface (none declare an index signature, so none are
  // structurally assignable to `Record<string, unknown>`) ; cast once here at the MCP
  // SDK boundary instead of at every call site.
  async #call<T>(name: string, schema: z.ZodType<T>, args: object = {}): Promise<T> {
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

    let json: unknown;
    try {
      json = JSON.parse(text);
    } catch {
      throw new CtraderMcpError(`L'outil ${name} a renvoyé un contenu non-JSON : ${text}`, name);
    }

    const parsed = schema.safeParse(json);
    if (!parsed.success) {
      throw new CtraderMcpError(
        `L'outil ${name} a renvoyé une réponse inattendue : ${parsed.error.message}`,
        name,
      );
    }
    return parsed.data;
  }

  // --- Compte -----------------------------------------------------------

  getVersion(): Promise<GetVersionResult> {
    return this.#call("get_version", GetVersionResultSchema);
  }

  getBalance(): Promise<GetBalanceResult> {
    return this.#call("get_balance", GetBalanceResultSchema);
  }

  // --- Référentiel --------------------------------------------------------

  getSymbols(): Promise<GetSymbolsResult> {
    return this.#call("get_symbols", GetSymbolsResultSchema);
  }

  getAssets(): Promise<GetAssetsResult> {
    return this.#call("get_assets", GetAssetsResultSchema);
  }

  getSpotPrices(params: GetSpotPricesParams): Promise<GetSpotPricesResult> {
    return this.#call("get_spot_prices", GetSpotPricesResultSchema, params);
  }

  getTrendbars(params: GetTrendbarsParams): Promise<GetTrendbarsResult> {
    return this.#call("get_trendbars", GetTrendbarsResultSchema, params);
  }

  // --- Positions & ordres (lecture) ---------------------------------------

  getPositions(): Promise<GetPositionsResult> {
    return this.#call("get_positions", GetPositionsResultSchema);
  }

  getPositionDetails(params: GetPositionDetailsParams): Promise<GetPositionDetailsResult> {
    return this.#call("get_position_details", GetPositionDetailsResultSchema, params);
  }

  getPendingOrders(): Promise<GetPendingOrdersResult> {
    return this.#call("get_pending_orders", GetPendingOrdersResultSchema);
  }

  getOrderHistory(params: GetOrderHistoryParams): Promise<GetOrderHistoryResult> {
    return this.#call("get_order_history", GetOrderHistoryResultSchema, params);
  }

  getDeals(params: GetDealsParams): Promise<GetDealsResult> {
    return this.#call("get_deals", GetDealsResultSchema, params);
  }

  // --- Trading (écriture — ordres réels) ----------------------------------

  createOrder(params: CreateOrderParams): Promise<CreateOrderResult> {
    return this.#call("create_order", CreateOrderResultSchema, params);
  }

  amendOrder(params: AmendOrderParams): Promise<AmendOrderResult> {
    return this.#call("amend_order", AmendOrderResultSchema, params);
  }

  cancelOrder(params: CancelOrderParams): Promise<CancelOrderResult> {
    return this.#call("cancel_order", CancelOrderResultSchema, params);
  }

  amendPosition(params: AmendPositionParams): Promise<AmendPositionResult> {
    return this.#call("amend_position", AmendPositionResultSchema, params);
  }

  closePosition(params: ClosePositionParams): Promise<ClosePositionResult> {
    return this.#call("close_position", ClosePositionResultSchema, params);
  }
}
