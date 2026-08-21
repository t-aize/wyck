import type { Command } from "./types.ts";

const USAGE = "refresh — force une actualisation immédiate du marché et du calendrier";

export const refreshCommand: Command = {
  name: "refresh",
  usage: USAGE,
  summary: "force une actualisation immédiate du marché et du calendrier",
  run(_args, ctx) {
    ctx.setFeedback({ kind: "info", message: "actualisation…" });
    void Promise.all([ctx.refreshMarket(), ctx.refreshNews({ force: true })]).then(() => {
      ctx.setFeedback({ kind: "success", message: "actualisé" });
    });
  },
};
