import type { OrderType, TradeSide } from "../ctrader/schemas.ts";

/**
 * Erreur de validation métier d'un trade (risque%, prix indisponible, SL/TP incohérents, volume
 * sous le minimum…). Une seule classe : l'ancienne hiérarchie de 7 sous-types
 * tagués (`Data.TaggedError`, un par ancien `throw new Error(...)` distinct) n'était discriminée
 * par aucun appelant — tous se contentent de `.message` (`toMessage`) — donc la distinction par
 * tag n'apportait rien en pratique.
 */
export class TradeValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TradeValidationError";
  }
}

export interface TradeInput {
  /** "market" = prix courant (ordre MARKET) ; un nombre = prix affiché (LIMIT/STOP déduit) */
  entry: number | "market";
  /** % de l'équity du compte */
  riskPercent: number;
  stopLoss: number;
  takeProfit: number;
}

export interface PreparedTrade {
  orderType: OrderType;
  tradeSide: TradeSide;
  entryPrice: number;
  stopLoss: number;
  takeProfit: number;
  /** Volume au format API (1/100 d'once pour XAUUSD) */
  volume: number;
  /** Volume en lots (1 lot = 100 onces), pour l'affichage */
  volumeLots: number;
  riskAmount: number;
  riskPercent: number;
  /** Gain potentiel si le TP est atteint (même formule que riskAmount, distance TP) */
  rewardAmount: number;
  /** Renseigné uniquement par `prepareAtr.ts` (jamais `prepare.ts`) — signal "ce trade vient du
   * mode ATR", lu par `useTradeConfirm.ts` pour décider s'il faut suivre cet ordre dans
   * `atrTradeStore.ts` en vue du refresh automatique (cf. useAtrAutoRefresh.ts). */
  atrRewardRiskRatio?: number;
}
