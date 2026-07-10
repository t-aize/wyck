/** Parsing des commandes CLI-style (`trade`, `modify`) tapées dans le CommandBar. */

import { roundPrice } from "../constants.ts";
import type { PreparedTrade, TradeInput } from "./trading.ts";

export const TRADE_USAGE =
  "usage : trade <risque%> <entrée|market> <sl> <tp>  (direction déduite du SL/TP)";

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
  const value = Number(raw);
  if (!Number.isFinite(value)) return { error: `${label} invalide : "${raw}"` };
  return { value: roundPrice(value) };
}

/** Retourne le `TradeInput` parsé, ou un message d'erreur (string) à afficher tel quel. */
export function parseTradeCommand(args: string[]): TradeInput | string {
  const [riskRaw, entryRaw, slRaw, tpRaw] = args;
  if (!riskRaw || !entryRaw || !slRaw || !tpRaw) return `arguments manquants — ${TRADE_USAGE}`;

  const riskPercent = Number(riskRaw);
  if (!Number.isFinite(riskPercent)) return `risque invalide : "${riskRaw}"`;

  const entryNumber = entryRaw.toLowerCase() === "market" ? undefined : Number(entryRaw);
  if (entryNumber !== undefined && !Number.isFinite(entryNumber)) {
    return `entrée invalide : "${entryRaw}"`;
  }
  const entry = entryNumber === undefined ? "market" : roundPrice(entryNumber);

  const stopLossRaw = Number(slRaw);
  if (!Number.isFinite(stopLossRaw)) return `sl invalide : "${slRaw}"`;
  const stopLoss = roundPrice(stopLossRaw);

  const takeProfitRaw = Number(tpRaw);
  if (!Number.isFinite(takeProfitRaw)) return `tp invalide : "${tpRaw}"`;
  const takeProfit = roundPrice(takeProfitRaw);

  return { entry, riskPercent, stopLoss, takeProfit };
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

  const sl = parseOptionalPrice(flags.sl, "sl");
  if (sl.error) return sl.error;
  const tp = parseOptionalPrice(flags.tp, "tp");
  if (tp.error) return tp.error;

  if (sl.value === undefined && tp.value === undefined) {
    return `au moins --sl ou --tp requis — ${MODIFY_USAGE}`;
  }

  return { id, stopLoss: sl.value, takeProfit: tp.value };
}

export function formatTradeSummary(trade: PreparedTrade): string {
  return (
    `${trade.tradeSide} ${trade.orderType} ${trade.entryPrice.toFixed(2)} · ` +
    `SL ${trade.stopLoss.toFixed(2)} · TP ${trade.takeProfit.toFixed(2)} · ` +
    `${trade.volumeLots.toFixed(2)} lots · risque ${trade.riskAmount.toFixed(2)} (${trade.riskPercent}%)`
  );
}
