import type { Command } from "./types.ts";

const USAGE = "help [commande] — liste les commandes, ou détaille l'usage d'une commande précise";

export const helpCommand: Command = {
  name: "help",
  usage: USAGE,
  summary: "liste les commandes, ou détaille l'usage d'une commande précise",
  run(args, ctx) {
    const names = ctx.commands.map((c) => c.name).join("  ");
    const target = args[0]?.toLowerCase();
    if (!target) {
      ctx.setFeedback({ kind: "info", message: `commandes : ${names}` });
      return;
    }

    const command = ctx.commands.find((c) => c.name === target);
    ctx.setFeedback(
      command
        ? { kind: "info", message: command.usage }
        : { kind: "error", message: `commande inconnue : ${target} — ${names}` },
    );
  },
};
