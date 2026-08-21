import type { Command } from "./types.ts";

const USAGE = "clear — efface le message de feedback";

export const clearCommand: Command = {
  name: "clear",
  usage: USAGE,
  summary: "efface le message de feedback",
  run(_args, ctx) {
    ctx.setFeedback({ kind: "info", message: "" });
  },
};
