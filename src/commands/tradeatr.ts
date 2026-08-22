import { Effect } from "effect";
import type { TradeSide } from "../ctrader/schemas.ts";
import { prepareAtrTrade } from "../trading/prepareAtr.ts";
import { toMessage } from "../utils/errors.ts";
import { parseFiniteNumber, parseFlags, parsePrice } from "./_shared.ts";
import type { Command } from "./types.ts";

export const TRADEATR_USAGE =
  "usage : tradeatr <BUY|SELL> <entrée|market> [--rr <ratio>] [--risk <pct>]  " +
  "(SL = ATR(14) M5, TP = SL × RR, RR par défaut 1.2 ; --risk optionnel si un défaut est défini)";

const DEFAULT_REWARD_RISK_RATIO = 1.2;

const TRADEATR_FLAG_ALIASES = {
  rr: ["--rr", "-rr"],
  risk: ["--risk", "-risk"],
};

/** Direction explicite en argument plutôt que déduite (cf. commande `trade`) : l'ATR donne une
 * distance, pas un sens — et c'est justement l'ancien sous-système qui inférait un biais
 * (SMC/structure) qui a été supprimé (76452d9), pas de raison de le faire renaître ici. */
function parseSide(raw: string | undefined): TradeSide | undefined {
  const upper = raw?.toUpperCase();
  return upper === "BUY" || upper === "SELL" ? upper : undefined;
}

export const tradeAtrCommand: Command = {
  name: "tradeatr",
  usage: TRADEATR_USAGE,
  summary: "prépare un trade avec SL = ATR(14) M5 et TP en RR fixe (1.2 par défaut)",
  run(args, ctx) {
    if (!ctx.symbolId) {
      ctx.setFeedback({ kind: "error", message: "pas encore connecté au serveur" });
      return;
    }

    const [sideRaw, entryRaw, ...rest] = args;
    const side = parseSide(sideRaw);
    if (!side) {
      ctx.setFeedback({
        kind: "error",
        message: `direction invalide : "${sideRaw ?? ""}" — ${TRADEATR_USAGE}`,
      });
      return;
    }

    if (!entryRaw) {
      ctx.setFeedback({ kind: "error", message: `entrée manquante — ${TRADEATR_USAGE}` });
      return;
    }

    const flags = parseFlags(rest, TRADEATR_FLAG_ALIASES);
    if (typeof flags === "string") {
      ctx.setFeedback({ kind: "error", message: `${flags} — ${TRADEATR_USAGE}` });
      return;
    }

    // Échoue ici plutôt qu'après l'aller-retour réseau de prepareAtrTrade (spot + ATR), même
    // raisonnement que `trade` : une erreur de saisie ne mérite pas d'attendre le réseau.
    const riskPercent = parseFiniteNumber(flags.risk) ?? ctx.defaultRiskPercent;
    if (riskPercent === undefined) {
      ctx.setFeedback({
        kind: "error",
        message: `--risk requis (aucun défaut réglé — voir la commande \`risk\`) — ${TRADEATR_USAGE}`,
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

    const rewardRiskRatio = parseFiniteNumber(flags.rr) ?? DEFAULT_REWARD_RISK_RATIO;
    if (rewardRiskRatio <= 0) {
      ctx.setFeedback({
        kind: "error",
        message: `rr invalide : "${flags.rr}" — doit être un nombre positif`,
      });
      return;
    }

    const isMarket = entryRaw.toLowerCase() === "market";
    const entry = isMarket ? ("market" as const) : parsePrice(entryRaw);
    if (entry === undefined) {
      ctx.setFeedback({
        kind: "error",
        message: `entrée invalide : "${entryRaw}" — ${TRADEATR_USAGE}`,
      });
      return;
    }

    ctx.setFeedback({ kind: "info", message: "calcul ATR en cours…" });
    void Effect.runPromise(
      prepareAtrTrade(ctx.client, ctx.symbolId, {
        tradeSide: side,
        entry,
        riskPercent,
        rewardRiskRatio,
      }),
    ).then(ctx.proposeTrade, (error) =>
      ctx.setFeedback({ kind: "error", message: toMessage(error) }),
    );
  },
};
