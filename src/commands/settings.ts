import { DEFAULT_MCP_URL } from "../constants.ts";
import { TRENDBAR_PERIODS, type TrendbarPeriod } from "../ctrader/protocol/TrendbarPeriod.ts";
import { parseFiniteNumber } from "./_shared.ts";
import type { Command } from "./types.ts";

function isTrendbarPeriod(value: string): value is TrendbarPeriod {
  return (TRENDBAR_PERIODS as readonly string[]).includes(value);
}

export const SETTINGS_USAGE =
  "usage : settings — liste les réglages et leur valeur actuelle\n" +
  "  settings url [<url>] — sans argument, propose l'url MCP par défaut ; avec, l'enregistre et relance la connexion\n" +
  "  settings token [<token>] — sans argument, indique si un token est déjà défini (jamais sa valeur) ; avec, l'enregistre et relance la connexion\n" +
  "  settings atrrefresh [on|off] — consulte/active/désactive le rafraîchissement auto du SL/TP des ordres ATR en attente (toutes les 60s)\n" +
  "  settings atrperiod [<n>] — consulte/règle la période de l'ATR (défaut 14)\n" +
  `  settings atrtimeframe [<tf>] — consulte/règle le timeframe de l'ATR (défaut M_5) — ${TRENDBAR_PERIODS.join("|")}\n` +
  "  settings symbol [<nom>] — consulte/règle le symbole tradé (liste cTrader, ex. XAUUSD, US100, BTCUSD)";

/** Commande générique à un seul niveau (`settings <clé> [valeur]`) plutôt qu'une commande dédiée
 * par réglage — un seul point d'entrée pour tout futur réglage, pas de redesign nécessaire pour en
 * ajouter un (juste un `case` de plus). `url`/`token` remplacent l'ancien SetupScreen.tsx : plus
 * d'assistant séparé, on tape les identifiants ici et l'app se reconnecte en tâche de fond
 * (cf. App.tsx#reloadConfig). */
export const settingsCommand: Command = {
  name: "settings",
  usage: SETTINGS_USAGE,
  summary: "consulte ou modifie les réglages (MCP, ATR, symbole)",
  run(args, ctx) {
    const [key, value] = args;

    if (!key) {
      ctx.setFeedback({
        kind: "info",
        message:
          `réglages : url (${ctx.hasMcpUrl ? "défini" : "non défini"}), ` +
          `token (${ctx.hasMcpToken ? "défini" : "non défini"}), ` +
          `atrrefresh (${ctx.atrRefreshEnabled ? "on" : "off"}), ` +
          `atrperiod (${ctx.atrPeriod}), ` +
          `atrtimeframe (${ctx.atrTimeframe}), ` +
          `symbol (${ctx.instrument?.symbolName ?? "—"})`,
      });
      return;
    }

    switch (key.toLowerCase()) {
      case "url": {
        if (!value) {
          ctx.setFeedback({
            kind: "info",
            message: `url MCP par défaut : ${DEFAULT_MCP_URL} — settings url ${DEFAULT_MCP_URL} pour l'enregistrer`,
          });
          return;
        }
        ctx.setMcpUrl(value);
        ctx.setFeedback({ kind: "success", message: "url MCP enregistrée — reconnexion…" });
        return;
      }

      case "token": {
        if (!value) {
          ctx.setFeedback({
            kind: "info",
            message: `token MCP : ${ctx.hasMcpToken ? "défini" : "non défini"} — settings token <token> pour le régler`,
          });
          return;
        }
        ctx.setMcpToken(value);
        ctx.setFeedback({ kind: "success", message: "token MCP enregistré — reconnexion…" });
        return;
      }

      case "atrrefresh": {
        if (!value) {
          ctx.setFeedback({
            kind: "info",
            message: `rafraîchissement auto ATR : ${ctx.atrRefreshEnabled ? "activé" : "désactivé"}`,
          });
          return;
        }
        const lower = value.toLowerCase();
        if (lower !== "on" && lower !== "off") {
          ctx.setFeedback({
            kind: "error",
            message: `valeur invalide : "${value}" — attendu on|off`,
          });
          return;
        }
        const next = lower === "on";
        ctx.setAtrRefreshEnabled(next);
        ctx.setFeedback({
          kind: "info",
          message: `rafraîchissement auto ATR ${next ? "activé" : "désactivé"}`,
        });
        return;
      }

      case "atrperiod": {
        if (!value) {
          ctx.setFeedback({ kind: "info", message: `période ATR : ${ctx.atrPeriod}` });
          return;
        }
        const period = parseFiniteNumber(value);
        if (period === undefined || !Number.isInteger(period) || period <= 0) {
          ctx.setFeedback({
            kind: "error",
            message: `valeur invalide : "${value}" — attendu un entier positif`,
          });
          return;
        }
        ctx.setAtrPeriod(period);
        ctx.setFeedback({ kind: "success", message: `période ATR réglée sur ${period}` });
        return;
      }

      case "atrtimeframe": {
        if (!value) {
          ctx.setFeedback({ kind: "info", message: `timeframe ATR : ${ctx.atrTimeframe}` });
          return;
        }
        const upper = value.toUpperCase();
        if (!isTrendbarPeriod(upper)) {
          ctx.setFeedback({
            kind: "error",
            message: `valeur invalide : "${value}" — attendu ${TRENDBAR_PERIODS.join("|")}`,
          });
          return;
        }
        ctx.setAtrTimeframe(upper);
        ctx.setFeedback({ kind: "success", message: `timeframe ATR réglé sur ${upper}` });
        return;
      }

      case "symbol": {
        if (!value) {
          ctx.setFeedback({
            kind: "info",
            message:
              `symbole : ${ctx.instrument?.symbolName ?? "—"} — settings symbol <nom> ` +
              "ou clic sur le symbole dans le header. " +
              `${ctx.catalog.length} symboles disponibles côté cTrader.`,
          });
          return;
        }
        if (ctx.selectSymbol(value)) {
          const resolved =
            ctx.catalog.find((item) => item.symbolName.toUpperCase() === value.toUpperCase()) ??
            ctx.catalog.find((item) =>
              item.symbolName.toUpperCase().startsWith(value.toUpperCase()),
            );
          ctx.setFeedback({
            kind: "success",
            message: `symbole réglé sur ${resolved?.symbolName ?? value}`,
          });
          return;
        }
        const matches = ctx.catalog
          .filter((item) => item.symbolName.toUpperCase().includes(value.toUpperCase()))
          .slice(0, 8)
          .map((item) => item.symbolName);
        ctx.setFeedback({
          kind: "error",
          message:
            matches.length > 0
              ? `symbole introuvable : "${value}" — proches : ${matches.join(", ")}`
              : `symbole introuvable : "${value}" — ${ctx.catalog.length} symboles côté serveur`,
        });
        return;
      }

      default:
        ctx.setFeedback({
          kind: "error",
          message: `réglage inconnu : "${key}" — ${SETTINGS_USAGE}`,
        });
    }
  },
};
