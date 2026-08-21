import type { CtraderOrder } from "../ctrader/schemas.ts";
import { parseFiniteNumber } from "./_shared.ts";
import type { Command } from "./types.ts";

export const CANCEL_USAGE = "usage : cancel <id> [id...] | cancel all";

export const cancelCommand: Command = {
  name: "cancel",
  usage: CANCEL_USAGE,
  summary: "annule un ou plusieurs ordres en attente (ou tous, avec `all`)",
  run(args, ctx) {
    const pendingOrders = ctx.positions?.orders ?? [];

    if (args.length === 0) {
      ctx.setFeedback({ kind: "error", message: CANCEL_USAGE });
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
        ctx.setFeedback({ kind: "error", message: `id invalide : "${raw}"` });
        return;
      }
      ids.push(id);
    }

    // Même limite que `amend` : seuls les ordres en attente (structure vérifiée) sont annulables
    // pour l'instant, pas les positions ouvertes (closePosition non exercé).
    const orders: CtraderOrder[] = [];
    for (const id of ids) {
      const order = pendingOrders.find((o) => o.orderId === id);
      if (!order) {
        ctx.setFeedback({ kind: "error", message: `ordre en attente ${id} introuvable` });
        return;
      }
      orders.push(order);
    }
    ctx.proposeCancel(orders);
  },
};
