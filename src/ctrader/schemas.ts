/**
 * Types et schémas des échanges avec le serveur MCP cTrader.
 *
 * Deux mondes délibérément distincts, pas un mélange accidentel :
 * - **Params** (sortant) : `interface` TS simples, jamais validées à l'exécution — construites par
 *   ce client lui-même à partir de valeurs déjà typées par `tsc`, jamais reçues du réseau. Rien à
 *   valider à cette frontière.
 * - **Results** (entrant) : schémas zod, validés via `safeParse` dans `client.ts`. La forme vient du
 *   serveur, hors de notre contrôle à la compilation — zod est le filet de sécurité qui transforme un
 *   payload inattendu en échec typé (`CtraderSchemaMismatch`) plutôt qu'un plantage plus loin dans le
 *   mapping/l'UI.
 * Exception : les enums (OrderType/TradeSide/TimeInForce) sont zod même si des Params les utilisent —
 * ils doivent être validés côté Results (ex. `CtraderOrderSchema.tradeSide`), les Params se contentent
 * d'emprunter le type déjà dérivé (`z.infer`) sans jamais appeler le schéma.
 *
 * `CtraderOrder`/`CtraderDeal` ont été vérifiés contre de vrais payloads
 * (get_order_history / get_deals, compte réel, symbole XAUUSD). `CtraderPosition`,
 * `GetPositionDetailsResult` et les résultats d'écriture ont depuis été vérifiés à leur
 * tour contre de vrais payloads — compte DÉMO, symbole XAUUSD, tous les 16 tools que le
 * serveur expose (create_order/amend_order/cancel_order/amend_position/close_position
 * exercés sur un ordre puis une position de test, chacun annulé/clôturé aussitôt après ;
 * exploration ponctuelle, pas d'outil dédié conservé dans le dépôt).
 *
 * `CtraderPositionSchema` valide toujours l'entrée en `z.record(z.string(), z.unknown())`
 * plutôt qu'un `z.object` strict, verified ou pas : un schéma strict qui se trompe (ex.
 * un champ optionnel présent seulement sur une classe d'actif jamais testée) ferait
 * *planter* l'affichage des positions (échec `safeParse` → `CtraderMcpError`), ce qui
 * est pire pour un panel de trading que des valeurs possiblement fausses mais visibles.
 * Un `.transform()` post-parse (jamais un `.refine()`, qui pourrait faire échouer le
 * parse) projette ensuite ce record sur la forme réellement utile, champ par champ,
 * chacun `undefined` plutôt qu'une erreur s'il est absent/du mauvais type — cf. le type
 * `CtraderPosition` ci-dessous pour la forme réelle désormais confirmée. `isUnmapped`
 * (cf. `PositionsPanel.tsx`) détecte après coup si les champs à haute confiance n'ont
 * rien résolu, pour avertir plutôt qu'afficher silencieusement des positions fausses.
 * Les résultats d'écriture restent en `z.record` brut pour la même raison, en pire : un
 * `safeParse` qui échoue sur `create_order` transformerait un ordre *réussi* en échec
 * apparent côté UI (`useOrderActions.ts` ne lit déjà aujourd'hui aucun de leurs champs,
 * seulement succès/échec de la Promise — verrouiller ces schémas n'apporterait donc
 * aucun bénéfice, seulement ce risque). Leur forme réelle confirmée est documentée en
 * commentaire à côté de chaque schéma ci-dessous, pour un futur consommateur qui
 * voudrait lire `orderId`/`positionId` sans se re-taper toute cette exploration.
 */

import { z } from "zod";
import { TRENDBAR_PERIODS, type TrendbarPeriod } from "../constants.ts";
import { toLots } from "../utils/priceMath.ts";

// ─── Enums (zod : validés côté Results, cf. commentaire en tête de fichier) ─

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

/** Volontairement permissif malgré une forme réelle désormais connue — cf. commentaire en tête
 * de fichier pour pourquoi (positions : ne jamais planter l'affichage ; écriture : ne jamais
 * transformer un ordre réussi en échec apparent). */
const PermissiveRecordSchema = z.record(z.string(), z.unknown());

// ─── Compte ─────────────────────────────────────────────────────────────

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

// ─── Référentiel ────────────────────────────────────────────────────────

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

export const CtraderAssetSchema = z.object({
  assetId: z.number(),
  name: z.string(),
  displayName: z.string(),
});
export type CtraderAsset = z.infer<typeof CtraderAssetSchema>;

export const GetAssetsResultSchema = z.object({ assets: z.array(CtraderAssetSchema) });
export type GetAssetsResult = z.infer<typeof GetAssetsResultSchema>;

export interface GetSpotPricesParams {
  /** IDs des symboles */
  symbolId: number[];
}

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

// ─── Positions & ordres (lecture) ───────────────────────────────────────

/** Lit `keys[0]`, sinon `keys[1]`... dans un record non typé — `undefined` si aucune clé
 * n'est présente ou si la valeur trouvée n'a pas le bon type, plutôt qu'une exception. */
function readNumber(record: Record<string, unknown>, keys: string[]): number | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "number") return value;
  }
  return undefined;
}

function readTradeSide(record: Record<string, unknown>): TradeSide | undefined {
  const value = record.tradeSide;
  return value === "BUY" || value === "SELL" ? value : undefined;
}

/**
 * Forme réelle confirmée (compte démo, cf. commentaire en tête de fichier) :
 * `{ positionId, symbolId, tradeSide, volume, entryPrice, stopLoss?, takeProfit?,
 * commission, swap }` — `volume`/`entryPrice` valent `0` sur le stub de position associé
 * à un ordre pending pas encore rempli (ex: champ `position` de la réponse `create_order`
 * pour un LIMIT). Pas de champ de P&L latent ni d'`openTimestamp` observé sur cette forme.
 * Projetée (`.transform()`, jamais en échec) sur les seuls champs utiles à `PositionsPanel.tsx` —
 * `symbolId`/`commission` ignorés faute de consommateur aujourd'hui, à réintroduire si un jour
 * nécessaire plutôt que de les porter sans usage.
 */
export const CtraderPositionSchema = PermissiveRecordSchema.transform((position) => {
  const volume = readNumber(position, ["volume"]);
  return {
    id: readNumber(position, ["positionId"]),
    side: readTradeSide(position),
    // lotSize métaux = 100 → volume(1/100 unités) / 10000 = lots. Approximation valable pour XAUUSD,
    // seul symbole tradé par ce panel — à revoir si d'autres classes d'actifs sont ajoutées un jour.
    volumeLots: volume === undefined ? undefined : toLots(volume),
    entry: readNumber(position, ["entryPrice"]),
    stopLoss: readNumber(position, ["stopLoss"]),
    takeProfit: readNumber(position, ["takeProfit"]),
    swap: readNumber(position, ["swap"]),
  };
});
export type CtraderPosition = z.infer<typeof CtraderPositionSchema>;

/** `CtraderPosition` dont `id` a été confirmé résolu (potentiellement absent sur un mapping
 * incomplet, cf. commentaire sur `CtraderPositionSchema` ci-dessus) — le sous-ensemble
 * réellement actionnable pour amend/close, plutôt que de retaper `CtraderPosition & { id: number }`
 * à chaque site d'appel. */
export type AmendablePosition = CtraderPosition & { id: number };

/** `AmendablePosition` dont `volumeLots` a lui aussi été confirmé résolu — nécessaire pour calculer
 * le volume de clôture (cf. trading/amendParams.ts#toClosePositionParams). */
export type ClosablePosition = AmendablePosition & { volumeLots: number };

/**
 * Vérifié via get_order_history (10 ordres réels, XAUUSD). Les champs propres aux
 * ordres *en attente* (label, comment, timeInForce, statut) restent non vérifiés :
 * aucun ordre pending observé lors du test. expirationTimestamp est repris malgré
 * tout (même nom que dans CreateOrderParams/AmendOrderParams) — sans lui, un amend manuel (cf.
 * useModifyConfirm.ts) ne peut pas le renvoyer et cTrader l'efface silencieusement.
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

export interface GetPositionDetailsParams {
  positionId: number;
}

/** Vérifié (compte démo) : `{ position, orders, deals }`, où `orders`/`deals` incluent
 * respectivement l'ordre d'ouverture et le deal d'exécution correspondant. Verrouillé (pas un
 * `z.record`) : contrairement aux schémas d'écriture, cet outil n'a aujourd'hui aucun appelant
 * dans `src/` (cf. client.ts) — un futur mismatch romprait la compilation/les tests avant de
 * jamais atteindre un utilisateur, pas de risque de "faux échec" en prod. `position` reste
 * `CtraderPositionSchema` (permissif) par cohérence avec le reste du fichier. */
export const GetPositionDetailsResultSchema = z.object({
  position: CtraderPositionSchema,
  orders: z.array(CtraderOrderSchema),
  deals: z.array(CtraderDealSchema),
});
export type GetPositionDetailsResult = z.infer<typeof GetPositionDetailsResultSchema>;

export const GetPendingOrdersResultSchema = z.object({
  orders: z.array(CtraderOrderSchema),
  hasMore: z.boolean(),
});
export type GetPendingOrdersResult = z.infer<typeof GetPendingOrdersResultSchema>;

/** Plage temporelle requise par get_order_history/get_deals — plafonnée à 720h (30 jours)
 * côté serveur. Partagée par les deux Params ci-dessous : même paire de champs, même contrainte. */
interface TimestampRangeParams {
  /** Epoch ms ou ISO-8601. */
  fromTimestamp: string;
  toTimestamp: string;
}

export type GetOrderHistoryParams = TimestampRangeParams;

export const GetOrderHistoryResultSchema = z.object({
  orders: z.array(CtraderOrderSchema),
  hasMore: z.boolean(),
});
export type GetOrderHistoryResult = z.infer<typeof GetOrderHistoryResultSchema>;

export interface GetDealsParams extends TimestampRangeParams {
  /** Défaut serveur : 50 */
  maxRows?: number;
}

export const GetDealsResultSchema = z.object({
  deals: z.array(CtraderDealSchema),
  hasMore: z.boolean(),
});
export type GetDealsResult = z.infer<typeof GetDealsResultSchema>;

// ─── Trading (écriture — ordres réels) ──────────────────────────────────

/**
 * Forme réelle confirmée (compte démo, cf. commentaire en tête de fichier), identique pour les
 * 5 outils d'écriture : `{ orderId, positionId, executionType, order, position, deal? }`.
 *
 * - `executionType` : valeurs observées `ORDER_ACCEPTED` / `ORDER_REPLACED` / `ORDER_CANCELLED` /
 *   `ORDER_FILLED` — liste probablement non exhaustive (rejets, exécutions partielles jamais
 *   déclenchés pendant le test), d'où `z.string()` plutôt qu'un enum si ce champ était un jour lu.
 * - `order`/`position` : mêmes formes que `CtraderOrderSchema`/`CtraderPositionSchema`, mais ne
 *   décrivent pas toujours l'action "principale" de l'appel — ex. `close_position` sur une
 *   position dont l'ordre SL/TP auto-attaché (relativeStopLoss/relativeTakeProfit à la création)
 *   n'a jamais été déclenché renvoie `executionType: "ORDER_CANCELLED"` pour *cet* ordre SL/TP
 *   (annulé en effet de bord), pas l'ordre MARKET qui a réellement clôturé la position.
 * - `deal` (absent la plupart du temps) : présent seulement quand l'appel déclenche lui-même une
 *   exécution immédiate (ex. `close_position` sur une position sans SL/TP attaché, où le serveur
 *   crée et remplit directement un ordre MARKET de clôture) — forme minimale et *distincte* de
 *   `CtraderDealSchema` : `{ dealId, volume, closePrice }` (pas de symbolId/tradeSide/dealStatus,
 *   et `closePrice` au lieu d'`executionPrice`).
 *
 * Reste un `z.record` malgré cette forme connue — cf. commentaire en tête de fichier : rien dans
 * `src/` ne lit ces champs aujourd'hui (`useOrderActions.ts` ne regarde que succès/échec de la
 * Promise), verrouiller n'apporterait donc aucun bénéfice pour le risque pris (un `safeParse` qui
 * échoue sur un cas non testé transformerait un ordre *réussi* en échec apparent côté UI).
 */
const WriteResultSchema = PermissiveRecordSchema;

/**
 * Champs de prix/protection partagés par create_order et amend_order (même nom, même sens dans
 * les deux) — seule leur *contrainte* diffère selon le contexte, documentée par champ ci-dessous.
 */
interface OrderPriceFields {
  /** Requis pour LIMIT, STOP_LIMIT (création). */
  limitPrice?: number;
  /** Requis pour STOP, STOP_LIMIT (création). */
  stopPrice?: number;
  /**
   * Prix absolu ; supporté sur LIMIT/STOP/STOP_LIMIT, PAS sur MARKET/MARKET_RANGE en création.
   * Exclusif avec relativeStopLoss.
   */
  stopLoss?: number;
  /** Comme stopLoss, pour le take-profit. Exclusif avec relativeTakeProfit. */
  takeProfit?: number;
  /**
   * Distance en points depuis le prix d'exécution ; requis pour MARKET/MARKET_RANGE en création.
   * Exclusif avec stopLoss.
   */
  relativeStopLoss?: number;
  /** Comme relativeStopLoss, pour le take-profit. Exclusif avec takeProfit. */
  relativeTakeProfit?: number;
  /** Epoch ms (entier uniquement, pas d'ISO-8601 ici). */
  expirationTimestamp?: number;
}

export interface CreateOrderParams extends OrderPriceFields {
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
  comment?: string;
  label?: string;
  timeInForce?: TimeInForce;
  baseSlippagePrice?: number;
  slippageInPoints?: number;
}

export const CreateOrderResultSchema = WriteResultSchema;
export type CreateOrderResult = z.infer<typeof CreateOrderResultSchema>;

export interface AmendOrderParams extends OrderPriceFields {
  orderId: number;
  volume?: number;
}

export const AmendOrderResultSchema = WriteResultSchema;
export type AmendOrderResult = z.infer<typeof AmendOrderResultSchema>;

export interface CancelOrderParams {
  orderId: number;
}

export const CancelOrderResultSchema = WriteResultSchema;
export type CancelOrderResult = z.infer<typeof CancelOrderResultSchema>;

export interface AmendPositionParams {
  positionId: number;
  /** Nouveau SL en prix affiché (pas en pipettes) ; omis = inchangé */
  stopLoss?: number;
  /** Nouveau TP en prix affiché (pas en pipettes) ; omis = inchangé */
  takeProfit?: number;
  trailingStopLoss?: boolean;
}

export const AmendPositionResultSchema = WriteResultSchema;
export type AmendPositionResult = z.infer<typeof AmendPositionResultSchema>;

export interface ClosePositionParams {
  positionId: number;
  /** Volume à clôturer en 1/100 d'unité d'actif de base (volume = lots × lotSize × 100) */
  volume: number;
}

export const ClosePositionResultSchema = WriteResultSchema;
export type ClosePositionResult = z.infer<typeof ClosePositionResultSchema>;
