import { TextAttributes } from "@opentui/core";
import { useRef } from "react";
import { PRICE_SCALE } from "../../constants.ts";
import { activeKillzone, activeMarketSessions } from "../../sessions/active.ts";
import { formatClock, formatMoney, formatPrice, sparkline } from "../format.ts";
import { DOWN, FLAT, UP } from "../glyphs.ts";
import { theme } from "../theme.ts";

/** Ratio spread courant / médiane récente au-delà duquel le spread est signalé comme élargi
 * (news, rollover, liquidité faible) — pertinent pour un scalp où ça mange le take-profit. */
const WIDE_SPREAD_RATIO = 1.5;
/** Sous ce nombre d'échantillons, la médiane n'est pas assez fiable (ex : juste après connexion)
 * pour servir de référence — pas d'alerte plutôt qu'un faux positif. */
const MIN_SPREAD_SAMPLES = 5;
/** Seuil absolu ($ sur XAUUSD) au-delà duquel le spread est signalé "trop grand" quelle que soit
 * sa médiane récente — complète le seuil relatif ci-dessus, qui ne réagit pas si le spread est
 * déjà large en continu (médiane elle-même élevée) ou avant d'avoir assez d'historique. */
const MAX_SPREAD_DOLLARS = 1;

/** Médiane simple — moins sensible qu'une moyenne à un pic isolé (une bougie de news) dans la
 * fenêtre de référence du spread. */
function median(values: number[]): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[mid - 1]! + sorted[mid]!) / 2 : sorted[mid]!;
}

interface PriceHeaderProps {
  symbol: string;
  bid: number | undefined;
  ask: number | undefined;
  /** Prix moyen des derniers polls (cf. useMarketData) — rendu en sparkline à côté du bid/ask. */
  priceHistory: number[];
  /** ask-bid des derniers polls (cf. useMarketData) — référence pour repérer un spread élargi. */
  spreadHistory: number[];
  connected: boolean;
  now: Date;
  errorMessage: string | undefined;
  balance: number | undefined;
  moneyDigits: number | undefined;
  /** `false` tant qu'url/token n'ont pas été réglés via `settings url`/`settings token` (cf.
   * commands/settings.ts) — distingue "pas encore configuré" (jamais tenté connect(), pas
   * d'erreur) de "connexion en cours" pour ne pas laisser "chargement…" indéfiniment sans piste. */
  configured: boolean;
}

export function PriceHeader({
  symbol,
  bid,
  ask,
  priceHistory,
  spreadHistory,
  connected,
  now,
  errorMessage,
  balance,
  moneyDigits,
  configured,
}: PriceHeaderProps) {
  const statusColor = errorMessage ? theme.red : connected ? theme.green : theme.textDim;
  const statusLabel = errorMessage ? "ERREUR" : connected ? "LIVE" : "CONNEXION…";
  const hasPrice = bid !== undefined && ask !== undefined;

  // Compare au dernier prix DIFFÉRENT pour la flèche de direction (pas au dernier rendu :
  // l'horloge fait re-rendre ce composant chaque seconde même sans nouveau prix — sans la garde
  // ci-dessous, la flèche retombait à "flat" dans la seconde suivant chaque changement, vérifié
  // en pratique). Lire/écrire des refs pendant le rendu est correct ici : simple comparaison de
  // valeur, aucun appel impératif ni dépendance au layout.
  const lastBidRef = useRef<number | undefined>(undefined);
  const previousBidRef = useRef<number | undefined>(undefined);
  if (bid !== lastBidRef.current) {
    previousBidRef.current = lastBidRef.current;
    lastBidRef.current = bid;
  }
  const direction =
    bid === undefined || previousBidRef.current === undefined
      ? undefined
      : Math.sign(bid - previousBidRef.current);
  const directionIcon =
    direction === undefined || direction === 0 ? FLAT : direction > 0 ? UP : DOWN;
  const directionColor =
    direction === undefined || direction === 0
      ? theme.textMuted
      : direction > 0
        ? theme.green
        : theme.red;

  const spread = hasPrice ? ask - bid : undefined;
  const spreadBaseline = median(spreadHistory);
  const isRelativelyWide =
    spread !== undefined &&
    spreadHistory.length >= MIN_SPREAD_SAMPLES &&
    spreadBaseline > 0 &&
    spread > spreadBaseline * WIDE_SPREAD_RATIO;
  const isAbsolutelyWide = spread !== undefined && spread / PRICE_SCALE > MAX_SPREAD_DOLLARS;
  const spreadColor = isRelativelyWide || isAbsolutelyWide ? theme.red : theme.textMuted;

  const sessions = activeMarketSessions(now);
  const killzone = activeKillzone(now);

  return (
    <box
      style={{
        flexDirection: "row",
        justifyContent: "space-between",
        alignItems: "center",
        paddingLeft: 2,
        paddingRight: 2,
        height: 1,
        flexShrink: 0,
        backgroundColor: theme.panelBg,
      }}
    >
      <box style={{ flexDirection: "row", alignItems: "center", columnGap: 2 }}>
        <text attributes={TextAttributes.BOLD} fg={theme.accent}>
          AURUM
        </text>
        <text fg={theme.textDim}>{symbol}</text>
      </box>

      <box style={{ flexDirection: "row", alignItems: "center", columnGap: 1 }}>
        {hasPrice ? (
          <>
            <text fg={directionColor}>{directionIcon}</text>
            <text attributes={TextAttributes.BOLD} fg={theme.accent}>
              {formatPrice(bid)}
            </text>
            <text fg={theme.textMuted}> / {formatPrice(ask)}</text>
            {spread !== undefined && (
              <text fg={spreadColor}>
                {" · "}
                {formatPrice(spread)}
              </text>
            )}
            {priceHistory.length >= 2 && (
              <text fg={theme.textMuted}> {sparkline(priceHistory)}</text>
            )}
          </>
        ) : !configured ? (
          <text fg={theme.red}>
            {"non configuré — settings url <url> puis settings token <token>"}
          </text>
        ) : (
          <text fg={theme.textDim}>{errorMessage ?? "chargement…"}</text>
        )}
      </box>

      <box style={{ flexDirection: "row", alignItems: "center", columnGap: 2 }}>
        <text fg={statusColor} attributes={TextAttributes.BOLD}>
          ● {statusLabel}
        </text>
        {balance !== undefined && moneyDigits !== undefined && (
          <text fg={theme.accent}>{formatMoney(balance, moneyDigits)}</text>
        )}
        <text fg={theme.textDim}>
          {balance !== undefined && moneyDigits !== undefined ? "· " : ""}
          {formatClock(now)}
        </text>
        <box style={{ flexDirection: "row", alignItems: "center" }}>
          <text fg={theme.textDim}>· </text>
          {sessions.length === 0 ? (
            <text fg={theme.textMuted}>marché fermé</text>
          ) : (
            sessions.flatMap((session, i) => [
              i > 0 ? (
                <text key={`sep-${session.id}`} fg={theme.textMuted}>
                  /
                </text>
              ) : null,
              <text
                key={session.id}
                fg={theme.sessions[session.id]}
                attributes={TextAttributes.BOLD}
              >
                {session.label}
              </text>,
            ])
          )}
          {killzone && <text fg={theme.killzones[killzone.id]}> ({killzone.label})</text>}
        </box>
      </box>
    </box>
  );
}
