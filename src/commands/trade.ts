import type { TradeSide } from "@aurum/ctrader";
import { Effect } from "effect";
import { ATR_PERIOD, ATR_TIMEFRAME } from "../trading/atr.ts";
import { prepareTrade } from "../trading/prepare.ts";
import { prepareAtrTrade } from "../trading/prepareAtr.ts";
import { toMessage } from "../utils/errors.ts";
import { parseFiniteNumber, parsePrice } from "./_shared.ts";
import type { Command, CommandContext } from "./types.ts";

const TRADE_MANUAL_USAGE = "usage : trade <entrée|market> <sl> <tp> <risk%>";
/** Période/timeframe réglables via `settings atrperiod`/`settings atrtimeframe` (cf.
 * commands/settings.ts) — usage recalculé à chaque appel plutôt que figé en constante, pour refléter
 * le réglage courant. */
function tradeAtrUsage(ctx: Pick<CommandContext, "atrPeriod" | "atrTimeframe">): string {
  return (
    "usage (mode ATR, Shift+Tab) : trade <BUY|SELL|buy|sell> <entrée|market> <rr> <risk%>  " +
    `(SL = ATR(${ctx.atrPeriod}) ${ctx.atrTimeframe}, TP = SL × rr)`
  );
}
/** Affiché par `help trade` uniquement — les messages d'erreur inline citent chacun leur propre
 * usage (`TRADE_MANUAL_USAGE`/`tradeAtrUsage`) plutôt que cette version combinée. Défauts
 * ATR_PERIOD/ATR_TIMEFRAME ici : `help` n'a pas de `CommandContext` sous la main (cf. help.ts). */
export const TRADE_USAGE = `${TRADE_MANUAL_USAGE}\n${tradeAtrUsage({ atrPeriod: ATR_PERIOD, atrTimeframe: ATR_TIMEFRAME })}`;

/** Direction explicite en argument plutôt que déduite (cf. commande `trade` en mode manuel) : l'ATR
 * donne une distance, pas un sens — et c'est justement l'ancien sous-système qui inférait un biais
 * (SMC/structure) qui a été supprimé (76452d9), pas de raison de le faire renaître ici.
 * `.toUpperCase()` avant comparaison : "buy"/"Buy"/"BUY" sont tous acceptés. */
function parseSide(raw: string | undefined): TradeSide | undefined {
  const upper = raw?.toUpperCase();
  return upper === "BUY" || upper === "SELL" ? upper : undefined;
}

function parseRiskPercent(raw: string | undefined): number | undefined {
  const value = parseFiniteNumber(raw);
  if (value === undefined || value <= 0 || value > 100) return undefined;
  return value;
}

/**
 * Arguments positionnels stricts plutôt que des flags optionnels — l'utilisateur veut que rien ne
 * soit implicite : SL/TP/risque (ou RR/risque en mode ATR) sont toujours les 4 mots qui suivent la
 * commande, dans cet ordre, jamais de valeur par défaut à deviner.
 */
function runManual(symbolId: number, args: string[], ctx: CommandContext): void {
  const [entryRaw, slRaw, tpRaw, riskRaw] = args;
  if (!entryRaw || !slRaw || !tpRaw || !riskRaw) {
    ctx.setFeedback({ kind: "error", message: `arguments manquants — ${TRADE_MANUAL_USAGE}` });
    return;
  }

  const digits = ctx.instrument?.digits ?? 2;
  const isMarket = entryRaw.toLowerCase() === "market";
  const entry = isMarket ? ("market" as const) : parsePrice(entryRaw, digits);
  if (entry === undefined) {
    ctx.setFeedback({
      kind: "error",
      message: `entrée invalide : "${entryRaw}" — ${TRADE_MANUAL_USAGE}`,
    });
    return;
  }

  const stopLoss = parsePrice(slRaw, digits);
  if (stopLoss === undefined) {
    ctx.setFeedback({ kind: "error", message: `sl invalide : "${slRaw}" — ${TRADE_MANUAL_USAGE}` });
    return;
  }

  const takeProfit = parsePrice(tpRaw, digits);
  if (takeProfit === undefined) {
    ctx.setFeedback({ kind: "error", message: `tp invalide : "${tpRaw}" — ${TRADE_MANUAL_USAGE}` });
    return;
  }

  const riskPercent = parseRiskPercent(riskRaw);
  if (riskPercent === undefined) {
    ctx.setFeedback({
      kind: "error",
      message: `risque invalide : "${riskRaw}" — doit être entre 0 et 100`,
    });
    return;
  }

  ctx.setFeedback({ kind: "info", message: "calcul en cours…" });
  void Effect.runPromise(
    prepareTrade(
      ctx.client,
      symbolId,
      { entry, riskPercent, stopLoss, takeProfit },
      ctx.instrument?.lotSize ?? 100,
    ),
  ).then(ctx.proposeTrade, (error) =>
    ctx.setFeedback({ kind: "error", message: toMessage(error) }),
  );
}

function runAtr(symbolId: number, args: string[], ctx: CommandContext): void {
  const usage = tradeAtrUsage(ctx);
  const [sideRaw, entryRaw, rrRaw, riskRaw] = args;
  const side = parseSide(sideRaw);
  if (!side) {
    ctx.setFeedback({
      kind: "error",
      message: `direction invalide : "${sideRaw ?? ""}" — ${usage}`,
    });
    return;
  }

  if (!entryRaw || !rrRaw || !riskRaw) {
    ctx.setFeedback({ kind: "error", message: `arguments manquants — ${usage}` });
    return;
  }

  const digits = ctx.instrument?.digits ?? 2;
  const isMarket = entryRaw.toLowerCase() === "market";
  const entry = isMarket ? ("market" as const) : parsePrice(entryRaw, digits);
  if (entry === undefined) {
    ctx.setFeedback({
      kind: "error",
      message: `entrée invalide : "${entryRaw}" — ${usage}`,
    });
    return;
  }

  const rewardRiskRatio = parseFiniteNumber(rrRaw);
  if (rewardRiskRatio === undefined || rewardRiskRatio <= 0) {
    ctx.setFeedback({
      kind: "error",
      message: `rr invalide : "${rrRaw}" — doit être un nombre positif`,
    });
    return;
  }

  const riskPercent = parseRiskPercent(riskRaw);
  if (riskPercent === undefined) {
    ctx.setFeedback({
      kind: "error",
      message: `risque invalide : "${riskRaw}" — doit être entre 0 et 100`,
    });
    return;
  }

  ctx.setFeedback({ kind: "info", message: "calcul ATR en cours…" });
  void Effect.runPromise(
    prepareAtrTrade(
      ctx.client,
      symbolId,
      {
        tradeSide: side,
        entry,
        riskPercent,
        rewardRiskRatio,
        atrPeriod: ctx.atrPeriod,
        atrTimeframe: ctx.atrTimeframe,
      },
      { lotSize: ctx.instrument?.lotSize ?? 100, digits: ctx.instrument?.digits ?? 2 },
    ),
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
    // Aucun argument : probablement quelqu'un qui cherche l'usage plutôt qu'une vraie tentative
    // ratée — même traitement que `help trade`, pas une erreur. Avant la garde de connexion : voir
    // l'usage ne nécessite pas d'être connecté.
    if (args.length === 0) {
      ctx.setFeedback({
        kind: "info",
        message: ctx.atrMode ? tradeAtrUsage(ctx) : TRADE_MANUAL_USAGE,
      });
      return;
    }

    if (!ctx.symbolId) {
      ctx.setFeedback({ kind: "error", message: "pas encore connecté au serveur" });
      return;
    }

    (ctx.atrMode ? runAtr : runManual)(ctx.symbolId, args, ctx);
  },
};
