import type { Command } from "./types.ts";

export const SETTINGS_USAGE =
  "usage : settings — liste les réglages et leur valeur actuelle  " +
  "| settings atrrefresh [on|off] — consulte/active/désactive le rafraîchissement auto du SL/TP " +
  "des ordres ATR en attente (toutes les 60s)";

/** Commande générique à un seul niveau (`settings <clé> [valeur]`) plutôt qu'une commande dédiée
 * par réglage — un seul point d'entrée pour tout futur réglage, pas de redesign nécessaire pour en
 * ajouter un (juste une branche de plus sur `key`). Aujourd'hui : uniquement `atrrefresh`. */
export const settingsCommand: Command = {
  name: "settings",
  usage: SETTINGS_USAGE,
  summary: "consulte ou modifie les réglages de l'app (ex : rafraîchissement auto ATR)",
  run(args, ctx) {
    const [key, value] = args;

    if (!key) {
      ctx.setFeedback({
        kind: "info",
        message: `réglages : atrrefresh (${ctx.atrRefreshEnabled ? "on" : "off"})`,
      });
      return;
    }

    if (key.toLowerCase() !== "atrrefresh") {
      ctx.setFeedback({ kind: "error", message: `réglage inconnu : "${key}" — ${SETTINGS_USAGE}` });
      return;
    }

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
  },
};
