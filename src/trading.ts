/**
 * Calcul d'un trade à partir de trois entrées (direction, entrée, risque) : SL/TP
 * automatiques (ATR/RR) si non fournis, taille de position dérivée du risque en %
 * d'équity, type d'ordre déduit du prix d'entrée vs marché.
 *
 * Convention prix : l'API cTrader renvoie des prix bruts à l'échelle x10^5 sur les
 * endpoints de lecture (spot, trendbars) mais attend des prix "affichés" (divisés
 * par 10^5) sur les endpoints d'écriture (create_order). Tout dans ce fichier
 * travaille en prix affiché — la conversion x10^5 se fait une seule fois, à la
 * lecture des données brutes.
 */

import type {
  CreateOrderParams,
  CtraderClient,
  OrderType,
  TradeSide,
  TrendbarPeriod,
} from "./ctrader-client.ts";
import { env } from "./env.ts";

const PRICE_SCALE = 100_000;

const PERIOD_MS: Record<TrendbarPeriod, number> = {
  M_1: 60_000,
  M_5: 5 * 60_000,
  M_15: 15 * 60_000,
  M_30: 30 * 60_000,
  H_1: 60 * 60_000,
  H_4: 4 * 60 * 60_000,
  D_1: 24 * 60 * 60_000,
  W_1: 7 * 24 * 60 * 60_000,
  MN_1: 30 * 24 * 60 * 60_000,
};

export interface TradeInput {
  side: TradeSide;
  /** "market" = prix courant (ordre MARKET) ; un nombre = prix affiché (LIMIT/STOP déduit) */
  entry: number | "market";
  /** % de l'équity du compte */
  riskPercent: number;
  stopLoss?: number;
  takeProfit?: number;
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
}

/**
 * XAUUSD (métaux) : 1 lot = 100 onces, prix coté en $/once. Volume API = onces × 100
 * (cf. commentaire équivalent dans PositionsPanel.tsx). Seul symbole tradé ici — à
 * revoir si d'autres classes d'actifs sont ajoutées un jour (lotSize/valeur du point
 * diffèrent : forex, indices, crypto).
 */
const LOT_VOLUME = 10_000; // 1.00 lot en unités API (pour l'affichage, cf. volumeLots)
// Pas/minimum de volume imposés par ce compte sur XAUUSD : 0.01 lot (confirmé via la
// plateforme du broker — pas de dropdown 0.01→1.00 lot par incréments de 0.01).
const VOLUME_STEP = 100; // 0.01 lot

function computeVolume(riskAmount: number, stopDistance: number): number {
  if (stopDistance <= 0) throw new Error("Distance de stop invalide (SL identique à l'entrée ?)");
  const ounces = riskAmount / stopDistance;
  const volume = Math.round((ounces * 100) / VOLUME_STEP) * VOLUME_STEP;
  if (volume < VOLUME_STEP) {
    throw new Error(
      `Volume calculé (${(volume / LOT_VOLUME).toFixed(4)} lot) sous le minimum de ce compte ` +
        "(0.01 lot) — augmente le risque% ou resserre le stop",
    );
  }
  return volume;
}

function toPoints(priceDistance: number): number {
  return Math.round(priceDistance * PRICE_SCALE);
}

/** ATR (Average True Range) sur ATR_PERIOD bougies, moyenne simple des True Range. */
async function computeAtr(client: CtraderClient, symbolId: number): Promise<number> {
  const periodMs = PERIOD_MS[env.ATR_TIMEFRAME];
  const barsNeeded = env.ATR_PERIOD + 1;
  const marginBars = env.ATR_PERIOD + 8; // marge pour week-ends / jours fériés / bougies manquantes
  const now = Date.now();

  const { trendbars } = await client.getTrendbars({
    symbolId,
    period: env.ATR_TIMEFRAME,
    fromTimestamp: String(now - periodMs * marginBars),
    toTimestamp: String(now),
  });

  if (trendbars.length < barsNeeded) {
    throw new Error(
      `Pas assez de bougies pour l'ATR (${trendbars.length}/${barsNeeded} sur ${env.ATR_TIMEFRAME})`,
    );
  }

  const recent = [...trendbars].sort((a, b) => a.timestamp - b.timestamp).slice(-barsNeeded);
  const trueRanges: number[] = [];

  for (let i = 1; i < recent.length; i++) {
    const bar = recent[i];
    const prevBar = recent[i - 1];
    if (!bar || !prevBar) continue;
    const high = bar.high / PRICE_SCALE;
    const low = bar.low / PRICE_SCALE;
    const prevClose = prevBar.close / PRICE_SCALE;
    trueRanges.push(Math.max(high - low, Math.abs(high - prevClose), Math.abs(low - prevClose)));
  }

  if (trueRanges.length === 0) throw new Error("ATR incalculable (données de bougies vides)");
  return trueRanges.reduce((sum, tr) => sum + tr, 0) / trueRanges.length;
}

/** LIMIT/STOP déduit de la position de l'entrée par rapport au prix de référence (ask pour BUY, bid pour SELL). */
function inferOrderType(side: TradeSide, entryPrice: number, referencePrice: number): OrderType {
  if (entryPrice === referencePrice) return "MARKET";
  if (side === "BUY") return entryPrice > referencePrice ? "STOP" : "LIMIT";
  return entryPrice < referencePrice ? "STOP" : "LIMIT";
}

export async function prepareTrade(
  client: CtraderClient,
  symbolId: number,
  input: TradeInput,
): Promise<PreparedTrade> {
  if (!Number.isFinite(input.riskPercent) || input.riskPercent <= 0 || input.riskPercent > 100) {
    throw new Error("Risque invalide : doit être un pourcentage entre 0 et 100");
  }

  const [{ prices }, { equity, moneyDigits }] = await Promise.all([
    client.getSpotPrices({ symbolId: [symbolId] }),
    client.getBalance(),
  ]);
  const spot = prices[0];
  if (!spot) throw new Error("Prix indisponible pour ce symbole");
  const bid = spot.bid / PRICE_SCALE;
  const ask = spot.ask / PRICE_SCALE;
  const reference = input.side === "BUY" ? ask : bid;

  const entryPrice = input.entry === "market" ? reference : input.entry;
  const orderType: OrderType =
    input.entry === "market" ? "MARKET" : inferOrderType(input.side, entryPrice, reference);

  let stopLoss = input.stopLoss;
  let takeProfit = input.takeProfit;

  if (stopLoss === undefined || takeProfit === undefined) {
    const atr = await computeAtr(client, symbolId);
    const distance = atr * env.ATR_MULTIPLIER;
    if (stopLoss === undefined) {
      stopLoss = input.side === "BUY" ? entryPrice - distance : entryPrice + distance;
    }
    if (takeProfit === undefined) {
      const slDistance = Math.abs(entryPrice - stopLoss);
      takeProfit =
        input.side === "BUY"
          ? entryPrice + slDistance * env.DEFAULT_RR
          : entryPrice - slDistance * env.DEFAULT_RR;
    }
  }

  if (input.side === "BUY" && !(stopLoss < entryPrice && takeProfit > entryPrice)) {
    throw new Error("Incohérent pour un BUY : le SL doit être sous l'entrée et le TP au-dessus");
  }
  if (input.side === "SELL" && !(stopLoss > entryPrice && takeProfit < entryPrice)) {
    throw new Error(
      "Incohérent pour un SELL : le SL doit être au-dessus de l'entrée et le TP en dessous",
    );
  }

  const stopDistance = Math.abs(entryPrice - stopLoss);
  const targetDistance = Math.abs(entryPrice - takeProfit);
  const riskAmount = (equity / 10 ** moneyDigits) * (input.riskPercent / 100);
  const volume = computeVolume(riskAmount, stopDistance);

  return {
    orderType,
    tradeSide: input.side,
    entryPrice,
    stopLoss,
    takeProfit,
    volume,
    volumeLots: volume / LOT_VOLUME,
    riskAmount,
    riskPercent: input.riskPercent,
    rewardAmount: (volume / 100) * targetDistance,
  };
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

export const TRADE_USAGE =
  "usage : trade <buy|sell> --risk <%> [--entry <prix|market>] [--sl <prix>] [--tp <prix>]  " +
  "(raccourcis -r -e -sl -tp ; entry par défaut market ; sl/tp omis → calculés à l'ATR)";

/**
 * Parseur minimal `--flag valeur` / `-f valeur` (style CLI, ordre libre). `aliases` mappe une
 * clé logique vers ses formes acceptées sur la ligne de commande.
 */
function parseFlags(
  args: string[],
  aliases: Record<string, string[]>,
): Record<string, string> | string {
  const flagToKey = new Map<string, string>();
  for (const [key, flags] of Object.entries(aliases)) {
    for (const flag of flags) flagToKey.set(flag, key);
  }

  const result: Record<string, string> = {};
  for (let i = 0; i < args.length; i += 2) {
    const flag = args[i];
    const key = flag && flagToKey.get(flag);
    if (!key) return `option inconnue : "${flag ?? ""}"`;
    const value = args[i + 1];
    if (value === undefined) return `valeur manquante pour ${flag}`;
    result[key] = value;
  }
  return result;
}

/** Retourne le `TradeInput` parsé, ou un message d'erreur (string) à afficher tel quel. */
export function parseTradeCommand(args: string[]): TradeInput | string {
  const sideRaw = args[0]?.toUpperCase();
  if (sideRaw !== "BUY" && sideRaw !== "SELL") {
    return `direction invalide : "${args[0] ?? ""}" (buy/sell attendu) — ${TRADE_USAGE}`;
  }

  const flags = parseFlags(args.slice(1), {
    risk: ["-r", "--risk"],
    entry: ["-e", "--entry"],
    sl: ["-sl", "--sl"],
    tp: ["-tp", "--tp"],
  });
  if (typeof flags === "string") return `${flags} — ${TRADE_USAGE}`;

  if (flags.risk === undefined) return `--risk requis — ${TRADE_USAGE}`;
  const riskPercent = Number(flags.risk);
  if (!Number.isFinite(riskPercent)) return `risque invalide : "${flags.risk}"`;

  const entryRaw = flags.entry?.toLowerCase() ?? "market";
  const entry = entryRaw === "market" ? "market" : Number(flags.entry);
  if (entry !== "market" && !Number.isFinite(entry)) {
    return `entrée invalide : "${flags.entry ?? ""}"`;
  }

  let stopLoss: number | undefined;
  if (flags.sl !== undefined) {
    stopLoss = Number(flags.sl);
    if (!Number.isFinite(stopLoss)) return `sl invalide : "${flags.sl}"`;
  }

  let takeProfit: number | undefined;
  if (flags.tp !== undefined) {
    takeProfit = Number(flags.tp);
    if (!Number.isFinite(takeProfit)) return `tp invalide : "${flags.tp}"`;
  }

  return { side: sideRaw, entry, riskPercent, stopLoss, takeProfit };
}

export interface ModifyInput {
  id: number;
  stopLoss?: number;
  takeProfit?: number;
}

export const MODIFY_USAGE = "usage : modify <id> [--sl <prix>] [--tp <prix>]  (raccourcis -sl -tp)";

/** Retourne le `ModifyInput` parsé, ou un message d'erreur (string) à afficher tel quel. */
export function parseModifyCommand(args: string[]): ModifyInput | string {
  const id = Number(args[0]);
  if (!Number.isFinite(id)) return `id invalide : "${args[0] ?? ""}" — ${MODIFY_USAGE}`;

  const flags = parseFlags(args.slice(1), { sl: ["-sl", "--sl"], tp: ["-tp", "--tp"] });
  if (typeof flags === "string") return `${flags} — ${MODIFY_USAGE}`;

  let stopLoss: number | undefined;
  if (flags.sl !== undefined) {
    stopLoss = Number(flags.sl);
    if (!Number.isFinite(stopLoss)) return `sl invalide : "${flags.sl}"`;
  }

  let takeProfit: number | undefined;
  if (flags.tp !== undefined) {
    takeProfit = Number(flags.tp);
    if (!Number.isFinite(takeProfit)) return `tp invalide : "${flags.tp}"`;
  }

  if (stopLoss === undefined && takeProfit === undefined) {
    return `au moins --sl ou --tp requis — ${MODIFY_USAGE}`;
  }

  return { id, stopLoss, takeProfit };
}

export function formatTradeSummary(trade: PreparedTrade): string {
  return (
    `${trade.tradeSide} ${trade.orderType} ${trade.entryPrice.toFixed(2)} · ` +
    `SL ${trade.stopLoss.toFixed(2)} · TP ${trade.takeProfit.toFixed(2)} · ` +
    `${trade.volumeLots.toFixed(2)} lots · risque ${trade.riskAmount.toFixed(2)} (${trade.riskPercent}%)`
  );
}
