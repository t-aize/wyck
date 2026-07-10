import { createCliRenderer } from "@opentui/core";
import { createRoot } from "@opentui/react";
import { App } from "./App.tsx";
import { theme } from "./ui/theme.ts";

if (import.meta.main) {
  const renderer = await createCliRenderer({
    backgroundColor: theme.bg,
    // Ctrl+C est géré nous-mêmes (double appui, cf. useTerminalShortcuts). `exitOnCtrlC: false`
    // ne suffit pas seul : SIGINT (déclenché par Ctrl+C selon le terminal) a son propre chemin
    // de sortie via `exitSignals`, indépendant — vérifié en pratique, sans ce retrait le premier
    // Ctrl+C ferme quand même l'appli en coupant le parsing clavier avant le second appui.
    exitOnCtrlC: false,
    exitSignals: ["SIGTERM", "SIGQUIT", "SIGABRT", "SIGHUP", "SIGBREAK", "SIGPIPE", "SIGBUS"],
  });
  createRoot(renderer).render(<App />);
}
