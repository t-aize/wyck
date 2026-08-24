/** Source unique de vérité pour "quelles commandes existent" — remplace les deux registres
 * dupliqués de l'ancien code (`*_USAGE` dans domain/commands.ts d'un côté, `COMMAND_LIST`/
 * `COMMAND_HELP` dans useCommandRouter.ts de l'autre). Consommé par useCommandRouter.ts (dispatch),
 * CommandBar.tsx (autocomplétion) et help.ts (listing), donc une commande ajoutée ici apparaît
 * automatiquement aux trois endroits. */

import { amendCommand } from "./amend.ts";
import { cancelCommand } from "./cancel.ts";
import { clearCommand } from "./clear.ts";
import { closeCommand } from "./close.ts";
import { configCommand } from "./config.ts";
import { helpCommand } from "./help.ts";
import { refreshCommand } from "./refresh.ts";
import { settingsCommand } from "./settings.ts";
import { tradeCommand } from "./trade.ts";
import type { Command } from "./types.ts";

/** Ordre d'affichage dans `help` et l'autocomplétion du CommandBar. */
export const COMMANDS: Command[] = [
  tradeCommand,
  amendCommand,
  cancelCommand,
  closeCommand,
  settingsCommand,
  configCommand,
  refreshCommand,
  clearCommand,
  helpCommand,
];

const byName = new Map(COMMANDS.map((command) => [command.name, command]));

export function findCommand(name: string): Command | undefined {
  return byName.get(name);
}
