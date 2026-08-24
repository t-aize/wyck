import type { AmendablePosition, ClosablePosition, CtraderOrder } from "../ctrader/schemas.ts";
import { parseFiniteNumber } from "./_shared.ts";
import type { Command } from "./types.ts";

export const CLOSE_USAGE =
  "usage : close <id> [id...] | close all  " +
  "(id cherché à la fois parmi les positions ouvertes et les ordres en attente ; " +
  "`all` annule tous les ordres en attente, pas les positions ; une seule position à la fois)";

/**
 * Fusion de l'ancienne commande `cancel` (ordres en attente) dans `close` (positions ouvertes) :
 * un id est cherché dans les deux, sans que l'utilisateur ait à savoir de quel côté il se trouve.
 * `all` garde le sens qu'avait `cancel all` (tous les ordres en attente) plutôt que d'englober
 * aussi les positions — `proposeClose` (cf. useCloseConfirm.ts) ne gère qu'une position à la fois,
 * aucune modale de clôture par lot n'existe, et clôturer toutes les positions ouvertes d'un coup
 * mériterait de toute façon une confirmation bien plus explicite qu'un mot-clé générique.
 */
export const closeCommand: Command = {
  name: "close",
  usage: CLOSE_USAGE,
  summary:
    "clôture une position ouverte ou annule un/plusieurs ordres en attente (ou tous, avec `all`)",
  run(args, ctx) {
    const openPositions = ctx.positions?.positions ?? [];
    const pendingOrders = ctx.positions?.orders ?? [];

    if (args.length === 0) {
      ctx.setFeedback({ kind: "error", message: CLOSE_USAGE });
      return;
    }

    if (args[0]?.toLowerCase() === "all") {
      if (pendingOrders.length === 0) {
        ctx.setFeedback({ kind: "info", message: "aucun ordre en attente à annuler" });
        return;
      }
      ctx.proposeCancel(pendingOrders);
      return;
    }

    const ids: number[] = [];
    for (const raw of args) {
      const id = parseFiniteNumber(raw);
      if (id === undefined) {
        ctx.setFeedback({ kind: "error", message: `id invalide : "${raw}" — ${CLOSE_USAGE}` });
        return;
      }
      ids.push(id);
    }

    const positions: AmendablePosition[] = [];
    const orders: CtraderOrder[] = [];
    for (const id of ids) {
      const position = openPositions.find((p): p is AmendablePosition => p.id === id);
      if (position) {
        positions.push(position);
        continue;
      }
      const order = pendingOrders.find((o) => o.orderId === id);
      if (order) {
        orders.push(order);
        continue;
      }
      ctx.setFeedback({
        kind: "error",
        message: `id ${id} introuvable (ni position ouverte, ni ordre en attente)`,
      });
      return;
    }

    // Un seul flux de confirmation à la fois : `proposeClose` ne gère qu'une position (pas de
    // modale de clôture par lot), `proposeCancel` gère déjà un lot d'ordres. Un mélange des deux
    // ou plusieurs positions à la fois n'a donc pas de destination unique — on demande de séparer
    // les appels plutôt que de deviner lequel prioriser.
    if (positions.length > 0 && orders.length > 0) {
      ctx.setFeedback({
        kind: "error",
        message:
          "impossible de mélanger positions et ordres en attente dans le même appel — sépare les deux",
      });
      return;
    }

    if (positions.length > 1) {
      ctx.setFeedback({
        kind: "error",
        message: "une seule position à la fois — clôture-les une par une",
      });
      return;
    }

    if (positions.length === 1) {
      const position = positions[0]!;
      if (position.volumeLots === undefined) {
        ctx.setFeedback({
          kind: "error",
          message: `position ${position.id} : volume indisponible — mapping de données cassé, clôture refusée par sécurité`,
        });
        return;
      }
      // `volumeLots` vient d'être vérifié défini ci-dessus — TS ne propage pas cette narrowing à
      // travers l'intersection posée par le type predicate de `find` (limitation connue sur les
      // types issus de z.infer/.transform()), d'où l'assertion plutôt qu'une simple inférence.
      ctx.proposeClose(position as ClosablePosition);
      return;
    }

    ctx.proposeCancel(orders);
  },
};
