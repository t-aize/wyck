import { Effect } from "effect";
import { prepareTrade } from "../trading/prepare.ts";
import { toMessage } from "../utils/errors.ts";
import { parseFiniteNumber, parseFlags, parsePrice } from "./_shared.ts";
import type { Command } from "./types.ts";

export const TRADE_USAGE =
  "usage : trade <entrée|market> --sl <prix> --tp <prix> [--risk <pct>]  " +
  "(direction déduite du SL/TP ; --risk optionnel si un défaut est défini avec `risk`)";

const TRADE_FLAG_ALIASES = {
  sl: ["--sl", "-sl"],
  tp: ["--tp", "-tp"],
  risk: ["--risk", "-risk"],
};

/**
 * Params en flags nommés plutôt que positionnels — remplace l'ancien `trade [<risk%>] <entrée> <sl>
 * <tp>`, dont l'arité changeait de sens selon qu'un risque par défaut était réglé ou non (3 args →
 * entrée/sl/tp, 4 args → risk/entrée/sl/tp). Un argument oublié se faisait alors réinterpréter
 * silencieusement plutôt que rejeté — dangereux pour une commande qui engage de l'argent réel. Avec
 * des flags, chaque valeur est toujours auto-descriptive, quel que soit le nombre d'arguments fournis.
 */
export const tradeCommand: Command = {
  name: "trade",
  usage: TRADE_USAGE,
  summary: "prépare un ordre (risque, SL, TP) et demande confirmation avant envoi",
  run(args, ctx) {
    if (!ctx.symbolId) {
      ctx.setFeedback({ kind: "error", message: "pas encore connecté au serveur" });
      return;
    }

    const [entryRaw, ...rest] = args;
    if (!entryRaw) {
      ctx.setFeedback({ kind: "error", message: `entrée manquante — ${TRADE_USAGE}` });
      return;
    }

    const flags = parseFlags(rest, TRADE_FLAG_ALIASES);
    if (typeof flags === "string") {
      ctx.setFeedback({ kind: "error", message: `${flags} — ${TRADE_USAGE}` });
      return;
    }

    // Échoue ici plutôt qu'après l'aller-retour réseau de prepareTrade (spot price + balance) : un
    // risque hors bornes est une erreur de saisie, pas la peine d'attendre le réseau pour le savoir.
    const riskPercent = parseFiniteNumber(flags.risk) ?? ctx.defaultRiskPercent;
    if (riskPercent === undefined) {
      ctx.setFeedback({
        kind: "error",
        message: `--risk requis (aucun défaut réglé — voir la commande \`risk\`) — ${TRADE_USAGE}`,
      });
      return;
    }
    if (riskPercent <= 0 || riskPercent > 100) {
      ctx.setFeedback({
        kind: "error",
        message: `risque invalide : "${riskPercent}" — doit être entre 0 et 100`,
      });
      return;
    }

    const isMarket = entryRaw.toLowerCase() === "market";
    const entry = isMarket ? ("market" as const) : parsePrice(entryRaw);
    if (entry === undefined) {
      ctx.setFeedback({
        kind: "error",
        message: `entrée invalide : "${entryRaw}" — ${TRADE_USAGE}`,
      });
      return;
    }

    const stopLoss = parsePrice(flags.sl);
    if (stopLoss === undefined) {
      ctx.setFeedback({
        kind: "error",
        message: `sl invalide ou manquant : "${flags.sl ?? ""}" — ${TRADE_USAGE}`,
      });
      return;
    }

    const takeProfit = parsePrice(flags.tp);
    if (takeProfit === undefined) {
      ctx.setFeedback({
        kind: "error",
        message: `tp invalide ou manquant : "${flags.tp ?? ""}" — ${TRADE_USAGE}`,
      });
      return;
    }

    ctx.setFeedback({ kind: "info", message: "calcul en cours…" });
    void Effect.runPromise(
      prepareTrade(ctx.client, ctx.symbolId, { entry, riskPercent, stopLoss, takeProfit }),
    ).then(ctx.proposeTrade, (error) =>
      ctx.setFeedback({ kind: "error", message: toMessage(error) }),
    );
  },
};
