import type { AtrSettings } from "../../config.ts";
import type { CtraderOrder, GetPositionsResult } from "../../ctrader/schemas.ts";
import type { PreparedTrade } from "../../domain/trading.ts";
import type { AtrOrderTracking } from "./useAtrOrderTracking.ts";
import { useCancelConfirm } from "./useCancelConfirm.ts";
import { useCommandRouter } from "./useCommandRouter.ts";
import { type PendingModify, useModifyConfirm } from "./useModifyConfirm.ts";
import { useTradeConfirm } from "./useTradeConfirm.ts";
import type { TrendRow } from "./useTrend.ts";

export interface OrderActions {
  runCommand: (raw: string) => void;
  atrMode: boolean;
  toggleAtrMode: () => void;
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
 * futur 5ᵉ flux de confirmation (ex. une feature SMC) ajoute un `useXConfirm.ts` + un cas dans
 * `useCommandRouter.ts`, sans faire regrossir un fichier unique.
 */
export function useOrderActions(opts: {
  positions: GetPositionsResult | undefined;
  refreshMarket: () => Promise<void>;
  refreshNews: (options?: { force?: boolean }) => Promise<void>;
  refreshTrend: () => Promise<void>;
  trendRows: TrendRow[] | undefined;
  onReconfigure: () => void;
  /** ATR le plus récent sur le timeframe configuré, échelle brute x10^5 (cf. useTrend.ts#atr) —
   * consommé par `trade` en mode ATR. */
  atrRaw: number | undefined;
  atrSettings: AtrSettings;
  onUpdateAtrSettings: (patch: Partial<AtrSettings>) => void;
  atrTracking: Pick<AtrOrderTracking, "registerPendingAtrOrder" | "untrackOrder">;
}): OrderActions {
  const {
    positions,
    refreshMarket,
    refreshNews,
    refreshTrend,
    trendRows,
    onReconfigure,
    atrRaw,
    atrSettings,
    onUpdateAtrSettings,
    atrTracking,
  } = opts;

  const tradeConfirm = useTradeConfirm({ trendRows, refreshMarket, atrTracking });
  const modifyConfirm = useModifyConfirm({ refreshMarket, atrTracking });
  const cancelConfirm = useCancelConfirm({ refreshMarket });
  const { runCommand, atrMode, toggleAtrMode } = useCommandRouter({
    positions,
    refreshMarket,
    refreshNews,
    refreshTrend,
    onReconfigure,
    atrRaw,
    atrSettings,
    onUpdateAtrSettings,
    tradeConfirm,
    modifyConfirm,
    cancelConfirm,
  });

  return {
    runCommand,
    atrMode,
    toggleAtrMode,
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
