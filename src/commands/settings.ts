import { DEFAULT_MCP_URL } from "../constants.ts";
import type { Command } from "./types.ts";

export const SETTINGS_USAGE =
  "usage : settings — liste les réglages et leur valeur actuelle\n" +
  "  settings url [<url>] — sans argument, propose l'url MCP par défaut ; avec, l'enregistre et relance la connexion\n" +
  "  settings token [<token>] — sans argument, indique si un token est déjà défini (jamais sa valeur) ; avec, l'enregistre et relance la connexion\n" +
  "  settings atrrefresh [on|off] — consulte/active/désactive le rafraîchissement auto du SL/TP des ordres ATR en attente (toutes les 60s)";

/** Commande générique à un seul niveau (`settings <clé> [valeur]`) plutôt qu'une commande dédiée
 * par réglage — un seul point d'entrée pour tout futur réglage, pas de redesign nécessaire pour en
 * ajouter un (juste un `case` de plus). `url`/`token` remplacent l'ancien SetupScreen.tsx : plus
 * d'assistant séparé, on tape les identifiants ici et l'app se reconnecte en tâche de fond
 * (cf. App.tsx#reloadConfig). */
export const settingsCommand: Command = {
  name: "settings",
  usage: SETTINGS_USAGE,
  summary: "consulte ou modifie les réglages de l'app (connexion MCP, rafraîchissement auto ATR)",
  run(args, ctx) {
    const [key, value] = args;

    if (!key) {
      ctx.setFeedback({
        kind: "info",
        message:
          `réglages : url (${ctx.hasMcpUrl ? "défini" : "non défini"}), ` +
          `token (${ctx.hasMcpToken ? "défini" : "non défini"}), ` +
          `atrrefresh (${ctx.atrRefreshEnabled ? "on" : "off"})`,
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

      default:
        ctx.setFeedback({
          kind: "error",
          message: `réglage inconnu : "${key}" — ${SETTINGS_USAGE}`,
        });
    }
  },
};
