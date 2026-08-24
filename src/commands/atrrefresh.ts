import { Effect } from "effect";
import { atrLevels, fetchAtr } from "../trading/atr.ts";
import { toMessage } from "../utils/errors.ts";
import { parseFiniteNumber, parseFlags } from "./_shared.ts";
import type { Command } from "./types.ts";

export const ATRREFRESH_USAGE =
  "usage : atrrefresh <id> [--rr <ratio>]  " +
  "(recalcule le SL/TP d'un ordre en attente avec l'ATR(14) M5 courant, RR par défaut 1.2)";

const DEFAULT_REWARD_RISK_RATIO = 1.2;

const ATRREFRESH_FLAG_ALIASES = { rr: ["--rr", "-rr"] };

/**
 * Volontairement manuel et ciblé par id explicite plutôt qu'un refresh automatique/en masse — cf.
 * discussion : un re-tracking silencieux casserait la philosophie de confirmation de toute l'app
 * (`amend`/`cancel`/`close` passent tous par une modale), et il n'y a de toute façon aucun moyen
 * fiable de savoir quels ordres viennent de `trade` en mode ATR (le `label` d'un ordre n'est pas exposé par
 * `CtraderOrderSchema` — jamais vérifié en pratique, cf. son commentaire de tête).
 *
 * Ordres en attente seulement (jamais une position déjà ouverte) : un ordre pending n'a pas encore
 * déclenché, donc "à quel niveau je le placerais maintenant" a une réponse claire. Une position
 * ouverte a un entry et un risque déjà figés sur la distance de stop d'origine — "rafraîchir" son SL
 * redevient ambigu (SL qui suit le marché = trailing stop = le re-tracking automatique écarté plus
 * haut, ou figé une fois pour toutes = alors pourquoi le rafraîchir).
 *
 * Réutilise `ctx.proposeModify` (le même chemin que `amend`) : aucune nouvelle modale, le nouveau
 * SL/TP passe par la confirmation existante avant tout envoi réseau.
 */
export const atrRefreshCommand: Command = {
  name: "atrrefresh",
  usage: ATRREFRESH_USAGE,
  summary: "recalcule le SL/TP d'un ordre en attente avec l'ATR(14) M5 courant",
  run(args, ctx) {
    if (!ctx.symbolId) {
      ctx.setFeedback({ kind: "error", message: "pas encore connecté au serveur" });
      return;
    }

    const id = parseFiniteNumber(args[0]);
    if (id === undefined) {
      ctx.setFeedback({
        kind: "error",
        message: `id invalide : "${args[0] ?? ""}" — ${ATRREFRESH_USAGE}`,
      });
      return;
    }

    const flags = parseFlags(args.slice(1), ATRREFRESH_FLAG_ALIASES);
    if (typeof flags === "string") {
      ctx.setFeedback({ kind: "error", message: `${flags} — ${ATRREFRESH_USAGE}` });
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

    const order = ctx.positions?.orders.find((o) => o.orderId === id);
    if (!order) {
      ctx.setFeedback({ kind: "error", message: `ordre en attente ${id} introuvable` });
      return;
    }

    const entryPrice = order.limitPrice ?? order.stopPrice;
    if (entryPrice === undefined) {
      ctx.setFeedback({
        kind: "error",
        message: `ordre ${id} : prix d'entrée indisponible — mapping de données cassé`,
      });
      return;
    }

    ctx.setFeedback({ kind: "info", message: "calcul ATR en cours…" });
    void Effect.runPromise(fetchAtr(ctx.client, ctx.symbolId)).then(
      (atr) => {
        const { stopLoss, takeProfit } = atrLevels(
          entryPrice,
          order.tradeSide,
          atr,
          rewardRiskRatio,
        );
        ctx.proposeModify(order, stopLoss, takeProfit);
      },
      (error) => ctx.setFeedback({ kind: "error", message: toMessage(error) }),
    );
  },
};
