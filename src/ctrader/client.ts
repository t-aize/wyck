/**
 * Client MCP typé pour le serveur cTrader (`mcp.ctrader.com`).
 *
 * Usage :
 * ```ts
 * const client = new CtraderClientLive();
 * await client.connect();
 * const { balance, equity } = await Effect.runPromise(client.getBalance());
 * await client.close();
 * ```
 *
 * `connect`/`close` restent en Promise (cycle de vie, pas un appel d'outil MCP) ;
 * toutes les méthodes qui appellent un outil MCP retournent un `Effect<T, CtraderMcpError>`
 * — rien ne se passe tant qu'on ne les exécute pas via `Effect.runPromise`/`Effect.runSync` etc.
 *
 * Mirroir volontairement complet de la surface `trading`/`account`/`market data` du
 * serveur MCP, pas seulement des méthodes déjà câblées dans l'UI : `getVersion`,
 * `getAssets`, `getPositionDetails`, `getPendingOrders`, `getOrderHistory`, `getDeals`,
 * `amendPosition` et `closePosition` n'ont aujourd'hui aucun appelant dans `src/`. Choix
 * assumé plutôt qu'angle mort — `amendPosition`/`closePosition` en particulier
 * attendent que `CtraderPositionSchema` soit verrouillé (cf. ctrader/schemas.ts) avant
 * d'être exposées dans une commande, pour ne pas cibler une position réelle sur la base
 * d'un `positionId` deviné. Si une méthode reste inutilisée longtemps après avoir été
 * implémentée côté UI, c'est le signal pour la retirer plutôt que la garder « au cas où ».
 */

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { CallToolResultSchema } from "@modelcontextprotocol/sdk/types.js";
import { Context, Data, Effect, Schedule } from "effect";
import type { z } from "zod";
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

// Retry avec backoff exponentiel (200ms, 400ms), 2 tentatives supplémentaires max — appliqué
// uniquement aux méthodes de lecture (cf. #callWithRetry, AUDIT_EFFECT.md §3.2). Jamais sur les
// méthodes d'écriture (createOrder/amendOrder/cancelOrder/amendPosition/closePosition) : rejouer
// un ordre après un simple timeout réseau risquerait de le dupliquer côté serveur si la première
// tentative avait en fait réussi — l'audit lui-même met en garde contre un retry générique ici.
const READ_RETRY_SCHEDULE = Schedule.exponential("200 millis").pipe(
  Schedule.compose(Schedule.recurs(2)),
);

// Sous-types tagués plutôt qu'une seule classe (cf. AUDIT_EFFECT.md §1.4) : chaque échec de
// #call() est distinguable via `_tag`, ce qui permet à un appelant de faire `Effect.catchTag(...)`
// sur une cause précise (ex. token expiré → CtraderCallFailed) au lieu de parser un message.
// Toujours des sous-classes d'Error (Data.TaggedError) : `toMessage()` (error instanceof Error)
// continue de fonctionner sans changement côté appelants.

/** L'appel au transport MCP lui-même a échoué (réseau, DNS, timeout…) — avant même de savoir
 * si l'outil existe ou a réussi côté serveur. */
export class CtraderCallFailed extends Data.TaggedError("CtraderCallFailed")<{
  readonly toolName: string;
  readonly message: string;
}> {}

/** Le serveur a répondu, mais l'outil MCP a explicitement signalé une erreur (`isError: true`). */
export class CtraderToolFailed extends Data.TaggedError("CtraderToolFailed")<{
  readonly toolName: string;
  readonly message: string;
}> {}

/** Réponse "réussie" mais sans bloc de contenu texte exploitable. */
export class CtraderEmptyResponse extends Data.TaggedError("CtraderEmptyResponse")<{
  readonly toolName: string;
  readonly message: string;
}> {}

/** Le contenu texte renvoyé n'est pas du JSON valide. */
export class CtraderInvalidJson extends Data.TaggedError("CtraderInvalidJson")<{
  readonly toolName: string;
  readonly rawText: string;
  readonly message: string;
}> {}

/** Le JSON est valide mais ne correspond pas au schéma zod attendu pour cet outil. */
export class CtraderSchemaMismatch extends Data.TaggedError("CtraderSchemaMismatch")<{
  readonly toolName: string;
  readonly issues: string;
  readonly message: string;
}> {}

export type CtraderMcpError =
  | CtraderCallFailed
  | CtraderToolFailed
  | CtraderEmptyResponse
  | CtraderInvalidJson
  | CtraderSchemaMismatch;

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

/**
 * Implémentation concrète, construite directement (`new CtraderClientLive(config)`) là où un
 * `client` brut suffit (App.tsx, SetupScreen.tsx, les hooks qui l'appellent en paramètre
 * explicite). `CtraderClient` (le `Context.Tag` ci-dessous) sert uniquement aux fonctions qui
 * veulent le recevoir par injection Effect plutôt qu'en paramètre — cf. `domain/trading.ts`.
 */
export class CtraderClientLive {
  readonly #client: Client;
  readonly #transport: StreamableHTTPClientTransport;
  #connected = false;
  // Fuite corrigée (cf. AUDIT_EFFECT.md §2.3/§6.1) : sans ce suivi, un close() qui arrive pendant
  // qu'un connect() est encore en vol (ex. démontage rapide du composant React propriétaire, cf.
  // useCtraderConnection.ts) trouvait #connected encore à false et ne faisait rien — puis connect()
  // finissait par résoudre en arrière-plan sur un transport que plus personne ne fermait jamais.
  #connecting: Promise<void> | undefined;

  constructor(config: { url: string; token: string }) {
    this.#transport = new StreamableHTTPClientTransport(new URL(config.url), {
      requestInit: {
        headers: { Authorization: `Bearer ${config.token}` },
      },
    });
    this.#client = new Client(CLIENT_INFO);
  }

  async connect(): Promise<void> {
    if (this.#connected) return;
    if (!this.#connecting) {
      this.#connecting = this.#client
        .connect(this.#transport)
        .then(() => {
          this.#connected = true;
        })
        .finally(() => {
          this.#connecting = undefined;
        });
    }
    await this.#connecting;
  }

  async close(): Promise<void> {
    // Attend une connexion en vol avant de statuer : sinon un connect() qui résout juste après ce
    // close() laisserait le transport ouvert et orphelin (rien ne le refermerait jamais).
    if (this.#connecting) await this.#connecting.catch(() => {});
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
  #call<T>(
    name: string,
    schema: z.ZodType<T>,
    args: object = {},
  ): Effect.Effect<T, CtraderMcpError> {
    const client = this.#client;

    return Effect.gen(function* () {
      const result = yield* Effect.tryPromise({
        try: () =>
          client.callTool(
            { name, arguments: args as Record<string, unknown> },
            CallToolResultSchema,
          ),
        catch: (cause) =>
          new CtraderCallFailed({
            toolName: name,
            message: cause instanceof Error ? cause.message : String(cause),
          }),
      });
      const text = extractText(result);

      if (result.isError) {
        return yield* Effect.fail(
          new CtraderToolFailed({ toolName: name, message: text ?? `L'outil ${name} a échoué` }),
        );
      }
      if (text === undefined) {
        return yield* Effect.fail(
          new CtraderEmptyResponse({
            toolName: name,
            message: `L'outil ${name} n'a renvoyé aucun contenu`,
          }),
        );
      }

      const json = yield* Effect.try({
        try: () => JSON.parse(text) as unknown,
        catch: () =>
          new CtraderInvalidJson({
            toolName: name,
            rawText: text,
            message: `L'outil ${name} a renvoyé un contenu non-JSON : ${text}`,
          }),
      });

      const parsed = schema.safeParse(json);
      if (!parsed.success) {
        return yield* Effect.fail(
          new CtraderSchemaMismatch({
            toolName: name,
            issues: parsed.error.message,
            message: `L'outil ${name} a renvoyé une réponse inattendue : ${parsed.error.message}`,
          }),
        );
      }
      return parsed.data;
    });
  }

  /** Comme #call, avec retry (cf. READ_RETRY_SCHEDULE) — réservé aux méthodes de lecture ; ne
   * réessaie que sur CtraderCallFailed (échec de transport), jamais sur un échec déjà rendu par le
   * serveur (ToolFailed/EmptyResponse/InvalidJson/SchemaMismatch) que rejouer ne changerait pas. */
  #callWithRetry<T>(
    name: string,
    schema: z.ZodType<T>,
    args: object = {},
  ): Effect.Effect<T, CtraderMcpError> {
    return this.#call(name, schema, args).pipe(
      Effect.retry({
        schedule: READ_RETRY_SCHEDULE,
        while: (error) => error._tag === "CtraderCallFailed",
      }),
    );
  }

  // --- Compte -----------------------------------------------------------

  getVersion(): Effect.Effect<GetVersionResult, CtraderMcpError> {
    return this.#callWithRetry("get_version", GetVersionResultSchema);
  }

  getBalance(): Effect.Effect<GetBalanceResult, CtraderMcpError> {
    return this.#callWithRetry("get_balance", GetBalanceResultSchema);
  }

  // --- Référentiel --------------------------------------------------------

  getSymbols(): Effect.Effect<GetSymbolsResult, CtraderMcpError> {
    return this.#callWithRetry("get_symbols", GetSymbolsResultSchema);
  }

  getAssets(): Effect.Effect<GetAssetsResult, CtraderMcpError> {
    return this.#callWithRetry("get_assets", GetAssetsResultSchema);
  }

  getSpotPrices(params: GetSpotPricesParams): Effect.Effect<GetSpotPricesResult, CtraderMcpError> {
    return this.#callWithRetry("get_spot_prices", GetSpotPricesResultSchema, params);
  }

  getTrendbars(params: GetTrendbarsParams): Effect.Effect<GetTrendbarsResult, CtraderMcpError> {
    return this.#callWithRetry("get_trendbars", GetTrendbarsResultSchema, params);
  }

  // --- Positions & ordres (lecture) ---------------------------------------

  getPositions(): Effect.Effect<GetPositionsResult, CtraderMcpError> {
    return this.#callWithRetry("get_positions", GetPositionsResultSchema);
  }

  getPositionDetails(
    params: GetPositionDetailsParams,
  ): Effect.Effect<GetPositionDetailsResult, CtraderMcpError> {
    return this.#callWithRetry("get_position_details", GetPositionDetailsResultSchema, params);
  }

  getPendingOrders(): Effect.Effect<GetPendingOrdersResult, CtraderMcpError> {
    return this.#callWithRetry("get_pending_orders", GetPendingOrdersResultSchema);
  }

  getOrderHistory(
    params: GetOrderHistoryParams,
  ): Effect.Effect<GetOrderHistoryResult, CtraderMcpError> {
    return this.#callWithRetry("get_order_history", GetOrderHistoryResultSchema, params);
  }

  getDeals(params: GetDealsParams): Effect.Effect<GetDealsResult, CtraderMcpError> {
    return this.#callWithRetry("get_deals", GetDealsResultSchema, params);
  }

  // --- Trading (écriture — ordres réels) ----------------------------------

  createOrder(params: CreateOrderParams): Effect.Effect<CreateOrderResult, CtraderMcpError> {
    return this.#call("create_order", CreateOrderResultSchema, params);
  }

  amendOrder(params: AmendOrderParams): Effect.Effect<AmendOrderResult, CtraderMcpError> {
    return this.#call("amend_order", AmendOrderResultSchema, params);
  }

  cancelOrder(params: CancelOrderParams): Effect.Effect<CancelOrderResult, CtraderMcpError> {
    return this.#call("cancel_order", CancelOrderResultSchema, params);
  }

  amendPosition(params: AmendPositionParams): Effect.Effect<AmendPositionResult, CtraderMcpError> {
    return this.#call("amend_position", AmendPositionResultSchema, params);
  }

  closePosition(params: ClosePositionParams): Effect.Effect<ClosePositionResult, CtraderMcpError> {
    return this.#call("close_position", ClosePositionResultSchema, params);
  }
}

/**
 * Tag Effect pour `CtraderClientLive` : `yield* CtraderClient` dans un `Effect.gen` résout
 * l'instance fournie via une `Layer` (`Layer.succeed(CtraderClient, client)`, cf. App.tsx) au
 * lieu de la recevoir en paramètre explicite. Utilisé par `domain/trading.ts#prepareTrade` ;
 * les autres consommateurs (hooks) continuent de recevoir l'instance directement.
 */
export class CtraderClient extends Context.Tag("CtraderClient")<
  CtraderClient,
  CtraderClientLive
>() {}
