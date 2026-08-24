/** Contrat commun à toutes les commandes du CommandBar (`src/commands/*.ts`). */

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
  /** Réglé via `settings atrrefresh on|off` (cf. settings.ts), persisté dans config.ts — lu par
   * `useAtrAutoRefresh.ts`, pas par une commande. */
  atrRefreshEnabled: boolean;
  setAtrRefreshEnabled: (enabled: boolean) => void;
  setFeedback: (feedback: Feedback) => void;
  refreshMarket: () => Promise<void>;
  refreshNews: (options?: { force?: boolean }) => Promise<void>;
  onReconfigure: () => void;
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
