import type { ClosablePosition, CtraderOrder, GetPositionsResult } from "../../ctrader/schemas.ts";
import type { PreparedTrade } from "../../trading/types.ts";
import { useCancelConfirm } from "./useCancelConfirm.ts";
import { useCloseConfirm } from "./useCloseConfirm.ts";
import { useCommandRouter } from "./useCommandRouter.ts";
import { type PendingModify, useModifyConfirm } from "./useModifyConfirm.ts";
import { type PendingPositionAmend, usePositionAmendConfirm } from "./usePositionAmendConfirm.ts";
import { useTradeConfirm } from "./useTradeConfirm.ts";

interface OrderActions {
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
  pendingPositionAmend: PendingPositionAmend | undefined;
  confirmPendingPositionAmend: () => void;
  cancelPendingPositionAmend: () => void;
  pendingClose: ClosablePosition | undefined;
  confirmPendingClose: () => void;
  dismissPendingClose: () => void;
}

/**
 * Composition fine des 6 hooks à responsabilité unique issus de l'éclatement de ce fichier :
 * `useTradeConfirm`/`useModifyConfirm`/`useCancelConfirm`/`usePositionAmendConfirm`/
 * `useCloseConfirm` possèdent chacun un état de confirmation pendante, `useCommandRouter`
 * parse/route les commandes du CommandBar et les appelle sur succès. Retourne exactement la même
 * forme `OrderActions` qu'avant l'éclatement — le rendu (App.tsx, les popups de confirmation) n'a
 * pas besoin de changer de structure. Un futur flux de confirmation ajoute un `useXConfirm.ts` + un
 * cas dans `useCommandRouter.ts`, sans faire regrossir un fichier unique.
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
  const positionAmendConfirm = usePositionAmendConfirm({ refreshMarket });
  const closeConfirm = useCloseConfirm({ refreshMarket });
  const { runCommand } = useCommandRouter({
    positions,
    refreshMarket,
    refreshNews,
    onReconfigure,
    tradeConfirm,
    modifyConfirm,
    cancelConfirm,
    positionAmendConfirm,
    closeConfirm,
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
    pendingPositionAmend: positionAmendConfirm.pendingPositionAmend,
    confirmPendingPositionAmend: positionAmendConfirm.confirmPendingPositionAmend,
    cancelPendingPositionAmend: positionAmendConfirm.cancelPendingPositionAmend,
    pendingClose: closeConfirm.pendingClose,
    confirmPendingClose: closeConfirm.confirmPendingClose,
    dismissPendingClose: closeConfirm.dismissPendingClose,
  };
}
