import { TextAttributes } from "@opentui/core";
import type { CtraderOrder } from "../../../ctrader/schemas.ts";
import { toLots, toPips } from "../../../utils/priceMath.ts";
import { alignLeft, alignRight, formatPriceOrDash } from "../../format.ts";
import { DOWN, UP } from "../../glyphs.ts";
import { sideColor, theme } from "../../theme.ts";
import { COLUMNS } from "./columns.ts";

function orderHeaderRow() {
  return (
    <text fg={theme.textDim} attributes={TextAttributes.BOLD}>
      {alignLeft("ID", COLUMNS.symbol)}
      {alignLeft("SIDE", COLUMNS.side + 6)}
      {alignRight("VOL", COLUMNS.volume)}
      {alignRight("PRIX", COLUMNS.entry)}
      {alignRight("SL", COLUMNS.sl)}
      {alignRight("TP", COLUMNS.tp)}
      {alignRight("DIST", COLUMNS.dist)}
    </text>
  );
}

function OrderRow({ order, mid }: { order: CtraderOrder; mid: number | undefined }) {
  const side = order.tradeSide;
  const sideLabel = `${side === "BUY" ? UP : side === "SELL" ? DOWN : "—"} ${side ?? "—"} ${order.orderType}`;
  // limitPrice/stopPrice/stopLoss/takeProfit sont des "prix affichés" (cf. commentaire sur
  // CtraderOrder) — contrairement aux prix bruts x10^5 de CtraderSpotPrice/CtraderTrendbar,
  // ne PAS passer par formatPrice() ici (ça les redivisait par 100 000 en trop).
  const price = order.limitPrice ?? order.stopPrice;
  const dist = price === undefined || mid === undefined ? "—" : `${toPips(mid - price)}p`;

  return (
    <text>
      <span fg={theme.text}>{alignLeft(String(order.orderId), COLUMNS.symbol)}</span>
      <span fg={sideColor(side)}>{alignLeft(sideLabel, COLUMNS.side + 6)}</span>
      <span fg={theme.text}>{alignRight(toLots(order.volume).toFixed(2), COLUMNS.volume)}</span>
      <span fg={theme.text}>{alignRight(formatPriceOrDash(price), COLUMNS.entry)}</span>
      <span fg={theme.red}>{alignRight(formatPriceOrDash(order.stopLoss), COLUMNS.sl)}</span>
      <span fg={theme.green}>{alignRight(formatPriceOrDash(order.takeProfit), COLUMNS.tp)}</span>
      <span fg={theme.textDim}>{alignRight(dist, COLUMNS.dist)}</span>
    </text>
  );
}

export function OrdersTable({ orders, mid }: { orders: CtraderOrder[]; mid: number | undefined }) {
  return (
    <box style={{ flexDirection: "column", marginTop: 1 }}>
      <text fg={theme.textMuted} attributes={TextAttributes.BOLD}>
        — ordres en attente —
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
        {orders.map((order) => (
          <OrderRow key={order.orderId} order={order} mid={mid} />
        ))}
      </scrollbox>
    </box>
  );
}
