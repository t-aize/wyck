/**
 * Calcul d'un trade à partir de trois entrées (entrée, SL, TP) + risque : direction
 * déduite du SL/TP, type d'ordre déduit du prix d'entrée vs marché, taille de
 * position dérivée du risque en % d'équity.
 *
 * Convention prix : l'API cTrader renvoie des prix bruts à l'échelle x10^5 sur les
 * endpoints de lecture (spot, trendbars) mais attend des prix "affichés" (divisés
 * par 10^5) sur les endpoints d'écriture (create_order). Tout dans ce fichier
 * travaille en prix affiché — la conversion x10^5 se fait une seule fois, à la
 * lecture des données brutes.
 */

import { Effect } from "effect";
import { LOT_VOLUME, PRICE_SCALE, roundPrice } from "../constants.ts";
import type { CtraderClient, CtraderMcpError } from "../ctrader/client.ts";
import type {
  AmendOrderParams,
  CreateOrderParams,
  CtraderOrder,
  OrderType,
  TradeSide,
} from "../ctrader/schemas.ts";
import type { AtrTimeframeLabel } from "./smc/timeframes.ts";
import type { Trend } from "./smc/trend.ts";

/**
 * Erreur de validation métier d'un trade (risque%, prix indisponible, SL/TP incohérents, volume
 * sous le minimum, ATR indisponible…). Une seule classe : l'ancienne hiérarchie de 7 sous-types
 * tagués (`Data.TaggedError`, un par ancien `throw new Error(...)` distinct) n'était discriminée
 * par aucun appelant — tous se contentent de `.message` (`toMessage`, `error.rejects.toThrow(...)`
 * dans les tests) — donc la distinction par tag n'apportait rien en pratique.
 */
export class TradeValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TradeValidationError";
  }
}

export interface TradeInput {
  /** "market" = prix courant (ordre MARKET) ; un nombre = prix affiché (LIMIT/STOP déduit) */
  entry: number | "market";
  /** % de l'équity du compte */
  riskPercent: number;
  stopLoss: number;
  takeProfit: number;
}

export interface PreparedTrade {
  orderType: OrderType;
  tradeSide: TradeSide;
  entryPrice: number;
  stopLoss: number;
  takeProfit: number;
  /** Volume au format API (1/100 d'once pour XAUUSD) */
  volume: number;
  /** Volume en lots (1 lot = 100 onces), pour l'affichage */
  volumeLots: number;
  riskAmount: number;
  riskPercent: number;
  /** Gain potentiel si le TP est atteint (même formule que riskAmount, distance TP) */
  rewardAmount: number;
  /** Présent uniquement pour un trade préparé par `prepareAtrTrade` — `atrMultiplier`/
   * `rewardRiskRatio` sont figés à la création (le système de suivi, cf.
   * ui/hooks/useAtrOrderTracking.ts, les relit tels quels à chaque réamend, jamais depuis les
   * réglages globaux au moment du réamend — c'est la promesse de R:R faite au trader à la
   * confirmation). `atrPeriod`/`atrTimeframe` ne sont capturés que pour l'affichage (cf.
   * TradeConfirmModal.tsx) : le suivi, lui, recalcule toujours avec le réglage global *courant*
   * (`atr period`/`atr timeframe`), pas celui figé ici — voulu : l'intérêt du suivi est de rester
   * cohérent avec la lecture de volatilité la plus récente du trader. */
  atrTracking?: {
    atrMultiplier: number;
    rewardRiskRatio: number;
    atrPeriod: number;
    atrTimeframe: AtrTimeframeLabel;
  };
}

// Pas/minimum de volume imposés par ce compte sur XAUUSD : 0.01 lot (confirmé via la
// plateforme du broker — pas de dropdown 0.01→1.00 lot par incréments de 0.01).
export const VOLUME_STEP = 100; // 0.01 lot

/** Partie pure du calcul de volume (arrondi au pas du compte, sans validation) — extraite pour être
 * réutilisée par `computeAtrAmendments` (cf. atrTracking.ts), qui doit recalculer le volume à chaque
 * réamend ATR pour garder le risque$ constant quand l'ATR bouge, sans dupliquer la formule ni son
 * habillage Effect (une amende de suivi n'a pas de canal d'erreur utilisateur comme `prepareTrade`). */
export function computeVolumeFromRisk(riskAmount: number, stopDistance: number): number {
  const ounces = riskAmount / stopDistance;
  return Math.round((ounces * 100) / VOLUME_STEP) * VOLUME_STEP;
}

function computeVolume(
  riskAmount: number,
  stopDistance: number,
): Effect.Effect<number, TradeValidationError> {
  return Effect.gen(function* () {
    if (stopDistance <= 0) {
      return yield* Effect.fail(
        new TradeValidationError("Distance de stop invalide (SL identique à l'entrée ?)"),
      );
    }
    const volume = computeVolumeFromRisk(riskAmount, stopDistance);
    if (volume < VOLUME_STEP) {
      return yield* Effect.fail(
        new TradeValidationError(
          `Volume calculé (${(volume / LOT_VOLUME).toFixed(4)} lot) sous le minimum de ce compte ` +
            "(0.01 lot) — augmente le risque% ou resserre le stop",
        ),
      );
    }
    return volume;
  });
}

function toPoints(priceDistance: number): number {
  return Math.round(priceDistance * PRICE_SCALE);
}

/** LIMIT/STOP déduit de la position de l'entrée par rapport au prix de référence (ask pour BUY, bid pour SELL). */
function inferOrderType(side: TradeSide, entryPrice: number, referencePrice: number): OrderType {
  if (entryPrice === referencePrice) return "MARKET";
  if (side === "BUY") return entryPrice > referencePrice ? "STOP" : "LIMIT";
  return entryPrice < referencePrice ? "STOP" : "LIMIT";
}

function validateRiskPercent(riskPercent: number): Effect.Effect<void, TradeValidationError> {
  if (!Number.isFinite(riskPercent) || riskPercent <= 0 || riskPercent > 100) {
    return Effect.fail(
      new TradeValidationError("Risque invalide : doit être un pourcentage entre 0 et 100"),
    );
  }
  return Effect.void;
}

/** Fetch spot+balance concurrent, commun à `prepareTrade`/`prepareAtrTrade` — prix déjà convertis en
 * prix affiché (÷ PRICE_SCALE), comme le reste de ce fichier. */
function fetchTradeContext(
  client: CtraderClient,
  symbolId: number,
): Effect.Effect<
  { bid: number; ask: number; equity: number; moneyDigits: number },
  TradeValidationError | CtraderMcpError
> {
  return Effect.gen(function* () {
    const [{ prices }, { equity, moneyDigits }] = yield* Effect.all(
      [client.getSpotPrices({ symbolId: [symbolId] }), client.getBalance()],
      { concurrency: "unbounded" },
    );

    const spot = prices[0];
    if (!spot) {
      return yield* Effect.fail(new TradeValidationError("Prix indisponible pour ce symbole"));
    }
    return { bid: spot.bid / PRICE_SCALE, ask: spot.ask / PRICE_SCALE, equity, moneyDigits };
  });
}

/** Résout le prix d'entrée effectif et le type d'ordre à partir de la direction — commun à
 * `prepareTrade` (direction déduite du SL/TP) et `prepareAtrTrade` (direction donnée). */
function resolveEntry(
  side: TradeSide,
  entry: number | "market",
  reference: number,
): { entryPrice: number; orderType: OrderType } {
  const entryPrice = entry === "market" ? reference : entry;
  const orderType: OrderType =
    entry === "market" ? "MARKET" : inferOrderType(side, entryPrice, reference);
  return { entryPrice, orderType };
}

/**
 * SL = entrée ∓ (multiplicateur × ATR) ; TP = SL étendu au ratio récompense:risque — direction
 * dépend du côté (BUY : SL en dessous, SELL : SL au-dessus). `atrValue` en prix affiché (pas
 * l'échelle brute x10^5 des bougies — cf. commentaire de tête du fichier).
 */
export function computeAtrLevels(
  side: TradeSide,
  entryPrice: number,
  atrValue: number,
  atrMultiplier: number,
  rewardRiskRatio: number,
): { stopLoss: number; takeProfit: number } {
  const stopDistance = atrValue * atrMultiplier;
  const rewardDistance = stopDistance * rewardRiskRatio;
  return side === "BUY"
    ? {
        stopLoss: roundPrice(entryPrice - stopDistance),
        takeProfit: roundPrice(entryPrice + rewardDistance),
      }
    : {
        stopLoss: roundPrice(entryPrice + stopDistance),
        takeProfit: roundPrice(entryPrice - rewardDistance),
      };
}

/**
 * `client` reçu en paramètre explicite, comme partout ailleurs dans l'app (cf. commentaire de tête
 * de `CtraderClient`) — pas de DI Effect ici. Chaque règle de validation échoue via
 * `Effect.fail(new TradeValidationError(...))` plutôt qu'un `throw` générique, pour rester dans le
 * canal d'erreur typé d'Effect.
 */
export function prepareTrade(
  client: CtraderClient,
  symbolId: number,
  input: TradeInput,
): Effect.Effect<PreparedTrade, TradeValidationError | CtraderMcpError> {
  return Effect.gen(function* () {
    yield* validateRiskPercent(input.riskPercent);
    const { bid, ask, equity, moneyDigits } = yield* fetchTradeContext(client, symbolId);

    const { stopLoss, takeProfit } = input;
    if (stopLoss === takeProfit) {
      return yield* Effect.fail(
        new TradeValidationError("SL et TP ne peuvent pas être identiques"),
      );
    }
    // direction déduite du SL/TP : BUY si le SL est sous le TP, SELL sinon.
    const side: TradeSide = stopLoss < takeProfit ? "BUY" : "SELL";
    const reference = side === "BUY" ? ask : bid;
    const { entryPrice, orderType } = resolveEntry(side, input.entry, reference);

    if (side === "BUY" && !(stopLoss < entryPrice && takeProfit > entryPrice)) {
      return yield* Effect.fail(
        new TradeValidationError(
          "Incohérent pour un achat : le SL doit être sous l'entrée et le TP au-dessus",
        ),
      );
    }
    if (side === "SELL" && !(stopLoss > entryPrice && takeProfit < entryPrice)) {
      return yield* Effect.fail(
        new TradeValidationError(
          "Incohérent pour une vente : le SL doit être au-dessus de l'entrée et le TP en dessous",
        ),
      );
    }

    const stopDistance = Math.abs(entryPrice - stopLoss);
    const targetDistance = Math.abs(entryPrice - takeProfit);
    const riskAmount = (equity / 10 ** moneyDigits) * (input.riskPercent / 100);
    const volume = yield* computeVolume(riskAmount, stopDistance);

    const trade: PreparedTrade = {
      orderType,
      tradeSide: side,
      entryPrice,
      stopLoss,
      takeProfit,
      volume,
      volumeLots: volume / LOT_VOLUME,
      riskAmount,
      riskPercent: input.riskPercent,
      rewardAmount: (volume / 100) * targetDistance,
    };
    return trade;
  });
}

export interface AtrTradeInput {
  /** "market" = prix courant (ordre MARKET) ; un nombre = prix affiché (LIMIT/STOP déduit) */
  entry: number | "market";
  /** % de l'équity du compte */
  riskPercent: number;
  /** Donnée explicitement (pas déduite d'un SL/TP, qui n'existent pas encore en mode ATR). */
  side: TradeSide;
}

/**
 * Variante de `prepareTrade` pour le mode ATR (cf. commands.ts#parseAtrTradeCommand) : la direction
 * est donnée plutôt que déduite, et SL/TP viennent de `computeAtrLevels` plutôt que d'une saisie
 * manuelle. `atr.rawValue` est l'ATR le plus récent (période/timeframe configurés, cf.
 * config.ts#DEFAULT_ATR_SETTINGS) en échelle brute x10^5 (cf. smc/trend.ts#computeAtr) —
 * `undefined` tant qu'il n'y a pas assez de bougies closes sur ce timeframe, auquel cas cette
 * fonction échoue avec `TradeValidationError` plutôt que de calculer un stop sur une valeur absente.
 * `period`/`timeframe` ne sont là que pour affichage (cf. PreparedTrade.atrTracking).
 */
export function prepareAtrTrade(
  client: CtraderClient,
  symbolId: number,
  input: AtrTradeInput,
  atr: {
    rawValue: number | undefined;
    multiplier: number;
    rewardRiskRatio: number;
    period: number;
    timeframe: AtrTimeframeLabel;
  },
): Effect.Effect<PreparedTrade, TradeValidationError | CtraderMcpError> {
  return Effect.gen(function* () {
    yield* validateRiskPercent(input.riskPercent);
    if (atr.rawValue === undefined) {
      return yield* Effect.fail(
        new TradeValidationError(
          `ATR(${atr.period}) ${atr.timeframe} pas encore disponible — réessaie dans quelques instants`,
        ),
      );
    }
    const atrValue = atr.rawValue / PRICE_SCALE;

    const { bid, ask, equity, moneyDigits } = yield* fetchTradeContext(client, symbolId);
    const { side } = input;
    const reference = side === "BUY" ? ask : bid;
    const { entryPrice, orderType } = resolveEntry(side, input.entry, reference);

    const { stopLoss, takeProfit } = computeAtrLevels(
      side,
      entryPrice,
      atrValue,
      atr.multiplier,
      atr.rewardRiskRatio,
    );

    const stopDistance = Math.abs(entryPrice - stopLoss);
    const targetDistance = Math.abs(entryPrice - takeProfit);
    const riskAmount = (equity / 10 ** moneyDigits) * (input.riskPercent / 100);
    const volume = yield* computeVolume(riskAmount, stopDistance);

    const trade: PreparedTrade = {
      orderType,
      tradeSide: side,
      entryPrice,
      stopLoss,
      takeProfit,
      volume,
      volumeLots: volume / LOT_VOLUME,
      riskAmount,
      riskPercent: input.riskPercent,
      rewardAmount: (volume / 100) * targetDistance,
      atrTracking: {
        atrMultiplier: atr.multiplier,
        rewardRiskRatio: atr.rewardRiskRatio,
        atrPeriod: atr.period,
        atrTimeframe: atr.timeframe,
      },
    };
    return trade;
  });
}

/**
 * P&L latent d'une position ouverte, calculé plutôt que lu : l'API cTrader (Open API
 * `ProtoOAPosition`, que ce MCP reflète — cf. commentaire sur `CtraderPositionSchema` dans
 * ctrader/schemas.ts)
 * n'expose aucun champ de profit latent, seulement des données réalisées (swap,
 * commission). Mark-to-market au bid pour un long (prix de sortie si on clôturait
 * maintenant), à l'ask pour un short — convention standard, cohérente avec le reste du
 * fichier qui déduit toujours le prix de référence du côté de la position.
 */
export function computeUnrealizedPnl(
  side: TradeSide,
  volumeLots: number,
  entryPrice: number,
  bid: number,
  ask: number,
): number {
  const markPrice = side === "BUY" ? bid : ask;
  const priceDiff = side === "BUY" ? markPrice - entryPrice : entryPrice - markPrice;
  return priceDiff * volumeLots * 100;
}

/**
 * `side` va-t-il à l'encontre du biais H1 confirmé — `cascade_htf_bias` de la référence, mais en
 * avertissement non bloquant plutôt qu'un blocage dur : ceci est un outil de saisie manuelle, pas
 * un exécuteur de signaux automatique, la décision finale reste au trader. `htfBias` neutre (0,
 * pas encore de tendance confirmée) ne déclenche jamais d'avertissement.
 */
export function conflictsWithHtfBias(side: TradeSide, htfBias: Trend): boolean {
  if (htfBias === 0) return false;
  const tradeDirection: Trend = side === "BUY" ? 1 : -1;
  return tradeDirection !== htfBias;
}

export function toCreateOrderParams(symbolId: number, trade: PreparedTrade): CreateOrderParams {
  const base = {
    symbolId,
    orderType: trade.orderType,
    tradeSide: trade.tradeSide,
    volume: trade.volume,
    label: "aurum",
  };

  if (trade.orderType === "MARKET") {
    return {
      ...base,
      relativeStopLoss: toPoints(Math.abs(trade.entryPrice - trade.stopLoss)),
      relativeTakeProfit: toPoints(Math.abs(trade.entryPrice - trade.takeProfit)),
    };
  }

  return {
    ...base,
    limitPrice: trade.orderType === "LIMIT" ? trade.entryPrice : undefined,
    stopPrice: trade.orderType === "STOP" ? trade.entryPrice : undefined,
    stopLoss: trade.stopLoss,
    takeProfit: trade.takeProfit,
  };
}

/**
 * cTrader n'a pas d'amend partiel : tout champ non renvoyé sur `amend_order` est effacé côté
 * serveur (constaté sur limitPrice/stopPrice/SL/TP — cf. useModifyConfirm.ts — d'où le bug où une
 * date d'expiration disparaissait après un réamend ATR qui ne la reprenait pas). Seul point de
 * construction d'un payload amend dans l'app : reprend tout l'état resendable de l'ordre existant,
 * `changes` écrase juste ce qui doit réellement changer — impossible d'oublier un champ à un
 * nouveau point d'appel.
 */
export function toAmendOrderParams(
  order: CtraderOrder,
  changes: Partial<Omit<AmendOrderParams, "orderId">> = {},
): AmendOrderParams {
  // Un override explicitement `undefined` (ex. proposeModify appelé sans nouveau SL) doit garder
  // la valeur existante, pas l'effacer — on ne spread que les clés réellement fournies.
  const definedChanges = Object.fromEntries(
    Object.entries(changes).filter(([, value]) => value !== undefined),
  );
  return {
    orderId: order.orderId,
    volume: order.volume,
    limitPrice: order.limitPrice,
    stopPrice: order.stopPrice,
    stopLoss: order.stopLoss,
    takeProfit: order.takeProfit,
    expirationTimestamp: order.expirationTimestamp,
    ...definedChanges,
  };
}
