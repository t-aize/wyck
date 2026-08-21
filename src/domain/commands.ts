/** Parsing des commandes CLI-style (`trade`, `modify`) tapées dans le CommandBar. */

import { Schema } from "effect";
import { roundPrice } from "../constants.ts";
import type { CtraderOrder, TradeSide } from "../ctrader/schemas.ts";
import { ATR_TIMEFRAME_LABELS, type AtrTimeframeLabel } from "./smc/timeframes.ts";
import type { AtrTradeInput, PreparedTrade, TradeInput } from "./trading.ts";

/**
 * Coercion + validation d'un champ numérique (chaîne → nombre fini) : remplace les 5
 * `Number(raw); if (!Number.isFinite(raw))` dupliqués par un seul schéma déclaratif
 * réutilisé pour risque/entrée/sl/tp/id. Le tokenizing lui-même (flags `--sl`, arité positionnelle
 * du risque%) reste du code impératif ordinaire : ce n'est pas de la validation de données mais du
 * parsing de ligne de commande, un fit naturellement pauvre pour Schema.
 */
const FiniteNumberFromString = Schema.NumberFromString.pipe(
  Schema.filter((n) => Number.isFinite(n)),
);

/** `undefined` si `raw` n'est pas un nombre fini (chaîne vide/absente incluse) — le message
 * d'erreur, lui, reste composé par chaque appelant (le label diffère : "risque"/"entrée"/"sl"...). */
function decodeFiniteNumber(raw: string): number | undefined {
  const result = Schema.decodeUnknownEither(FiniteNumberFromString)(raw);
  return result._tag === "Right" ? result.right : undefined;
}

export const TRADE_USAGE =
  "usage : trade [<risque%>] <entrée|market> <sl> <tp>  (direction déduite du SL/TP ; " +
  "risque% optionnel si un défaut est défini avec `risk`)";

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

/** Parse un flag `--sl`/`--tp` optionnel : absent → `{}`, invalide → `{ error }`. */
function parseOptionalPrice(
  raw: string | undefined,
  label: string,
): { value?: number; error?: string } {
  if (raw === undefined) return {};
  const value = decodeFiniteNumber(raw);
  if (value === undefined) return { error: `${label} invalide : "${raw}"` };
  return { value: roundPrice(value) };
}

/**
 * Retourne le `TradeInput` parsé, ou un message d'erreur (string) à afficher tel quel.
 * `defaultRiskPercent` (réglé via la commande `risk`) rend le risque% optionnel : avec
 * exactement 3 arguments, ils sont interprétés comme (entrée, sl, tp) plutôt que
 * (risque, entrée, sl) — sinon le risque% reste le premier argument comme d'habitude.
 */
export function parseTradeCommand(
  args: string[],
  defaultRiskPercent?: number,
): TradeInput | string {
  let riskRaw: string | undefined;
  let entryRaw: string | undefined;
  let slRaw: string | undefined;
  let tpRaw: string | undefined;

  if (args.length === 3 && defaultRiskPercent !== undefined) {
    riskRaw = String(defaultRiskPercent);
    [entryRaw, slRaw, tpRaw] = args;
  } else {
    [riskRaw, entryRaw, slRaw, tpRaw] = args;
  }

  if (!riskRaw || !entryRaw || !slRaw || !tpRaw) return `arguments manquants — ${TRADE_USAGE}`;

  const riskPercent = decodeFiniteNumber(riskRaw);
  if (riskPercent === undefined) return `risque invalide : "${riskRaw}"`;

  const isMarket = entryRaw.toLowerCase() === "market";
  const entryNumber = isMarket ? undefined : decodeFiniteNumber(entryRaw);
  if (!isMarket && entryNumber === undefined) {
    return `entrée invalide : "${entryRaw}"`;
  }
  const entry = entryNumber === undefined ? "market" : roundPrice(entryNumber);

  const stopLoss = decodeFiniteNumber(slRaw);
  if (stopLoss === undefined) return `sl invalide : "${slRaw}"`;

  const takeProfit = decodeFiniteNumber(tpRaw);
  if (takeProfit === undefined) return `tp invalide : "${tpRaw}"`;

  return { entry, riskPercent, stopLoss: roundPrice(stopLoss), takeProfit: roundPrice(takeProfit) };
}

export const ATR_TRADE_USAGE =
  "usage (mode ATR) : trade [<risque%>] <entrée|market> <buy|sell>  (SL/TP calculés depuis " +
  "l'ATR(14) M5 ; risque% optionnel si un défaut est défini avec `risk`)";

/**
 * Équivalent de `parseTradeCommand` pour le mode ATR (basculé via Shift+Tab, cf.
 * CommandBar.tsx) : plus de SL/TP saisis à la main, la direction est donnée directement
 * (`buy`/`sell`) plutôt que déduite. Même règle de risque% optionnel que `parseTradeCommand`
 * (2 arguments avec un défaut défini, sinon 3).
 */
export function parseAtrTradeCommand(
  args: string[],
  defaultRiskPercent?: number,
): AtrTradeInput | string {
  let riskRaw: string | undefined;
  let entryRaw: string | undefined;
  let sideRaw: string | undefined;

  if (args.length === 2 && defaultRiskPercent !== undefined) {
    riskRaw = String(defaultRiskPercent);
    [entryRaw, sideRaw] = args;
  } else {
    [riskRaw, entryRaw, sideRaw] = args;
  }

  if (!riskRaw || !entryRaw || !sideRaw) return `arguments manquants — ${ATR_TRADE_USAGE}`;

  const riskPercent = decodeFiniteNumber(riskRaw);
  if (riskPercent === undefined) return `risque invalide : "${riskRaw}"`;

  const isMarket = entryRaw.toLowerCase() === "market";
  const entryNumber = isMarket ? undefined : decodeFiniteNumber(entryRaw);
  if (!isMarket && entryNumber === undefined) {
    return `entrée invalide : "${entryRaw}"`;
  }
  const entry = entryNumber === undefined ? "market" : roundPrice(entryNumber);

  const sideLower = sideRaw.toLowerCase();
  if (sideLower !== "buy" && sideLower !== "sell") {
    return `direction invalide : "${sideRaw}" (buy/sell attendu)`;
  }
  const side: TradeSide = sideLower === "buy" ? "BUY" : "SELL";

  return { entry, riskPercent, side };
}

export interface ModifyInput {
  id: number;
  stopLoss?: number;
  takeProfit?: number;
}

export const MODIFY_USAGE = "usage : modify <id> [--sl <prix>] [--tp <prix>]  (raccourcis -sl -tp)";

/** Retourne le `ModifyInput` parsé, ou un message d'erreur (string) à afficher tel quel. */
export function parseModifyCommand(args: string[]): ModifyInput | string {
  const id = decodeFiniteNumber(args[0] ?? "");
  if (id === undefined) return `id invalide : "${args[0] ?? ""}" — ${MODIFY_USAGE}`;

  const flags = parseFlags(args.slice(1), { sl: ["-sl", "--sl"], tp: ["-tp", "--tp"] });
  if (typeof flags === "string") return `${flags} — ${MODIFY_USAGE}`;

  const sl = parseOptionalPrice(flags.sl, "sl");
  if (sl.error) return sl.error;
  const tp = parseOptionalPrice(flags.tp, "tp");
  if (tp.error) return tp.error;

  if (sl.value === undefined && tp.value === undefined) {
    return `au moins --sl ou --tp requis — ${MODIFY_USAGE}`;
  }

  return { id, stopLoss: sl.value, takeProfit: tp.value };
}

export const CANCEL_USAGE = "usage : cancel <id> [id...] | cancel all";

export interface CancelTargetsAll {
  kind: "all";
  orders: CtraderOrder[];
}
export interface CancelTargetsIds {
  kind: "ids";
  orders: CtraderOrder[];
}
/** Rien à proposer en confirmation — `level` distingue un vrai souci de saisie ("error") d'un
 * simple constat ("info", ex. "cancel all" sans aucun ordre en attente). */
export interface CancelTargetsRejected {
  kind: "rejected";
  level: "info" | "error";
  message: string;
}
export type CancelTargets = CancelTargetsAll | CancelTargetsIds | CancelTargetsRejected;

/**
 * Résout `cancel <id...>`/`cancel all` en la liste d'ordres réellement ciblée, étant donné les
 * ordres en attente actuels — extrait de useCancelConfirm.ts pour rester testable sans monter de
 * hook React.
 */
export function resolveCancelTargets(args: string[], pendingOrders: CtraderOrder[]): CancelTargets {
  if (args.length === 0) return { kind: "rejected", level: "error", message: CANCEL_USAGE };

  if (args[0]?.toLowerCase() === "all") {
    if (pendingOrders.length === 0) {
      return { kind: "rejected", level: "info", message: "aucun ordre en attente à annuler" };
    }
    return { kind: "all", orders: pendingOrders };
  }

  const ids = args.map(Number);
  const invalidIndex = ids.findIndex((id) => !Number.isFinite(id));
  if (invalidIndex !== -1) {
    return { kind: "rejected", level: "error", message: `id invalide : "${args[invalidIndex]}"` };
  }

  // Même limite que `modify` : seuls les ordres en attente (structure vérifiée) sont annulables
  // pour l'instant, pas les positions ouvertes (closePosition non exercé).
  const orders: CtraderOrder[] = [];
  for (const id of ids) {
    const order = pendingOrders.find((o) => o.orderId === id);
    if (!order)
      return { kind: "rejected", level: "error", message: `ordre en attente ${id} introuvable` };
    orders.push(order);
  }
  return { kind: "ids", orders };
}

export const RISK_USAGE =
  "usage : risk <risque%>  (risque par défaut pour `trade`, valable cette session)";

/** Retourne le risque% parsé, ou un message d'erreur (string) à afficher tel quel. */
export function parseRiskCommand(args: string[]): number | string {
  const raw = args[0];
  if (raw === undefined) return `risque manquant — ${RISK_USAGE}`;
  const value = decodeFiniteNumber(raw);
  if (value === undefined || value <= 0 || value > 100) {
    return `risque invalide : "${raw}" — ${RISK_USAGE}`;
  }
  return value;
}

export const ATR_SETTINGS_USAGE =
  "usage : atr [rr <valeur> | mult <valeur> | period <entier> | timeframe " +
  `<${ATR_TIMEFRAME_LABELS.join("|")}>]  (sans argument : affiche les réglages actuels)`;

export interface AtrSettingsUpdate {
  rewardRiskRatio?: number;
  atrMultiplier?: number;
  atrPeriod?: number;
  atrTimeframe?: AtrTimeframeLabel;
}

function isAtrTimeframeLabel(value: string): value is AtrTimeframeLabel {
  return (ATR_TIMEFRAME_LABELS as readonly string[]).includes(value);
}

/** `{}` sans argument (l'appelant affiche alors les réglages actuels) — un message d'erreur (string)
 * sinon. Les réglages sont persistés dans config.json par l'appelant (cf. useOrderActions.ts).
 * `timeframe` est verrouillé sur `ATR_TIMEFRAME_LABELS` (liste fermée, pas une string libre) — cf.
 * commentaire équivalent dans config.ts#AppConfigSchema. */
export function parseAtrSettingsCommand(args: string[]): AtrSettingsUpdate | string {
  if (args.length === 0) return {};

  const [sub, valueRaw] = args;

  if (sub?.toLowerCase() === "timeframe") {
    const upper = valueRaw?.toUpperCase() ?? "";
    if (!isAtrTimeframeLabel(upper)) {
      return `timeframe invalide : "${valueRaw ?? ""}" — ${ATR_SETTINGS_USAGE}`;
    }
    return { atrTimeframe: upper };
  }

  const value = decodeFiniteNumber(valueRaw ?? "");
  if (value === undefined) return `valeur invalide : "${valueRaw ?? ""}" — ${ATR_SETTINGS_USAGE}`;

  switch (sub?.toLowerCase()) {
    case "rr":
      if (value <= 0) return `RR invalide : "${valueRaw}" — ${ATR_SETTINGS_USAGE}`;
      return { rewardRiskRatio: value };
    case "mult":
      if (value <= 0) return `multiplicateur invalide : "${valueRaw}" — ${ATR_SETTINGS_USAGE}`;
      return { atrMultiplier: value };
    case "period":
      if (!Number.isInteger(value) || value < 2) {
        return `période invalide : "${valueRaw}" — ${ATR_SETTINGS_USAGE}`;
      }
      return { atrPeriod: value };
    default:
      return `sous-commande inconnue : "${sub ?? ""}" — ${ATR_SETTINGS_USAGE}`;
  }
}

export function formatTradeSummary(trade: PreparedTrade): string {
  return (
    `${trade.tradeSide} ${trade.orderType} ${trade.entryPrice.toFixed(2)} · ` +
    `SL ${trade.stopLoss.toFixed(2)} · TP ${trade.takeProfit.toFixed(2)} · ` +
    `${trade.volumeLots.toFixed(2)} lots · risque ${trade.riskAmount.toFixed(2)} (${trade.riskPercent}%)`
  );
}
