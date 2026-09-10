/**
 * Client MCP typé pour le serveur cTrader (`mcp.ctrader.com`).
 *
 * Usage :
 * ```ts
 * const client = new CtraderClient({ url, token });
 * await client.connect();
 * const { equity } = await Effect.runPromise(client.getBalance());
 * await client.close();
 * ```
 *
 * `connect` / `close` restent en Promise (cycle de vie, pas un appel d'outil) ;
 * toutes les méthodes d'outil retournent un `Effect<T, CtraderMcpError>`
 * — rien ne se passe tant qu'on ne les exécute pas.
 *
 * Lectures (`get_*`) : timeout 10 s + retry exponentiel (200 ms, 400 ms, 2 fois)
 * **uniquement** si {@link CtraderMcpError.retryable}. Écritures : pas de retry,
 * timeout SDK 60 s — rejouer un `create_order` après un timeout risquerait de
 * dupliquer l'ordre si la première tentative avait en fait réussi.
 *
 * Le SDK valide l'enveloppe MCP. Le JSON dans le bloc texte est lu tel quel
 * (`as T`), sauf `get_positions` qui passe par {@link mapGetPositionsResult}
 * (renommage `positionId` → `id`, jamais d'échec).
 */

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import {
  StreamableHTTPClientTransport,
  StreamableHTTPError,
} from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { CallToolResultSchema } from "@modelcontextprotocol/sdk/types.js";
import { Effect, Schedule } from "effect";
import type { GetBalanceResult } from "../account/GetBalanceResult.ts";
import type { GetPendingOrdersResult } from "../book/GetPendingOrdersResult.ts";
import { type GetPositionsResult, mapGetPositionsResult } from "../book/GetPositionsResult.ts";
import type { GetAssetsResult } from "../catalog/GetAssetsResult.ts";
import type { GetSpotPricesParams } from "../catalog/GetSpotPricesParams.ts";
import type { GetSpotPricesResult } from "../catalog/GetSpotPricesResult.ts";
import type { GetSymbolsResult } from "../catalog/GetSymbolsResult.ts";
import type { GetTrendbarsParams } from "../catalog/GetTrendbarsParams.ts";
import type { GetTrendbarsResult } from "../catalog/GetTrendbarsResult.ts";
import type { AmendOrderParams } from "../trading/AmendOrderParams.ts";
import type { AmendOrderResult } from "../trading/AmendOrderResult.ts";
import type { AmendPositionParams } from "../trading/AmendPositionParams.ts";
import type { AmendPositionResult } from "../trading/AmendPositionResult.ts";
import type { CancelOrderParams } from "../trading/CancelOrderParams.ts";
import type { CancelOrderResult } from "../trading/CancelOrderResult.ts";
import type { ClosePositionParams } from "../trading/ClosePositionParams.ts";
import type { ClosePositionResult } from "../trading/ClosePositionResult.ts";
import type { CreateOrderParams } from "../trading/CreateOrderParams.ts";
import type { CreateOrderResult } from "../trading/CreateOrderResult.ts";
import type { CtraderClientConfig } from "./CtraderClientConfig.ts";
import { CtraderMcpError } from "./CtraderMcpError.ts";

const CLIENT_INFO = { name: "aurum", version: "0.2.0" };

const READ_RETRY_SCHEDULE = Schedule.exponential("200 millis").pipe(
  Schedule.compose(Schedule.recurs(2)),
);

const READ_TIMEOUT_MS = 10_000;

/**
 * Un 4xx (token invalide / expiré, requête malformée) ne sera jamais réparé par
 * un retry. `StreamableHTTPError.code` porte le status HTTP quand le serveur a
 * répondu ; toute autre cause (DNS, timeout, abort…) reste retryable.
 */
function isRetryableTransportError(cause: unknown): boolean {
  if (cause instanceof StreamableHTTPError) {
    return cause.code === undefined || cause.code < 400 || cause.code >= 500;
  }
  return true;
}

type ToolCallResult = Awaited<ReturnType<Client["callTool"]>>;
type ModernToolResult = Extract<ToolCallResult, { content: unknown[] }>;
type ToolContentBlock = ModernToolResult["content"][number];

function isModernToolResult(result: ToolCallResult): result is ModernToolResult {
  return Array.isArray((result as ModernToolResult).content);
}

function isTextBlock(
  block: ToolContentBlock,
): block is Extract<ToolContentBlock, { type: "text" }> {
  return block.type === "text";
}

/**
 * Client concret. Un `close()` pendant un `connect()` en vol attend la
 * connexion avant de statuer — sinon le transport resterait ouvert et orphelin.
 *
 * Nouvelle instance de transport à chaque `connect()` : `StreamableHTTPClientTransport#close()`
 * abort son AbortController sans le remettre à `undefined`, donc réutiliser la
 * même instance ferait échouer `start()` avec « already started! ».
 */
export class CtraderClient {
  private readonly client: Client;
  private readonly config: CtraderClientConfig;
  private connected = false;
  private connecting: Promise<void> | undefined;

  constructor(config: CtraderClientConfig) {
    this.config = config;
    this.client = new Client(CLIENT_INFO);
  }

  /** Établit le transport HTTP. Idempotent si déjà connecté. */
  async connect(): Promise<void> {
    if (this.connected) return;
    if (!this.connecting) {
      const transport = new StreamableHTTPClientTransport(new URL(this.config.url), {
        requestInit: {
          headers: { Authorization: `Bearer ${this.config.token}` },
        },
      });
      this.connecting = this.client
        .connect(transport)
        .then(() => {
          this.connected = true;
        })
        .finally(() => {
          this.connecting = undefined;
        });
    }
    await this.connecting;
  }

  /**
   * Ferme le transport. Attend une connexion en vol avant de statuer, pour ne
   * pas laisser un `connect()` résoudre juste après sur un transport orphelin.
   */
  async close(): Promise<void> {
    if (this.connecting) await this.connecting.catch(() => {});
    if (!this.connected) return;
    await this.client.close();
    this.connected = false;
  }

  /** `true` après un `connect()` réussi, `false` après `close()`. */
  get isConnected(): boolean {
    return this.connected;
  }

  /**
   * `false` tant que url / token n'ont pas été réglés (`settings url` /
   * `settings token`). Permet de ne même pas tenter `connect()` — `new URL("")`
   * lèverait une exception peu claire.
   */
  get isConfigured(): boolean {
    return this.config.url.trim() !== "" && this.config.token.trim() !== "";
  }

  /**
   * Version du serveur MCP, une fois connecté.
   * @throws si `connect()` n'a pas encore été appelé.
   */
  get serverVersion(): ReturnType<Client["getServerVersion"]> {
    if (!this.connected) {
      throw new Error("CtraderClient : connect() n'a pas encore été appelé");
    }
    return this.client.getServerVersion();
  }

  private call<T>(
    name: string,
    args: object = {},
    options?: { timeout?: number },
  ): Effect.Effect<T, CtraderMcpError> {
    const client = this.client;

    return Effect.gen(function* () {
      const result = yield* Effect.tryPromise({
        try: (signal) =>
          client.callTool(
            { name, arguments: args as Record<string, unknown> },
            CallToolResultSchema,
            { timeout: options?.timeout, signal },
          ),
        catch: (cause) =>
          new CtraderMcpError(
            cause instanceof Error ? cause.message : String(cause),
            isRetryableTransportError(cause),
          ),
      });

      if (!isModernToolResult(result)) {
        return yield* Effect.fail(
          new CtraderMcpError(`L'outil ${name} a renvoyé une réponse dans un format inattendu`),
        );
      }

      const textBlocks = result.content.filter(isTextBlock);
      if (textBlocks.length > 1) {
        return yield* Effect.fail(
          new CtraderMcpError(`L'outil ${name} a renvoyé plusieurs blocs texte (cas non géré)`),
        );
      }
      const text = textBlocks[0]?.text;

      if (result.isError) {
        return yield* Effect.fail(new CtraderMcpError(text ?? `L'outil ${name} a échoué`));
      }
      if (text === undefined) {
        return yield* Effect.fail(new CtraderMcpError(`L'outil ${name} n'a renvoyé aucun contenu`));
      }

      return yield* Effect.try({
        try: () => JSON.parse(text) as T,
        catch: () => new CtraderMcpError(`L'outil ${name} a renvoyé un contenu non-JSON : ${text}`),
      });
    });
  }

  private callWithRetry<T>(name: string, args: object = {}): Effect.Effect<T, CtraderMcpError> {
    return this.call<T>(name, args, { timeout: READ_TIMEOUT_MS }).pipe(
      Effect.retry({
        schedule: READ_RETRY_SCHEDULE,
        while: (error) => error.retryable,
      }),
    );
  }

  /** Solde, equity, marge libre — montants à l'échelle `moneyDigits`. */
  getBalance(): Effect.Effect<GetBalanceResult, CtraderMcpError> {
    return this.callWithRetry("get_balance");
  }

  /** Catalogue des symboles du compte (tickers, base/quote asset ids). */
  getSymbols(): Effect.Effect<GetSymbolsResult, CtraderMcpError> {
    return this.callWithRetry("get_symbols");
  }

  /** Assets (devises, métaux…) référencés par les symboles. */
  getAssets(): Effect.Effect<GetAssetsResult, CtraderMcpError> {
    return this.callWithRetry("get_assets");
  }

  /** Bid / ask courants, prix en entier × 10⁵. */
  getSpotPrices(params: GetSpotPricesParams): Effect.Effect<GetSpotPricesResult, CtraderMcpError> {
    return this.callWithRetry("get_spot_prices", params);
  }

  /**
   * Bougies OHLC. Préférer `(fromTimestamp, toTimestamp)` : `count` seul a
   * renvoyé une 400 malgré le schéma annoncé.
   */
  getTrendbars(params: GetTrendbarsParams): Effect.Effect<GetTrendbarsResult, CtraderMcpError> {
    return this.callWithRetry("get_trendbars", params);
  }

  /** Positions ouvertes + ordres souvent limités aux SL/TP attachés. */
  getPositions(): Effect.Effect<GetPositionsResult, CtraderMcpError> {
    return this.callWithRetry<unknown>("get_positions").pipe(Effect.map(mapGetPositionsResult));
  }

  /** Tous les pendings du compte, tous symboles — à fusionner avec `get_positions.orders`. */
  getPendingOrders(): Effect.Effect<GetPendingOrdersResult, CtraderMcpError> {
    return this.callWithRetry("get_pending_orders");
  }

  /** Crée un ordre. **Pas de retry.** */
  createOrder(params: CreateOrderParams): Effect.Effect<CreateOrderResult, CtraderMcpError> {
    return this.call("create_order", params);
  }

  /** Amend un ordre existant (payload complet, pas un patch). **Pas de retry.** */
  amendOrder(params: AmendOrderParams): Effect.Effect<AmendOrderResult, CtraderMcpError> {
    return this.call("amend_order", params);
  }

  /** Annule un ordre pending. **Pas de retry.** */
  cancelOrder(params: CancelOrderParams): Effect.Effect<CancelOrderResult, CtraderMcpError> {
    return this.call("cancel_order", params);
  }

  /** Amend SL / TP d'une position ouverte. **Pas de retry.** */
  amendPosition(params: AmendPositionParams): Effect.Effect<AmendPositionResult, CtraderMcpError> {
    return this.call("amend_position", params);
  }

  /** Clôture (tout ou partie) une position. **Pas de retry.** */
  closePosition(params: ClosePositionParams): Effect.Effect<ClosePositionResult, CtraderMcpError> {
    return this.call("close_position", params);
  }
}
