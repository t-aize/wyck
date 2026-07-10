import { TextAttributes } from "@opentui/core";
import type { GetPositionsResult } from "../ctrader-client.ts";
import { alignLeft, alignRight, formatDuration, formatPrice } from "./format.ts";
import { theme } from "./theme.ts";

interface PositionsPanelProps {
  positions: GetPositionsResult | undefined;
  now: Date;
}

const COLUMNS = {
  symbol: 10,
  side: 5,
  volume: 9,
  entry: 10,
  sl: 10,
  tp: 10,
  swap: 9,
  pnl: 11,
  age: 7,
} as const;

/**
 * `CtraderPosition`/`CtraderOrder` sont typés `Record<string, unknown>` : leur
 * forme exacte n'a jamais pu être vérifiée contre un payload réel (compte
 * démo sans position ouverte au moment de l'implémentation). Ces lecteurs
 * essaient plusieurs noms de champ plausibles (convention "prix affiché"
 * observée sur le reste de l'API) et retombent sur "—" plutôt que d'inventer
 * une valeur. À corriger avec les vrais noms dès qu'une position réelle
 * passe par ici.
 */
function readNumber(record: Record<string, unknown>, keys: string[]): number | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "number") return value;
  }
  return undefined;
}

function readString(record: Record<string, unknown>, keys: string[]): string | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string") return value;
  }
  return undefined;
}

function readPosition(position: Record<string, unknown>) {
  return {
    symbol:
      readString(position, ["symbolName", "symbol"]) ??
      String(readNumber(position, ["symbolId"]) ?? "—"),
    side: readString(position, ["tradeSide", "side"]),
    // lotSize métaux = 100 → volume(1/100 unités) / 10000 = lots. Approximation valable pour XAUUSD,
    // seul symbole tradé par ce panel — à revoir si d'autres classes d'actifs sont ajoutées un jour.
    volumeLots: (() => {
      const raw = readNumber(position, ["volume"]);
      return raw === undefined ? undefined : raw / 10_000;
    })(),
    entry: readNumber(position, ["entryPrice", "price"]),
    stopLoss: readNumber(position, ["stopLoss"]),
    takeProfit: readNumber(position, ["takeProfit"]),
    swap: readNumber(position, ["swap"]),
    pnl: readNumber(position, ["pnl", "profit", "grossProfit", "netProfit", "unrealizedNetProfit"]),
    openTimestamp: readNumber(position, ["openTimestamp", "utcLastUpdateTimestamp", "timestamp"]),
  };
}

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

function PositionRow({ position, now }: { position: Record<string, unknown>; now: Date }) {
  const p = readPosition(position);
  const sideColor = p.side === "SELL" ? theme.red : p.side === "BUY" ? theme.green : theme.textDim;
  const pnlColor = p.pnl === undefined ? theme.textDim : p.pnl >= 0 ? theme.green : theme.red;
  const age = p.openTimestamp === undefined ? "—" : formatDuration(now.getTime() - p.openTimestamp);

  return (
    <text>
      <span fg={theme.text}>{alignLeft(p.symbol, COLUMNS.symbol)}</span>
      <span fg={sideColor}>{alignLeft(p.side ?? "—", COLUMNS.side)}</span>
      <span fg={theme.text}>{alignRight(p.volumeLots?.toFixed(2) ?? "—", COLUMNS.volume)}</span>
      <span fg={theme.text}>{alignRight(formatPrice(p.entry), COLUMNS.entry)}</span>
      <span fg={theme.red}>{alignRight(formatPrice(p.stopLoss), COLUMNS.sl)}</span>
      <span fg={theme.green}>{alignRight(formatPrice(p.takeProfit), COLUMNS.tp)}</span>
      <span fg={theme.textDim}>{alignRight(p.swap?.toFixed(2) ?? "—", COLUMNS.swap)}</span>
      <span fg={pnlColor}>{alignRight(p.pnl?.toFixed(2) ?? "—", COLUMNS.pnl)}</span>
      <span fg={theme.textDim}>{alignRight(age, COLUMNS.age)}</span>
    </text>
  );
}

function OrderRow({ order }: { order: Record<string, unknown> }) {
  const symbol =
    readString(order, ["symbolName", "symbol"]) ?? String(readNumber(order, ["symbolId"]) ?? "—");
  const side = readString(order, ["tradeSide", "side"]);
  const orderType = readString(order, ["orderType", "type"]) ?? "PENDING";
  const limitPrice = readNumber(order, ["limitPrice"]);
  const stopPrice = readNumber(order, ["stopPrice"]);
  const sideColor = side === "SELL" ? theme.red : side === "BUY" ? theme.green : theme.textDim;

  return (
    <text>
      <span fg={theme.text}>{alignLeft(symbol, COLUMNS.symbol)}</span>
      <span fg={sideColor}>{alignLeft(side ?? "—", COLUMNS.side)}</span>
      <span fg={theme.textDim}>{alignLeft(orderType, COLUMNS.volume + COLUMNS.entry)}</span>
      <span fg={theme.textDim}>{alignRight(formatPrice(limitPrice ?? stopPrice), COLUMNS.sl)}</span>
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
          {pendingOrders.map((order, index) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: idem, orderId non vérifié
            <OrderRow key={index} order={order} />
          ))}
        </box>
      )}
    </box>
  );
}
