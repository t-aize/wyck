/**
 * Préchargé par `bun test` (cf. bunfig.toml) avant tout fichier de test : enregistre un DOM
 * happy-dom global (document/window/…) une seule fois pour toute la suite — requis par
 * @testing-library/react (usePendingAction.test.tsx), qui rend via react-dom et a donc besoin d'un
 * DOM réel, contrairement au reste de l'app qui rend via le renderer terminal d'@opentui/react.
 */

import { GlobalRegistrator } from "@happy-dom/global-registrator";

GlobalRegistrator.register();
