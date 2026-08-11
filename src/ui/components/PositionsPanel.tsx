import { TextAttributes } from "@opentui/core";
import { useMemo } from "react";
import { PRICE_SCALE, SYMBOL, toLots, toPips } from "../../constants.ts";
import type { CtraderOrder, GetPositionsResult } from "../../ctrader/client.ts";
import { isUnmapped, type ReadPosition, readPosition } from "../../ctrader/mappers.ts";
import { computeUnrealizedPnl } from "../../domain/trading.ts";
import { alignLeft, alignRight, formatDuration, formatPriceOrDash } from "../format.ts";
import { DOWN, UP } from "../glyphs.ts";
import { theme } from "../theme.ts";

interface PositionsPanelProps {
  positions: GetPositionsResult | undefined;
  now: Date;
  /** Bid/ask bruts (échelle x10^5, cf. constants.ts) — pour les distances en pips. */
  bid: number | undefined;
  ask: number | undefined;
  /** Ordres en attente actuellement sous suivi ATR (cf. useAtrOrderTracking.ts) — marqués
   * visuellement dans la table, SL/TP réamendés automatiquement tant qu'ils y restent. */
  trackedOrderIds: Set<number>;
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
  dist: 14,
} as const;

/** Distance (en pips) au SL/TP le plus proche du prix courant, avec l'étiquette du côté concerné. */
function nearestPipsLabel(
  mid: number | undefined,
  stopLoss: number | undefined,
  takeProfit: number | undefined,
): string {
  if (mid === undefined) return "—";
  const candidates = [
    stopLoss === undefined ? undefined : { label: "SL", pips: toPips(mid - stopLoss) },
    takeProfit === undefined ? undefined : { label: "TP", pips: toPips(mid - takeProfit) },
  ].filter((c): c is { label: string; pips: number } => c !== undefined);
  if (candidates.length === 0) return "—";
  const nearest = candidates.reduce((a, b) => (a.pips <= b.pips ? a : b));
  return `${nearest.label} ${nearest.pips}p`;
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
      {alignRight("PROCHE", COLUMNS.dist)}
    </text>
  );
}

function PositionRow({
  p,
  now,
  mid,
  bidPrice,
  askPrice,
}: {
  p: ReadPosition;
  now: Date;
  mid: number | undefined;
  bidPrice: number | undefined;
  askPrice: number | undefined;
}) {
  const sideColor = p.side === "SELL" ? theme.red : p.side === "BUY" ? theme.green : theme.textDim;
  const sideLabel = p.side === "BUY" ? `${UP} BUY` : p.side === "SELL" ? `${DOWN} SELL` : "—";
  const pnl =
    p.side !== undefined &&
    p.volumeLots !== undefined &&
    p.entry !== undefined &&
    bidPrice !== undefined &&
    askPrice !== undefined
      ? computeUnrealizedPnl(p.side, p.volumeLots, p.entry, bidPrice, askPrice)
      : undefined;
  const pnlColor = pnl === undefined ? theme.textDim : pnl >= 0 ? theme.green : theme.red;
  const pnlLabel = pnl === undefined ? "—" : `${pnl >= 0 ? UP : DOWN} ${pnl.toFixed(2)}`;
  const age = p.openTimestamp === undefined ? "—" : formatDuration(now.getTime() - p.openTimestamp);
  const nearest = nearestPipsLabel(mid, p.stopLoss, p.takeProfit);

  return (
    <text>
      <span fg={theme.text}>
        {alignLeft(p.id === undefined ? SYMBOL : String(p.id), COLUMNS.symbol)}
      </span>
      <span fg={sideColor}>{alignLeft(sideLabel, COLUMNS.side)}</span>
      <span fg={theme.text}>{alignRight(p.volumeLots?.toFixed(2) ?? "—", COLUMNS.volume)}</span>
      <span fg={theme.text}>{alignRight(formatPriceOrDash(p.entry), COLUMNS.entry)}</span>
      <span fg={theme.red}>{alignRight(formatPriceOrDash(p.stopLoss), COLUMNS.sl)}</span>
      <span fg={theme.green}>{alignRight(formatPriceOrDash(p.takeProfit), COLUMNS.tp)}</span>
      <span fg={theme.textDim}>{alignRight(p.swap?.toFixed(2) ?? "—", COLUMNS.swap)}</span>
      <span fg={pnlColor}>{alignRight(pnlLabel, COLUMNS.pnl)}</span>
      <span fg={theme.textDim}>{alignRight(age, COLUMNS.age)}</span>
      <span fg={theme.textDim}>{alignRight(nearest, COLUMNS.dist)}</span>
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
      {alignRight("DIST", COLUMNS.dist)}
    </text>
  );
}

function OrderRow({
  order,
  mid,
  atrTracked,
}: {
  order: CtraderOrder;
  mid: number | undefined;
  atrTracked: boolean;
}) {
  const side = order.tradeSide;
  const sideColor = side === "SELL" ? theme.red : side === "BUY" ? theme.green : theme.textDim;
  const sideLabel = `${side === "BUY" ? UP : side === "SELL" ? DOWN : "—"} ${side ?? "—"} ${order.orderType}`;
  // limitPrice/stopPrice/stopLoss/takeProfit sont des "prix affichés" (cf. commentaire sur
  // CtraderOrder) — contrairement aux prix bruts x10^5 de CtraderSpotPrice/CtraderTrendbar,
  // ne PAS passer par formatPrice() ici (ça les redivisait par 100 000 en trop).
  const price = order.limitPrice ?? order.stopPrice;
  const dist = price === undefined || mid === undefined ? "—" : `${toPips(mid - price)}p`;
  // Marqueur "⚡" : SL/TP réamendés automatiquement tant que l'ordre reste en attente (cf.
  // useAtrOrderTracking.ts) — pas un simple id, un rappel que ce trade n'est plus figé.
  const idLabel = atrTracked ? `⚡${order.orderId}` : String(order.orderId);

  return (
    <text>
      <span fg={atrTracked ? theme.accent : theme.text}>{alignLeft(idLabel, COLUMNS.symbol)}</span>
      <span fg={sideColor}>{alignLeft(sideLabel, COLUMNS.side + 6)}</span>
      <span fg={theme.text}>{alignRight(toLots(order.volume).toFixed(2), COLUMNS.volume)}</span>
      <span fg={theme.text}>{alignRight(formatPriceOrDash(price), COLUMNS.entry)}</span>
      <span fg={theme.red}>{alignRight(formatPriceOrDash(order.stopLoss), COLUMNS.sl)}</span>
      <span fg={theme.green}>{alignRight(formatPriceOrDash(order.takeProfit), COLUMNS.tp)}</span>
      <span fg={theme.textDim}>{alignRight(dist, COLUMNS.dist)}</span>
    </text>
  );
}

export function PositionsPanel({ positions, now, bid, ask, trackedOrderIds }: PositionsPanelProps) {
  const openPositions = positions?.positions ?? [];
  const pendingOrders = positions?.orders ?? [];
  const mid = bid === undefined || ask === undefined ? undefined : (bid + ask) / 2 / PRICE_SCALE;
  const bidPrice = bid === undefined ? undefined : bid / PRICE_SCALE;
  const askPrice = ask === undefined ? undefined : ask / PRICE_SCALE;

  const mapped = useMemo(() => openPositions.map(readPosition), [openPositions]);
  // Si les champs à haute confiance (cf. commentaire en tête de ctrader/mappers.ts) ne
  // résolvent pas sur des positions réellement ouvertes, l'hypothèse de mapping est
  // cassée — mieux vaut le dire que d'afficher des "—" sans explication.
  const hasUnmapped = mapped.some(isUnmapped);

  return (
    <box
      title=" POSITIONS "
      titleColor={theme.accent}
      style={{
        flexDirection: "column",
        // Dynamique plutôt qu'une part fixe de l'écran (flexGrow) : la plupart du temps il n'y a
        // que 2-3 positions max (pas d'automatisation), pas de raison de réserver une grosse
        // portion d'écran vide au-dessus du calendrier/de la structure quand "Aucune position
        // ouverte." s'affiche seul.
        flexShrink: 0,
        border: true,
        borderColor: theme.border,
        backgroundColor: theme.bg,
        paddingLeft: 1,
        paddingRight: 1,
      }}
    >
      {hasUnmapped && (
        <text fg={theme.red}>
          ⚠ format de position inattendu — certaines valeurs peuvent être fausses ou manquantes
        </text>
      )}
      {positions === undefined ? (
        <text fg={theme.textDim}>chargement…</text>
      ) : openPositions.length === 0 ? (
        <text fg={theme.textDim}>Aucune position ouverte.</text>
      ) : (
        // Les colonnes (~99 caractères au total, cf. COLUMNS) sont à largeur fixe — sur un
        // terminal plus étroit, mieux vaut pouvoir défiler horizontalement que perdre des
        // colonnes recadrées par le terminal. `scrollX` seul (pas de flexGrow/height imposé)
        // laisse la hauteur continuer à s'ajuster au contenu comme avant.
        <scrollbox scrollX scrollY={false} style={{ flexDirection: "column" }}>
          {headerRow()}
          {mapped.map((p, index) => (
            <PositionRow
              // positionId a une confiance haute (cf. mappers.ts) ; l'index reste un filet
              // pour le cas — signalé ci-dessus — où le mapping échoue en pratique.
              key={p.id ?? index}
              p={p}
              now={now}
              mid={mid}
              bidPrice={bidPrice}
              askPrice={askPrice}
            />
          ))}
        </scrollbox>
      )}

      {pendingOrders.length > 0 && (
        <box style={{ flexDirection: "column", marginTop: 1 }}>
          <text fg={theme.textMuted} attributes={TextAttributes.BOLD}>
            — ordres en attente —
          </text>
          <scrollbox scrollX scrollY={false} style={{ flexDirection: "column" }}>
            {orderHeaderRow()}
            {pendingOrders.map((order) => (
              <OrderRow
                key={order.orderId}
                order={order}
                mid={mid}
                atrTracked={trackedOrderIds.has(order.orderId)}
              />
            ))}
          </scrollbox>
        </box>
      )}
    </box>
  );
}
