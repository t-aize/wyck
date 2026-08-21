import { parseFiniteNumber, parseFlags, parseOptionalPrice } from "./_shared.ts";
import type { Command } from "./types.ts";

export const AMEND_USAGE = "usage : amend <id> [--sl <prix>] [--tp <prix>]  (raccourcis -sl -tp)";

const AMEND_FLAG_ALIASES = { sl: ["--sl", "-sl"], tp: ["--tp", "-tp"] };

/** Ex-"modify" — renommé pour matcher le vocabulaire déjà utilisé partout ailleurs dans le domaine
 * (`toAmendOrderParams`, `client.amendOrder`, cf. domain/trading.ts et ctrader/client.ts) : "modify"
 * était le seul endroit de l'app à parler de "modification" plutôt que d'"amend". */
export const amendCommand: Command = {
  name: "amend",
  usage: AMEND_USAGE,
  summary: "modifie le SL/TP d'un ordre en attente",
  run(args, ctx) {
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

    const sl = parseOptionalPrice(flags.sl, "sl");
    if (sl.error) {
      ctx.setFeedback({ kind: "error", message: sl.error });
      return;
    }
    const tp = parseOptionalPrice(flags.tp, "tp");
    if (tp.error) {
      ctx.setFeedback({ kind: "error", message: tp.error });
      return;
    }
    if (sl.value === undefined && tp.value === undefined) {
      ctx.setFeedback({ kind: "error", message: `au moins --sl ou --tp requis — ${AMEND_USAGE}` });
      return;
    }

    // `amend` ne cible que les ordres en attente pour l'instant, pas les positions ouvertes — cf.
    // `CtraderPositionSchema` dans ctrader/schemas.ts pour la forme désormais confirmée.
    const order = ctx.positions?.orders.find((o) => o.orderId === id);
    if (!order) {
      ctx.setFeedback({ kind: "error", message: `ordre en attente ${id} introuvable` });
      return;
    }

    ctx.proposeModify(order, sl.value, tp.value);
  },
};
