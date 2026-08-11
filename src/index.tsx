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

  // opentui redimensionne automatiquement le rendu via SIGWINCH (cf. sa propre doc sur
  // CliRenderer#resize) — mais SIGWINCH n'existe pas sous Windows, donc sans ce relais le rendu
  // ne suit jamais un redimensionnement de la fenêtre du terminal sur cette plateforme.
  // `stdout.on("resize", ...)` est l'équivalent Node/Bun multi-plateforme (fonctionne aussi là où
  // SIGWINCH est déjà géré — relais inoffensif en double, `resize()` est idempotent sur des
  // dimensions inchangées).
  process.stdout.on("resize", () => {
    if (process.stdout.columns && process.stdout.rows) {
      renderer.resize(process.stdout.columns, process.stdout.rows);
    }
  });

  createRoot(renderer).render(<App />);
}
