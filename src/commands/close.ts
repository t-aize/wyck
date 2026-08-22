import type { CtraderPosition } from "../ctrader/schemas.ts";
import { parseFiniteNumber } from "./_shared.ts";
import type { Command } from "./types.ts";

export const CLOSE_USAGE = "usage : close <id>";

export const closeCommand: Command = {
  name: "close",
  usage: CLOSE_USAGE,
  summary: "clôture une position ouverte",
  run(args, ctx) {
    const id = parseFiniteNumber(args[0]);
    if (id === undefined) {
      ctx.setFeedback({
        kind: "error",
        message: `id invalide : "${args[0] ?? ""}" — ${CLOSE_USAGE}`,
      });
      return;
    }

    const position = ctx.positions?.positions.find(
      (p): p is CtraderPosition & { id: number } => p.id === id,
    );
    if (!position) {
      ctx.setFeedback({ kind: "error", message: `position ${id} introuvable` });
      return;
    }
    if (position.volumeLots === undefined) {
      ctx.setFeedback({
        kind: "error",
        message: `position ${id} : volume indisponible — mapping de données cassé, clôture refusée par sécurité`,
      });
      return;
    }

    // `volumeLots` vient d'être vérifié défini ci-dessus — TS ne propage pas cette narrowing à
    // travers l'intersection posée par le type predicate de `find` (limitation connue sur les
    // types issus de z.infer/.transform()), d'où l'assertion plutôt qu'une simple inférence.
    ctx.proposeClose(position as CtraderPosition & { id: number; volumeLots: number });
  },
};
