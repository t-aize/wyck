import { TextAttributes } from "@opentui/core";
import { toLots } from "../../constants.ts";
import type { CtraderOrder, CtraderPosition, GetPositionsResult } from "../../ctrader/client.ts";
import { readPosition } from "../../ctrader/mappers.ts";
import { alignLeft, alignRight, formatDuration, formatPriceOrDash } from "../format.ts";
import { DOWN, UP } from "../glyphs.ts";
import { theme } from "../theme.ts";

interface PositionsPanelProps {
  positions: GetPositionsResult | undefined;
  now: Date;
}

const COLUMNS = {
  symbol: 10,
  side: 7,
  volume: 9,
  entry: 10,
  sl: 10,
  tp: 10,
  swap: 9,
  pnl: 13,
  age: 7,
} as const;

function headerRow() {
  return (
    <text fg={theme.textDim} attributes={TextAttributes.BOLD}>
      {alignLeft("SYMBOL", COLUMNS.symbol)}
      {alignLeft("SIDE", COLUMNS.side)}
      {alignRight("VOL", COLUMNS.volume)}
      {alignRight("ENTRY", COLUMNS.entry)}
      {alignRight("SL", COLUMNS.sl)}
      {alignRight("TP", COLUMNS.tp)}
      {alignRight("SWAP", COLUMNS.swap)}
      {alignRight("P&L", COLUMNS.pnl)}
      {alignRight("AGE", COLUMNS.age)}
    </text>
  );
}

function PositionRow({ position, now }: { position: CtraderPosition; now: Date }) {
  const p = readPosition(position);
  const sideColor = p.side === "SELL" ? theme.red : p.side === "BUY" ? theme.green : theme.textDim;
  const sideLabel = p.side === "BUY" ? `${UP} BUY` : p.side === "SELL" ? `${DOWN} SELL` : "—";
  const pnlColor = p.pnl === undefined ? theme.textDim : p.pnl >= 0 ? theme.green : theme.red;
  const pnlLabel = p.pnl === undefined ? "—" : `${p.pnl >= 0 ? UP : DOWN} ${p.pnl.toFixed(2)}`;
  const age = p.openTimestamp === undefined ? "—" : formatDuration(now.getTime() - p.openTimestamp);

  return (
    <text>
      <span fg={theme.text}>
        {alignLeft(p.id === undefined ? p.symbol : String(p.id), COLUMNS.symbol)}
      </span>
      <span fg={sideColor}>{alignLeft(sideLabel, COLUMNS.side)}</span>
      <span fg={theme.text}>{alignRight(p.volumeLots?.toFixed(2) ?? "—", COLUMNS.volume)}</span>
      <span fg={theme.text}>{alignRight(formatPriceOrDash(p.entry), COLUMNS.entry)}</span>
      <span fg={theme.red}>{alignRight(formatPriceOrDash(p.stopLoss), COLUMNS.sl)}</span>
      <span fg={theme.green}>{alignRight(formatPriceOrDash(p.takeProfit), COLUMNS.tp)}</span>
      <span fg={theme.textDim}>{alignRight(p.swap?.toFixed(2) ?? "—", COLUMNS.swap)}</span>
      <span fg={pnlColor}>{alignRight(pnlLabel, COLUMNS.pnl)}</span>
      <span fg={theme.textDim}>{alignRight(age, COLUMNS.age)}</span>
    </text>
  );
}

function orderHeaderRow() {
  return (
    <text fg={theme.textDim} attributes={TextAttributes.BOLD}>
      {alignLeft("ID", COLUMNS.symbol)}
      {alignLeft("SIDE", COLUMNS.side + 6)}
      {alignRight("VOL", COLUMNS.volume)}
      {alignRight("PRIX", COLUMNS.entry)}
      {alignRight("SL", COLUMNS.sl)}
      {alignRight("TP", COLUMNS.tp)}
    </text>
  );
}

function OrderRow({ order }: { order: CtraderOrder }) {
  const side = order.tradeSide;
  const sideColor = side === "SELL" ? theme.red : side === "BUY" ? theme.green : theme.textDim;
  const sideLabel = `${side === "BUY" ? UP : side === "SELL" ? DOWN : "—"} ${side ?? "—"} ${order.orderType}`;
  // limitPrice/stopPrice/stopLoss/takeProfit sont des "prix affichés" (cf. commentaire sur
  // CtraderOrder) — contrairement aux prix bruts x10^5 de CtraderSpotPrice/CtraderTrendbar,
  // ne PAS passer par formatPrice() ici (ça les redivisait par 100 000 en trop).
  const price = order.limitPrice ?? order.stopPrice;

  return (
    <text>
      <span fg={theme.text}>{alignLeft(String(order.orderId), COLUMNS.symbol)}</span>
      <span fg={sideColor}>{alignLeft(sideLabel, COLUMNS.side + 6)}</span>
      <span fg={theme.text}>{alignRight(toLots(order.volume).toFixed(2), COLUMNS.volume)}</span>
      <span fg={theme.text}>{alignRight(formatPriceOrDash(price), COLUMNS.entry)}</span>
      <span fg={theme.red}>{alignRight(formatPriceOrDash(order.stopLoss), COLUMNS.sl)}</span>
      <span fg={theme.green}>{alignRight(formatPriceOrDash(order.takeProfit), COLUMNS.tp)}</span>
    </text>
  );
}

export function PositionsPanel({ positions, now }: PositionsPanelProps) {
  const openPositions = positions?.positions ?? [];
  const pendingOrders = positions?.orders ?? [];

  return (
    <box
      title=" POSITIONS "
      titleColor={theme.gold}
      style={{
        flexDirection: "column",
        flexGrow: 3,
        flexBasis: 0,
        border: true,
        borderColor: theme.border,
        backgroundColor: theme.bg,
        paddingLeft: 1,
        paddingRight: 1,
      }}
    >
      {positions === undefined ? (
        <text fg={theme.textDim}>chargement…</text>
      ) : openPositions.length === 0 ? (
        <text fg={theme.textDim}>Aucune position ouverte.</text>
      ) : (
        <>
          {headerRow()}
          {openPositions.map((position, index) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: liste réactualisée en bloc à chaque poll, pas de clé stable connue (positionId non vérifié)
            <PositionRow key={index} position={position} now={now} />
          ))}
        </>
      )}

      {pendingOrders.length > 0 && (
        <box style={{ flexDirection: "column", marginTop: 1 }}>
          <text fg={theme.textMuted} attributes={TextAttributes.BOLD}>
            — ordres en attente —
          </text>
          {orderHeaderRow()}
          {pendingOrders.map((order) => (
            <OrderRow key={order.orderId} order={order} />
          ))}
        </box>
      )}
    </box>
  );
}
