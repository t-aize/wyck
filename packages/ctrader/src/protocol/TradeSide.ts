/**
 * Sens d'un trade ou d'une position, tel que le fil MCP le transporte (`"BUY"` / `"SELL"`).
 *
 * Enum string : la valeur runtime **est** le literal JSON, donc un payload
 * `{ tradeSide: "BUY" }` se compare et s'assigne sans table de correspondance.
 */
export enum TradeSide {
  BUY = "BUY",
  SELL = "SELL",
}
