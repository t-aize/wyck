/** Contrat commun à toutes les commandes du CommandBar (`src/commands/*.ts`). */

import type { TrendbarPeriod } from "../constants.ts";
import type { CtraderClient } from "../ctrader/client.ts";
import type {
  AmendablePosition,
  ClosablePosition,
  CtraderOrder,
  GetPositionsResult,
} from "../ctrader/schemas.ts";
import type { PreparedTrade } from "../trading/types.ts";
import type { Feedback } from "../ui/feedback.ts";

/**
 * Dépendances qu'une commande peut lire ou déclencher — jamais les hooks React de confirmation
 * eux-mêmes (`useTradeConfirm`/`useModifyConfirm`/`useCancelConfirm`), seulement leur action
 * `propose*` à l'arité déjà réduite. Ça garde `src/commands/*.ts` testable sans monter de composant
 * React, et surtout garde la dépendance à sens unique : `ui/hooks/useCommandRouter.ts` dépend de
 * `commands/`, jamais l'inverse.
 */
export interface CommandContext {
  client: CtraderClient;
  symbolId: number | undefined;
  positions: GetPositionsResult | undefined;
  /** Basculé par Shift+Tab (cf. `useTerminalShortcuts.ts`), lu ici en lecture seule par `trade`. */
  atrMode: boolean;
  /** Réglé via `settings atrrefresh on|off` (cf. commands/settings.ts), persisté dans settings.ts —
   * lu par `useAtrAutoRefresh.ts`, pas par une commande. */
  atrRefreshEnabled: boolean;
  setAtrRefreshEnabled: (enabled: boolean) => void;
  /** Réglés via `settings atrperiod <n>`/`settings atrtimeframe <tf>` (cf. commands/settings.ts),
   * persistés dans settings.ts — lus par `trade.ts` (mode ATR) et `useAtrAutoRefresh.ts`. */
  atrPeriod: number;
  atrTimeframe: TrendbarPeriod;
  setAtrPeriod: (period: number) => void;
  setAtrTimeframe: (timeframe: TrendbarPeriod) => void;
  /** `true` dès qu'une url/un token non vide est persisté — jamais la valeur elle-même (le token ne
   * doit jamais transiter par du texte affiché). Lu par `settings url`/`settings token` pour
   * répondre "défini/non défini" sans avoir à relire le disque depuis une commande. */
  hasMcpUrl: boolean;
  hasMcpToken: boolean;
  /** Persistent puis déclenchent une reconnexion (nouveau CtraderClient, cf. App.tsx#reloadConfig)
   * — utilisés uniquement par `settings url <url>`/`settings token <token>`. */
  setMcpUrl: (url: string) => void;
  setMcpToken: (token: string) => void;
  setFeedback: (feedback: Feedback) => void;
  refreshMarket: () => Promise<void>;
  refreshNews: (options?: { force?: boolean }) => Promise<void>;
  proposeTrade: (trade: PreparedTrade) => void;
  proposeModify: (order: CtraderOrder, stopLoss?: number, takeProfit?: number) => void;
  proposeCancel: (orders: CtraderOrder[]) => void;
  proposePositionAmend: (
    position: AmendablePosition,
    stopLoss?: number,
    takeProfit?: number,
  ) => void;
  proposeClose: (position: ClosablePosition) => void;
  /** Registre complet — seule `help` en a besoin (cf. help.ts). Injecté ici plutôt qu'importé
   * depuis registry.ts pour éviter un cycle d'import registry.ts → help.ts → registry.ts. */
  commands: Command[];
}

export interface Command {
  /** Mot tapé dans le CommandBar, ex. "trade". Toujours en minuscules. */
  name: string;
  /** Ligne "usage : ..." affichée sur une erreur de parsing ou via `help <commande>`. */
  usage: string;
  /** Une phrase, affichée dans `help` sans argument. */
  summary: string;
  /** Parse `args` et exécute — chaque implémentation gère elle-même son feedback d'erreur via
   * `ctx.setFeedback`, il n'y a pas de canal d'erreur séparé : une commande ne "retourne" rien. */
  run(args: string[], ctx: CommandContext): void;
}
