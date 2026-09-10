import type { AmendablePosition } from "@aurum/ctrader";
import { parseFiniteNumber, parseFlags, parseOptionalPrice } from "./_shared.ts";
import type { Command } from "./types.ts";

export const AMEND_USAGE = "usage : amend <id> [--sl <prix>] [--tp <prix>]  (raccourcis -sl -tp)";

const AMEND_FLAG_ALIASES = { sl: ["--sl", "-sl"], tp: ["--tp", "-tp"] };

/** Ex-"modify" — renommé pour matcher le vocabulaire déjà utilisé partout ailleurs dans le domaine
 * (`toAmendOrderParams`, `client.amendOrder`, cf. trading/amendParams.ts et ctrader/client.ts) : "modify"
 * était le seul endroit de l'app à parler de "modification" plutôt que d'"amend". Cible un ordre
 * en attente OU une position ouverte selon où l'id se trouve — cTrader utilise déjà "amend" pour
 * les deux (`amend_order`/`amend_position`), pas de raison d'avoir deux commandes distinctes côté
 * app pour la même intention utilisateur ("change le SL/TP de #id"). */
export const amendCommand: Command = {
  name: "amend",
  usage: AMEND_USAGE,
  summary: "modifie le SL/TP d'un ordre en attente ou d'une position ouverte",
  run(args, ctx) {
    // Aucun argument : probablement quelqu'un qui cherche l'usage plutôt qu'une vraie tentative
    // ratée — même traitement que `help amend`, pas une erreur.
    if (args.length === 0) {
      ctx.setFeedback({ kind: "info", message: AMEND_USAGE });
      return;
    }

    const id = parseFiniteNumber(args[0]);
    if (id === undefined) {
      ctx.setFeedback({
        kind: "error",
        message: `id invalide : "${args[0] ?? ""}" — ${AMEND_USAGE}`,
      });
      return;
    }

    const flags = parseFlags(args.slice(1), AMEND_FLAG_ALIASES);
    if (typeof flags === "string") {
      ctx.setFeedback({ kind: "error", message: `${flags} — ${AMEND_USAGE}` });
      return;
    }

    const order = ctx.positions?.orders.find((o) => o.orderId === id);
    const position = ctx.positions?.positions.find((p): p is AmendablePosition => p.id === id);
    if (!order && !position) {
      ctx.setFeedback({ kind: "error", message: `ordre/position ${id} introuvable` });
      return;
    }

    const targetSymbolId = order?.symbolId ?? position?.symbolId;
    const targetSpecs =
      targetSymbolId !== undefined
        ? ctx.catalog.find((item) => item.symbolId === targetSymbolId)
        : undefined;
    const digits = targetSpecs?.digits ?? ctx.instrument?.digits ?? 2;
    const sl = parseOptionalPrice(flags.sl, "sl", digits);
    if (sl.error) {
      ctx.setFeedback({ kind: "error", message: sl.error });
      return;
    }
    const tp = parseOptionalPrice(flags.tp, "tp", digits);
    if (tp.error) {
      ctx.setFeedback({ kind: "error", message: tp.error });
      return;
    }
    if (sl.value === undefined && tp.value === undefined) {
      ctx.setFeedback({ kind: "error", message: `au moins --sl ou --tp requis — ${AMEND_USAGE}` });
      return;
    }

    if (order) {
      ctx.proposeModify(order, sl.value, tp.value);
      return;
    }
    ctx.proposePositionAmend(position!, sl.value, tp.value);
  },
};
