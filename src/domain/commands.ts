/** Parsing des commandes CLI-style (`trade`, `modify`) tapées dans le CommandBar. */

import type { PreparedTrade, TradeInput } from "./trading.ts";

export const TRADE_USAGE =
  "usage : trade --risk <%> --sl <prix> --tp <prix> [--entry <prix|market>]  " +
  "(raccourcis -r -sl -tp -e ; entry par défaut market ; direction déduite du SL/TP)";

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
  return { value };
}

/** Retourne le `TradeInput` parsé, ou un message d'erreur (string) à afficher tel quel. */
export function parseTradeCommand(args: string[]): TradeInput | string {
  const flags = parseFlags(args, {
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

  if (flags.sl === undefined) return `--sl requis — ${TRADE_USAGE}`;
  if (flags.tp === undefined) return `--tp requis — ${TRADE_USAGE}`;
  const sl = parseOptionalPrice(flags.sl, "sl");
  if (sl.error) return sl.error;
  const tp = parseOptionalPrice(flags.tp, "tp");
  if (tp.error) return tp.error;

  return { entry, riskPercent, stopLoss: sl.value as number, takeProfit: tp.value as number };
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
