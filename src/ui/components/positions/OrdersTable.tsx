import { TextAttributes } from "@opentui/core";
import type { CtraderOrder } from "../../../ctrader/schemas.ts";
import type { InstrumentSpecs } from "../../../instrument/specs.ts";
import { toLots, toPips } from "../../../utils/priceMath.ts";
import { alignLeft, alignRight, formatPriceOrDash } from "../../format.ts";
import { ATR_TRACKED, DOWN, UP } from "../../glyphs.ts";
import type { SpotQuote } from "../../hooks/useMarketData.ts";
import { sideColor, theme } from "../../theme.ts";
import { COLUMNS, DEFAULT_PIP_SIZE } from "./columns.ts";

function orderHeaderRow() {
  return (
    <text fg={theme.textDim} attributes={TextAttributes.BOLD}>
      {alignLeft("SYMBOL", COLUMNS.symbol)}
      {alignLeft("ID", COLUMNS.id)}
      {alignLeft("SIDE", COLUMNS.side + 6)}
      {alignRight("VOL", COLUMNS.volume)}
      {alignRight("PRIX", COLUMNS.entry)}
      {alignRight("SL", COLUMNS.sl)}
      {alignRight("TP", COLUMNS.tp)}
      {alignRight("DIST", COLUMNS.dist)}
      {"  "}
      {alignLeft("ATR", COLUMNS.atr)}
    </text>
  );
}

function OrderRow({
  order,
  specs,
  quote,
  atrTracked,
  active,
}: {
  order: CtraderOrder;
  specs: InstrumentSpecs | undefined;
  quote: SpotQuote | undefined;
  atrTracked: boolean;
  active: boolean;
}) {
  const side = order.tradeSide;
  const sideLabel = `${side === "BUY" ? UP : side === "SELL" ? DOWN : "—"} ${side ?? "—"} ${order.orderType}`;
  const price = order.limitPrice ?? order.stopPrice;
  const mid = quote === undefined ? undefined : (quote.bidPrice + quote.askPrice) / 2;
  const pipSize = specs?.pipSize ?? DEFAULT_PIP_SIZE;
  const dist = price === undefined || mid === undefined ? "—" : `${toPips(mid - price, pipSize)}p`;
  const digits = specs?.digits ?? 2;
  const lots = specs ? toLots(order.volume, specs.lotSize) : undefined;
  const name = specs?.symbolName ?? `#${order.symbolId}`;
  const symbolFg = active ? theme.accent : theme.text;

  return (
    <text>
      <span fg={symbolFg} attributes={active ? TextAttributes.BOLD : TextAttributes.NONE}>
        {alignLeft(name, COLUMNS.symbol)}
      </span>
      <span fg={theme.textDim}>{alignLeft(String(order.orderId), COLUMNS.id)}</span>
      <span fg={sideColor(side)}>{alignLeft(sideLabel, COLUMNS.side + 6)}</span>
      <span fg={theme.text}>{alignRight(lots?.toFixed(2) ?? "—", COLUMNS.volume)}</span>
      <span fg={theme.text}>{alignRight(formatPriceOrDash(price, digits), COLUMNS.entry)}</span>
      <span fg={theme.red}>
        {alignRight(formatPriceOrDash(order.stopLoss, digits), COLUMNS.sl)}
      </span>
      <span fg={theme.green}>
        {alignRight(formatPriceOrDash(order.takeProfit, digits), COLUMNS.tp)}
      </span>
      <span fg={theme.textDim}>{alignRight(dist, COLUMNS.dist)}</span>
      <span>{"  "}</span>
      <span fg={atrTracked ? theme.atrMode : theme.textMuted}>
        {alignLeft(atrTracked ? ATR_TRACKED : "—", COLUMNS.atr)}
      </span>
    </text>
  );
}

export function OrdersTable({
  orders,
  catalogById,
  quotesBySymbolId,
  atrOrderIds,
  activeSymbolId,
}: {
  orders: CtraderOrder[];
  catalogById: ReadonlyMap<number, InstrumentSpecs>;
  quotesBySymbolId: ReadonlyMap<number, SpotQuote>;
  atrOrderIds: Set<number>;
  activeSymbolId: number | undefined;
}) {
  return (
    <box flexDirection="column" marginTop={1}>
      <text fg={theme.textMuted} attributes={TextAttributes.BOLD}>
        — ordres en attente (tous symboles) —
      </text>
      <scrollbox
        scrollX
        scrollY={false}
        wrapperOptions={{ flexGrow: 0, flexShrink: 0 }}
        viewportOptions={{ flexGrow: 0, flexShrink: 0 }}
        contentOptions={{ minHeight: 0 }}
        style={{ flexDirection: "column" }}
      >
        {orderHeaderRow()}
        {orders.length === 0 ? (
          <text fg={theme.textMuted}>aucun ordre en attente</text>
        ) : (
          orders.map((order) => (
            <OrderRow
              key={order.orderId}
              order={order}
              specs={catalogById.get(order.symbolId)}
              quote={quotesBySymbolId.get(order.symbolId)}
              atrTracked={atrOrderIds.has(order.orderId)}
              active={activeSymbolId === order.symbolId}
            />
          ))
        )}
      </scrollbox>
    </box>
  );
}
