import type { CreateOrderParams } from "@aurum/ctrader";
import { PRICE_SCALE } from "../constants.ts";
import type { PreparedTrade } from "./types.ts";

function toPoints(priceDistance: number): number {
  return Math.round(priceDistance * PRICE_SCALE);
}

export function toCreateOrderParams(symbolId: number, trade: PreparedTrade): CreateOrderParams {
  const base = {
    symbolId,
    orderType: trade.orderType,
    tradeSide: trade.tradeSide,
    volume: trade.volume,
    label: "aurum",
  };

  if (trade.orderType === "MARKET") {
    return {
      ...base,
      relativeStopLoss: toPoints(Math.abs(trade.entryPrice - trade.stopLoss)),
      relativeTakeProfit: toPoints(Math.abs(trade.entryPrice - trade.takeProfit)),
    };
  }

  return {
    ...base,
    limitPrice: trade.orderType === "LIMIT" ? trade.entryPrice : undefined,
    stopPrice: trade.orderType === "STOP" ? trade.entryPrice : undefined,
    stopLoss: trade.stopLoss,
    takeProfit: trade.takeProfit,
  };
}

export function formatTradeSummary(trade: PreparedTrade): string {
  return (
    `${trade.tradeSide} ${trade.orderType} ${trade.entryPrice.toFixed(2)} · ` +
    `SL ${trade.stopLoss.toFixed(2)} · TP ${trade.takeProfit.toFixed(2)} · ` +
    `${trade.volumeLots.toFixed(2)} lots · risque ${trade.riskAmount.toFixed(2)} (${trade.riskPercent}%)`
  );
}
