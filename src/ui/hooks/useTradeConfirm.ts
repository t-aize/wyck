import { OrderType } from "@aurum/ctrader";
import { Effect } from "effect";
import { recordAtrTrade } from "../../trading/atrTradeStore.ts";
import { formatTradeSummary, toCreateOrderParams } from "../../trading/orderParams.ts";
import type { PreparedTrade } from "../../trading/types.ts";
import { fsRuntime } from "../../utils/effectRuntime.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { usePendingAction } from "./usePendingAction.ts";

export interface TradeConfirm {
  pendingTrade: PreparedTrade | undefined;
  /** Reçoit un `PreparedTrade` déjà calculé (cf. useCommandRouter.ts#runCommand, cas "trade") — pas
   * de parsing/préparation ici, uniquement le cycle propose → confirme/annule. */
  proposeTrade: (trade: PreparedTrade) => void;
  confirmPendingTrade: () => void;
  cancelPendingTrade: () => void;
}

/** `CreateOrderResult` est un {@link WriteResult} peu contractuel — `orderId` n'est pas garanti.
 * `undefined` plutôt qu'une exception si absent ou de forme inattendue : un ordre réellement créé
 * ne doit jamais apparaître en échec côté UI faute d'avoir pu extraire son id pour le suivi ATR. */
function extractOrderId(result: Record<string, unknown>): number | undefined {
  const raw = result.orderId;
  if (typeof raw === "number") return raw;
  if (typeof raw === "string") {
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? parsed : undefined;
  }
  return undefined;
}

/** Un des 4 hooks de confirmation bâtis sur usePendingAction.ts — possède `pendingTrade` et tout
 * son cycle de vie. */
export function useTradeConfirm(opts: { refreshMarket: () => Promise<void> }): TradeConfirm {
  const { client, symbolId } = useCtrader();
  const {
    pending: pendingTrade,
    propose: proposeTrade,
    confirm,
    cancel: cancelPendingTrade,
  } = usePendingAction<PreparedTrade>({
    refreshMarket: opts.refreshMarket,
    proposeMessage: "trade calculé — confirme dans la popup",
    progressMessage: "envoi de l'ordre…",
    cancelMessage: "trade annulé",
    errorPrefix: "échec envoi",
    // `symbolId` garanti défini ici par la garde de confirmPendingTrade ci-dessous — usePendingAction
    // n'appelle `run` que depuis confirm(), jamais avant.
    run: (trade) =>
      client.createOrder(toCreateOrderParams(symbolId as number, trade)).pipe(
        // Suivi best-effort pour le refresh auto ATR (cf. useAtrAutoRefresh.ts) : seulement les
        // trades ATR (atrRewardRiskRatio défini) qui restent en attente (jamais MARKET — devient une
        // position tout de suite, rien à rafraîchir, cf. l'ancien atrrefresh.ts). `Effect.promise`
        // garde le canal d'erreur à `never` (contrainte de `usePendingAction.run`) — un échec
        // d'écriture du suivi ne doit jamais faire échouer un ordre par ailleurs bien créé
        // (`recordAtrTrade` avale déjà ses propres erreurs, cf. atrTradeStore.ts).
        Effect.tap((result) => {
          if (trade.atrRewardRiskRatio === undefined || trade.orderType === OrderType.MARKET) {
            return Effect.void;
          }
          const orderId = extractOrderId(result);
          if (orderId === undefined) return Effect.void;
          return Effect.promise(() =>
            fsRuntime.runPromise(
              recordAtrTrade({
                orderId,
                tradeSide: trade.tradeSide,
                rewardRiskRatio: trade.atrRewardRiskRatio as number,
                riskPercent: trade.riskPercent,
              }),
            ),
          );
        }),
      ),
    successMessage: (trade) => `ordre envoyé : ${formatTradeSummary(trade)}`,
  });

  function confirmPendingTrade() {
    if (!symbolId) return;
    confirm();
  }

  return { pendingTrade, proposeTrade, confirmPendingTrade, cancelPendingTrade };
}
