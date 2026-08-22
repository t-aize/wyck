import type { CtraderPosition } from "../ctrader/schemas.ts";
import { parseFiniteNumber, parseFlags, parseOptionalPrice } from "./_shared.ts";
import type { Command } from "./types.ts";

export const AMEND_USAGE = "usage : amend <id> [--sl <prix>] [--tp <prix>]  (raccourcis -sl -tp)";

const AMEND_FLAG_ALIASES = { sl: ["--sl", "-sl"], tp: ["--tp", "-tp"] };

/** Ex-"modify" — renommé pour matcher le vocabulaire déjà utilisé partout ailleurs dans le domaine
 * (`toAmendOrderParams`, `client.amendOrder`, cf. domain/trading.ts et ctrader/client.ts) : "modify"
 * était le seul endroit de l'app à parler de "modification" plutôt que d'"amend". Cible un ordre
 * en attente OU une position ouverte selon où l'id se trouve — cTrader utilise déjà "amend" pour
 * les deux (`amend_order`/`amend_position`), pas de raison d'avoir deux commandes distinctes côté
 * app pour la même intention utilisateur ("change le SL/TP de #id"). */
export const amendCommand: Command = {
  name: "amend",
  usage: AMEND_USAGE,
  summary: "modifie le SL/TP d'un ordre en attente ou d'une position ouverte",
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

    const order = ctx.positions?.orders.find((o) => o.orderId === id);
    if (order) {
      ctx.proposeModify(order, sl.value, tp.value);
      return;
    }

    const position = ctx.positions?.positions.find(
      (p): p is CtraderPosition & { id: number } => p.id === id,
    );
    if (position) {
      ctx.proposePositionAmend(position, sl.value, tp.value);
      return;
    }

    ctx.setFeedback({ kind: "error", message: `ordre/position ${id} introuvable` });
  },
};
