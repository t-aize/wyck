import type { GetPositionsResult } from "../../ctrader/schemas.ts";
import { theme } from "../theme.ts";
import { isUnmapped } from "./positions/columns.ts";
import { OrdersTable } from "./positions/OrdersTable.tsx";
import { PositionsTable } from "./positions/PositionsTable.tsx";

interface PositionsPanelProps {
  positions: GetPositionsResult | undefined;
  /** Prix affiché (déjà divisé par PRICE_SCALE, cf. useMarketData). */
  bidPrice: number | undefined;
  askPrice: number | undefined;
}

export function PositionsPanel({ positions, bidPrice, askPrice }: PositionsPanelProps) {
  const openPositions = positions?.positions ?? [];
  const pendingOrders = positions?.orders ?? [];
  const mid =
    bidPrice === undefined || askPrice === undefined ? undefined : (bidPrice + askPrice) / 2;

  // Si les champs à haute confiance (cf. isUnmapped) ne résolvent pas sur des positions réellement
  // ouvertes, l'hypothèse de mapping (cf. CtraderPositionSchema dans ctrader/schemas.ts) est
  // cassée — mieux vaut le dire que d'afficher des "—" sans explication.
  const hasUnmapped = openPositions.some(isUnmapped);

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
        <PositionsTable
          positions={openPositions}
          mid={mid}
          bidPrice={bidPrice}
          askPrice={askPrice}
        />
      )}

      {pendingOrders.length > 0 && <OrdersTable orders={pendingOrders} mid={mid} />}
    </box>
  );
}
