import type { Command } from "./types.ts";

const USAGE = "config — reconfigure l'URL/le token MCP";

/** Ex-"settings" — renommé vers un terme CLI plus standard. */
export const configCommand: Command = {
  name: "config",
  usage: USAGE,
  summary: "reconfigure l'URL/le token MCP",
  run(_args, ctx) {
    ctx.onReconfigure();
  },
};
