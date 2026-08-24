import { Effect } from "effect";
import type { TradeSide } from "../ctrader/schemas.ts";
import { prepareTrade } from "../trading/prepare.ts";
import { prepareAtrTrade } from "../trading/prepareAtr.ts";
import { toMessage } from "../utils/errors.ts";
import { parseFiniteNumber, parseFlags, parsePrice, resolveRiskPercent } from "./_shared.ts";
import type { Command, CommandContext } from "./types.ts";

const TRADE_MANUAL_USAGE =
  "usage : trade <entrée|market> --sl <prix> --tp <prix> [--risk <pct>]  " +
  "(direction déduite du SL/TP ; --risk optionnel si un défaut est défini avec `risk`)";
const TRADE_ATR_USAGE =
  "usage (mode ATR, Shift+Tab) : trade <BUY|SELL> <entrée|market> [--rr <ratio>] [--risk <pct>]  " +
  "(SL = ATR(14) M5, TP = SL × RR, RR par défaut 1.2)";
/** Affiché par `help trade` uniquement — les messages d'erreur inline citent chacun leur propre
 * usage (`TRADE_MANUAL_USAGE`/`TRADE_ATR_USAGE`) plutôt que cette version combinée. */
export const TRADE_USAGE = `${TRADE_MANUAL_USAGE}\n${TRADE_ATR_USAGE}`;

const DEFAULT_REWARD_RISK_RATIO = 1.2;

const MANUAL_FLAG_ALIASES = {
  sl: ["--sl", "-sl"],
  tp: ["--tp", "-tp"],
  risk: ["--risk", "-risk"],
};

const ATR_FLAG_ALIASES = {
  rr: ["--rr", "-rr"],
  risk: ["--risk", "-risk"],
};

/** Direction explicite en argument plutôt que déduite (cf. commande `trade` en mode manuel) : l'ATR
 * donne une distance, pas un sens — et c'est justement l'ancien sous-système qui inférait un biais
 * (SMC/structure) qui a été supprimé (76452d9), pas de raison de le faire renaître ici. */
function parseSide(raw: string | undefined): TradeSide | undefined {
  const upper = raw?.toUpperCase();
  return upper === "BUY" || upper === "SELL" ? upper : undefined;
}

/**
 * Params en flags nommés plutôt que positionnels — remplace l'ancien `trade [<risk%>] <entrée> <sl>
 * <tp>`, dont l'arité changeait de sens selon qu'un risque par défaut était réglé ou non (3 args →
 * entrée/sl/tp, 4 args → risk/entrée/sl/tp). Un argument oublié se faisait alors réinterpréter
 * silencieusement plutôt que rejeté — dangereux pour une commande qui engage de l'argent réel. Avec
 * des flags, chaque valeur est toujours auto-descriptive, quel que soit le nombre d'arguments fournis.
 */
function runManual(symbolId: number, args: string[], ctx: CommandContext): void {
  const [entryRaw, ...rest] = args;
  if (!entryRaw) {
    ctx.setFeedback({ kind: "error", message: `entrée manquante — ${TRADE_MANUAL_USAGE}` });
    return;
  }

  const flags = parseFlags(rest, MANUAL_FLAG_ALIASES);
  if (typeof flags === "string") {
    ctx.setFeedback({ kind: "error", message: `${flags} — ${TRADE_MANUAL_USAGE}` });
    return;
  }

  // Échoue ici plutôt qu'après l'aller-retour réseau de prepareTrade (spot price + balance) : un
  // risque hors bornes est une erreur de saisie, pas la peine d'attendre le réseau pour le savoir.
  const risk = resolveRiskPercent(flags.risk, ctx.defaultRiskPercent, TRADE_MANUAL_USAGE);
  if (risk.error !== undefined) {
    ctx.setFeedback({ kind: "error", message: risk.error });
    return;
  }

  const isMarket = entryRaw.toLowerCase() === "market";
  const entry = isMarket ? ("market" as const) : parsePrice(entryRaw);
  if (entry === undefined) {
    ctx.setFeedback({
      kind: "error",
      message: `entrée invalide : "${entryRaw}" — ${TRADE_MANUAL_USAGE}`,
    });
    return;
  }

  const stopLoss = parsePrice(flags.sl);
  if (stopLoss === undefined) {
    ctx.setFeedback({
      kind: "error",
      message: `sl invalide ou manquant : "${flags.sl ?? ""}" — ${TRADE_MANUAL_USAGE}`,
    });
    return;
  }

  const takeProfit = parsePrice(flags.tp);
  if (takeProfit === undefined) {
    ctx.setFeedback({
      kind: "error",
      message: `tp invalide ou manquant : "${flags.tp ?? ""}" — ${TRADE_MANUAL_USAGE}`,
    });
    return;
  }

  ctx.setFeedback({ kind: "info", message: "calcul en cours…" });
  void Effect.runPromise(
    prepareTrade(ctx.client, symbolId, { entry, riskPercent: risk.value!, stopLoss, takeProfit }),
  ).then(ctx.proposeTrade, (error) =>
    ctx.setFeedback({ kind: "error", message: toMessage(error) }),
  );
}

function runAtr(symbolId: number, args: string[], ctx: CommandContext): void {
  const [sideRaw, entryRaw, ...rest] = args;
  const side = parseSide(sideRaw);
  if (!side) {
    ctx.setFeedback({
      kind: "error",
      message: `direction invalide : "${sideRaw ?? ""}" — ${TRADE_ATR_USAGE}`,
    });
    return;
  }

  if (!entryRaw) {
    ctx.setFeedback({ kind: "error", message: `entrée manquante — ${TRADE_ATR_USAGE}` });
    return;
  }

  const flags = parseFlags(rest, ATR_FLAG_ALIASES);
  if (typeof flags === "string") {
    ctx.setFeedback({ kind: "error", message: `${flags} — ${TRADE_ATR_USAGE}` });
    return;
  }

  const risk = resolveRiskPercent(flags.risk, ctx.defaultRiskPercent, TRADE_ATR_USAGE);
  if (risk.error !== undefined) {
    ctx.setFeedback({ kind: "error", message: risk.error });
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
      message: `entrée invalide : "${entryRaw}" — ${TRADE_ATR_USAGE}`,
    });
    return;
  }

  ctx.setFeedback({ kind: "info", message: "calcul ATR en cours…" });
  void Effect.runPromise(
    prepareAtrTrade(ctx.client, symbolId, {
      tradeSide: side,
      entry,
      riskPercent: risk.value!,
      rewardRiskRatio,
    }),
  ).then(ctx.proposeTrade, (error) =>
    ctx.setFeedback({ kind: "error", message: toMessage(error) }),
  );
}

export const tradeCommand: Command = {
  name: "trade",
  usage: TRADE_USAGE,
  summary:
    "prépare un ordre et demande confirmation — SL/TP manuels par défaut, Shift+Tab pour basculer en mode ATR (SL/TP auto)",
  run(args, ctx) {
    if (!ctx.symbolId) {
      ctx.setFeedback({ kind: "error", message: "pas encore connecté au serveur" });
      return;
    }

    (ctx.atrMode ? runAtr : runManual)(ctx.symbolId, args, ctx);
  },
};
