/**
 * Client MCP typé pour le serveur cTrader (`mcp.ctrader.com`).
 *
 * Usage :
 * ```ts
 * const client = new CtraderClient();
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
 * `getAssets`, `getTrendbars`, `getPositionDetails`, `getPendingOrders`, `getOrderHistory`,
 * `getDeals`, `amendPosition` et `closePosition` n'ont aujourd'hui aucun appelant dans `src/`
 * (`getTrendbars` en particulier depuis le retrait du mode ATR et de l'analyse SMC, seuls
 * consommateurs de bougies historiques). Choix assumé plutôt qu'angle mort —
 * `amendPosition`/`closePosition` en particulier attendent que `CtraderPositionSchema` soit
 * verrouillé (cf. ctrader/schemas.ts) avant d'être exposées dans une commande, pour ne pas cibler
 * une position réelle sur la base d'un `positionId` deviné. Si une méthode reste inutilisée
 * longtemps après avoir été implémentée côté UI, c'est le signal pour la retirer plutôt que la
 * garder « au cas où ».
 */

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import {
  StreamableHTTPClientTransport,
  StreamableHTTPError,
} from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { CallToolResultSchema } from "@modelcontextprotocol/sdk/types.js";
import { Effect, Schedule } from "effect";
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

const CLIENT_INFO = { name: "aurum", version: "0.1.0" };

// Retry avec backoff exponentiel (200ms, 400ms), 2 tentatives supplémentaires max — appliqué
// uniquement aux méthodes de lecture (cf. callWithRetry). Jamais sur
// les méthodes d'écriture (createOrder/amendOrder/cancelOrder/amendPosition/closePosition) :
// rejouer un ordre après un simple timeout réseau risquerait de le dupliquer côté serveur si la
// première tentative avait en fait réussi.
const READ_RETRY_SCHEDULE = Schedule.exponential("200 millis").pipe(
  Schedule.compose(Schedule.recurs(2)),
);

// Le SDK applique déjà un timeout de 60s par requête par défaut (DEFAULT_REQUEST_TIMEOUT_MSEC,
// cf. shared/protocol.ts) — resserré ici uniquement pour les lectures, sûr de le faire puisqu'elles
// sont retryable (cf. READ_RETRY_SCHEDULE) : 3 tentatives à 10s valent mieux qu'une seule à 60s
// avant qu'un panel affiche enfin quelque chose. Les écritures gardent le défaut SDK — les couper
// plus tôt risquerait d'afficher un échec en UI alors que l'ordre a en fait fini par passer côté
// serveur, même raison que l'absence de retry sur elles.
const READ_TIMEOUT_MS = 10_000;

/**
 * Échec d'un appel à un outil MCP cTrader — transport (réseau/DNS/timeout), erreur explicite de
 * l'outil, réponse vide, contenu non-JSON, ou schéma zod inattendu. Une seule classe : l'ancienne
 * hiérarchie de 5 sous-types tagués (`Data.TaggedError`, un par cause) n'était discriminée par aucun
 * appelant (tous affichent juste `.message` via `utils/errors.ts#toMessage`) — seule la distinction
 * "échec de transport, donc retryable" servait réellement (cf. callWithRetry), portée ici par
 * `retryable` plutôt que par une classe séparée par cause.
 */
export class CtraderMcpError extends Error {
  constructor(
    message: string,
    readonly retryable = false,
  ) {
    super(message);
    this.name = "CtraderMcpError";
  }
}

/** Un échec HTTP 4xx (token invalide/expiré, requête malformée) ne sera jamais réparé par un
 * retry — seul un échec de transport (réseau, timeout, 5xx) peut l'être. `StreamableHTTPError.code`
 * (cf. SDK `client/streamableHttp.ts`) porte le status HTTP quand le serveur a explicitement
 * répondu ; toute autre cause (DNS, timeout réseau, abort...) reste retryable par défaut. */
function isRetryableTransportError(cause: unknown): boolean {
  if (cause instanceof StreamableHTTPError) {
    return cause.code === undefined || cause.code < 400 || cause.code >= 500;
  }
  return true;
}

/**
 * `Client#callTool`, quand on lui passe `CallToolResultSchema` (toujours le cas ici), a un type de
 * retour précis — une union entre le format moderne (`content[]`, `isError?`) et le format legacy
 * `toolResult` (pré-MCP-2025) — donc pas besoin de re-sniffer sa forme à la main sur un `unknown` :
 * on peut cibler directement les membres de cette union.
 */
type ToolCallResult = Awaited<ReturnType<Client["callTool"]>>;
type ModernToolResult = Extract<ToolCallResult, { content: unknown[] }>;
type ToolContentBlock = ModernToolResult["content"][number];

// Les deux membres de `ToolCallResult` portent un index signature `[x: string]: unknown` (cf. SDK),
// ce qui empêche `"content" in result` de vraiment exclure le membre legacy à la vérification de
// type — un garde explicite (plutôt qu'un `in` narrowing qui échoue silencieusement ici) reste le
// seul moyen fiable de récupérer le membre moderne, correctement typé au-delà de ce point.
function isModernToolResult(result: ToolCallResult): result is ModernToolResult {
  return Array.isArray((result as ModernToolResult).content);
}

function isTextBlock(
  block: ToolContentBlock,
): block is Extract<ToolContentBlock, { type: "text" }> {
  return block.type === "text";
}

/** Params sortants, jamais reçus tels quels du réseau (lus depuis `~/.aurum/config.json` ou saisis
 * dans SetupScreen.tsx, cf. config.ts#readConfig) — `interface` simple, pas de schéma zod, même
 * convention que les Params de ctrader/schemas.ts. Aucune validation de forme volontairement (cf.
 * config.ts) : une URL/un token invalide échoue au premier appel réseau plutôt qu'à la lecture. */
export interface CtraderClientConfig {
  url: string;
  token: string;
}

/**
 * Implémentation concrète, construite directement (`new CtraderClient(config)`) et reçue en
 * paramètre explicite partout (App.tsx, SetupScreen.tsx, les hooks, `domain/trading.ts`) — pas de
 * DI Effect (`Context.Tag`/`Layer`) : ça n'aurait servi qu'à `prepareTrade`, seul consommateur à
 * jamais en avoir eu besoin, pendant que tout le reste de l'app passe déjà le client en paramètre
 * simple.
 */
export class CtraderClient {
  private readonly client: Client;
  private readonly config: CtraderClientConfig;
  private connected = false;
  // Fuite corrigée : sans ce suivi, un close() qui arrive pendant
  // qu'un connect() est encore en vol (ex. démontage rapide du composant React propriétaire, cf.
  // useCtraderConnection.ts) trouvait `connected` encore à false et ne faisait rien — puis connect()
  // finissait par résoudre en arrière-plan sur un transport que plus personne ne fermait jamais.
  private connecting: Promise<void> | undefined;

  constructor(config: CtraderClientConfig) {
    this.config = config;
    this.client = new Client(CLIENT_INFO);
  }

  async connect(): Promise<void> {
    if (this.connected) return;
    if (!this.connecting) {
      // Nouvelle instance de transport à chaque tentative, jamais réutilisée après un close() —
      // vérifié dans le SDK : `StreamableHTTPClientTransport#close()` abort son AbortController
      // interne mais ne le remet jamais à `undefined`, donc réutiliser la même instance ferait
      // échouer start() avec "already started!" sur un connect() qui suit un close(). Le `Client`
      // MCP lui, réinitialise bien son état interne au close (`onclose`), donc lui reste réutilisé.
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

  async close(): Promise<void> {
    // Attend une connexion en vol avant de statuer : sinon un connect() qui résout juste après ce
    // close() laisserait le transport ouvert et orphelin (rien ne le refermerait jamais).
    if (this.connecting) await this.connecting.catch(() => {});
    if (!this.connected) return;
    await this.client.close();
    this.connected = false;
  }

  /** Pour un consommateur externe qui voudrait éviter de dupliquer cet état ou d'appeler
   * connect()/serverVersion à l'aveugle (cf. useCtraderConnection.ts, qui garde aujourd'hui son
   * propre état React distinct plutôt que de lire celui-ci). */
  get isConnected(): boolean {
    return this.connected;
  }

  /** Lève explicitement plutôt que de laisser le SDK renvoyer une erreur peu claire si appelé
   * avant connect(). */
  get serverVersion(): ReturnType<Client["getServerVersion"]> {
    if (!this.connected) {
      throw new Error("CtraderClient : connect() n'a pas encore été appelé");
    }
    return this.client.getServerVersion();
  }

  // `args` accepts any params interface (none declare an index signature, so none are
  // structurally assignable to `Record<string, unknown>`) ; cast once here at the MCP
  // SDK boundary instead of at every call site.
  private call<T>(
    name: string,
    schema: z.ZodType<T>,
    args: object = {},
    options?: { timeout?: number },
  ): Effect.Effect<T, CtraderMcpError> {
    const client = this.client;

    return Effect.gen(function* () {
      const result = yield* Effect.tryPromise({
        // `signal` vient d'Effect (annulé si la Fiber qui exécute cet Effect est interrompue,
        // ex. timeout Effect en amont) — relayé au SDK pour que l'annulation coupe vraiment la
        // requête HTTP sous-jacente plutôt que de la laisser tourner en arrière-plan.
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

      // Le serveur cTrader ne renvoie jamais le format legacy `toolResult` (pré-MCP-2025) — écarté
      // explicitement pour que `result` soit ensuite typé sur le format moderne (`content`/
      // `isError`), pas sur l'index signature `[x: string]: unknown` que le SDK met sur les deux
      // membres de l'union.
      if (!isModernToolResult(result)) {
        return yield* Effect.fail(
          new CtraderMcpError(`L'outil ${name} a renvoyé une réponse dans un format inattendu`),
        );
      }

      const textBlocks = result.content.filter(isTextBlock);
      if (textBlocks.length > 1) {
        // Jamais observé sur aucun payload vérifié (cf. schemas.ts) — mieux vaut un échec explicite
        // que de silencieusement ignorer tout sauf le premier bloc si ça arrive un jour.
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

      const json = yield* Effect.try({
        try: () => JSON.parse(text) as unknown,
        catch: () => new CtraderMcpError(`L'outil ${name} a renvoyé un contenu non-JSON : ${text}`),
      });

      const parsed = schema.safeParse(json);
      if (!parsed.success) {
        return yield* Effect.fail(
          new CtraderMcpError(
            `L'outil ${name} a renvoyé une réponse inattendue : ${parsed.error.message}`,
          ),
        );
      }
      return parsed.data;
    });
  }

  /** Comme call(), avec retry (cf. READ_RETRY_SCHEDULE) et timeout resserré (cf. READ_TIMEOUT_MS)
   * — réservé aux méthodes de lecture ; ne réessaie que sur un échec de transport (`retryable`),
   * jamais sur un échec déjà rendu par le serveur que rejouer ne changerait pas. */
  private callWithRetry<T>(
    name: string,
    schema: z.ZodType<T>,
    args: object = {},
  ): Effect.Effect<T, CtraderMcpError> {
    return this.call(name, schema, args, { timeout: READ_TIMEOUT_MS }).pipe(
      Effect.retry({
        schedule: READ_RETRY_SCHEDULE,
        while: (error) => error.retryable,
      }),
    );
  }

  // --- Compte -----------------------------------------------------------

  getVersion(): Effect.Effect<GetVersionResult, CtraderMcpError> {
    return this.callWithRetry("get_version", GetVersionResultSchema);
  }

  getBalance(): Effect.Effect<GetBalanceResult, CtraderMcpError> {
    return this.callWithRetry("get_balance", GetBalanceResultSchema);
  }

  // --- Référentiel --------------------------------------------------------

  getSymbols(): Effect.Effect<GetSymbolsResult, CtraderMcpError> {
    return this.callWithRetry("get_symbols", GetSymbolsResultSchema);
  }

  getAssets(): Effect.Effect<GetAssetsResult, CtraderMcpError> {
    return this.callWithRetry("get_assets", GetAssetsResultSchema);
  }

  getSpotPrices(params: GetSpotPricesParams): Effect.Effect<GetSpotPricesResult, CtraderMcpError> {
    return this.callWithRetry("get_spot_prices", GetSpotPricesResultSchema, params);
  }

  getTrendbars(params: GetTrendbarsParams): Effect.Effect<GetTrendbarsResult, CtraderMcpError> {
    return this.callWithRetry("get_trendbars", GetTrendbarsResultSchema, params);
  }

  // --- Positions & ordres (lecture) ---------------------------------------

  getPositions(): Effect.Effect<GetPositionsResult, CtraderMcpError> {
    return this.callWithRetry("get_positions", GetPositionsResultSchema);
  }

  getPositionDetails(
    params: GetPositionDetailsParams,
  ): Effect.Effect<GetPositionDetailsResult, CtraderMcpError> {
    return this.callWithRetry("get_position_details", GetPositionDetailsResultSchema, params);
  }

  getPendingOrders(): Effect.Effect<GetPendingOrdersResult, CtraderMcpError> {
    return this.callWithRetry("get_pending_orders", GetPendingOrdersResultSchema);
  }

  getOrderHistory(
    params: GetOrderHistoryParams,
  ): Effect.Effect<GetOrderHistoryResult, CtraderMcpError> {
    return this.callWithRetry("get_order_history", GetOrderHistoryResultSchema, params);
  }

  getDeals(params: GetDealsParams): Effect.Effect<GetDealsResult, CtraderMcpError> {
    return this.callWithRetry("get_deals", GetDealsResultSchema, params);
  }

  // --- Trading (écriture — ordres réels) ----------------------------------

  createOrder(params: CreateOrderParams): Effect.Effect<CreateOrderResult, CtraderMcpError> {
    return this.call("create_order", CreateOrderResultSchema, params);
  }

  amendOrder(params: AmendOrderParams): Effect.Effect<AmendOrderResult, CtraderMcpError> {
    return this.call("amend_order", AmendOrderResultSchema, params);
  }

  cancelOrder(params: CancelOrderParams): Effect.Effect<CancelOrderResult, CtraderMcpError> {
    return this.call("cancel_order", CancelOrderResultSchema, params);
  }

  amendPosition(params: AmendPositionParams): Effect.Effect<AmendPositionResult, CtraderMcpError> {
    return this.call("amend_position", AmendPositionResultSchema, params);
  }

  closePosition(params: ClosePositionParams): Effect.Effect<ClosePositionResult, CtraderMcpError> {
    return this.call("close_position", ClosePositionResultSchema, params);
  }
}
