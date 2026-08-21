import { parseFiniteNumber } from "./_shared.ts";
import type { Command } from "./types.ts";

export const RISK_USAGE =
  "usage : risk <risque%>  (risque par défaut pour `trade`, valable cette session)";

export const riskCommand: Command = {
  name: "risk",
  usage: RISK_USAGE,
  summary: "affiche, ou règle, le risque% par défaut de la session",
  run(args, ctx) {
    if (args.length === 0) {
      ctx.setFeedback({
        kind: "info",
        message:
          ctx.defaultRiskPercent === undefined
            ? `aucun risque par défaut — ${RISK_USAGE}`
            : `risque par défaut : ${ctx.defaultRiskPercent}%`,
      });
      return;
    }

    const value = parseFiniteNumber(args[0]);
    if (value === undefined || value <= 0 || value > 100) {
      ctx.setFeedback({
        kind: "error",
        message: `risque invalide : "${args[0] ?? ""}" — ${RISK_USAGE}`,
      });
      return;
    }

    ctx.setDefaultRiskPercent(value);
    ctx.setFeedback({
      kind: "success",
      message: `risque par défaut réglé à ${value}% pour cette session`,
    });
  },
};
