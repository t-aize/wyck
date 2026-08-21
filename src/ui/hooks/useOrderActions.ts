import type { CtraderOrder, GetPositionsResult } from "../../ctrader/schemas.ts";
import type { PreparedTrade } from "../../domain/trading.ts";
import { useCancelConfirm } from "./useCancelConfirm.ts";
import { useCommandRouter } from "./useCommandRouter.ts";
import { type PendingModify, useModifyConfirm } from "./useModifyConfirm.ts";
import { useTradeConfirm } from "./useTradeConfirm.ts";

export interface OrderActions {
  runCommand: (raw: string) => void;
  pendingTrade: PreparedTrade | undefined;
  confirmPendingTrade: () => void;
  cancelPendingTrade: () => void;
  pendingModify: PendingModify | undefined;
  confirmPendingModify: () => void;
  cancelPendingModify: () => void;
  pendingCancel: CtraderOrder[] | undefined;
  confirmPendingCancel: () => void;
  dismissPendingCancel: () => void;
}

/**
 * Composition fine des 4 hooks à responsabilité unique issus de l'éclatement de ce fichier :
 * `useTradeConfirm`/`useModifyConfirm`/`useCancelConfirm` possèdent
 * chacun un état de confirmation pendante, `useCommandRouter` parse/route les commandes du
 * CommandBar et les appelle sur succès. Retourne exactement la même forme `OrderActions` qu'avant
 * l'éclatement — le rendu (App.tsx, les 4 popups de confirmation) n'a pas besoin de changer. Un
 * futur 5ᵉ flux de confirmation ajoute un `useXConfirm.ts` + un cas dans `useCommandRouter.ts`,
 * sans faire regrossir un fichier unique.
 */
export function useOrderActions(opts: {
  positions: GetPositionsResult | undefined;
  refreshMarket: () => Promise<void>;
  refreshNews: (options?: { force?: boolean }) => Promise<void>;
  onReconfigure: () => void;
}): OrderActions {
  const { positions, refreshMarket, refreshNews, onReconfigure } = opts;

  const tradeConfirm = useTradeConfirm({ refreshMarket });
  const modifyConfirm = useModifyConfirm({ refreshMarket });
  const cancelConfirm = useCancelConfirm({ refreshMarket });
  const { runCommand } = useCommandRouter({
    positions,
    refreshMarket,
    refreshNews,
    onReconfigure,
    tradeConfirm,
    modifyConfirm,
    cancelConfirm,
  });

  return {
    runCommand,
    pendingTrade: tradeConfirm.pendingTrade,
    confirmPendingTrade: tradeConfirm.confirmPendingTrade,
    cancelPendingTrade: tradeConfirm.cancelPendingTrade,
    pendingModify: modifyConfirm.pendingModify,
    confirmPendingModify: modifyConfirm.confirmPendingModify,
    cancelPendingModify: modifyConfirm.cancelPendingModify,
    pendingCancel: cancelConfirm.pendingCancel,
    confirmPendingCancel: cancelConfirm.confirmPendingCancel,
    dismissPendingCancel: cancelConfirm.dismissPendingCancel,
  };
}
