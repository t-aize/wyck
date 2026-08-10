import {
  ADX_TRENDING_THRESHOLD,
  type PendingLevel,
  type PendingLevels,
  type Trend,
  type TrendState,
} from "../../domain/smc/trend.ts";
import { alignLeft, formatPrice } from "../format.ts";
import { DOWN, FLAT, UP } from "../glyphs.ts";
import type { TrendRow } from "../hooks/useTrend.ts";
import { theme } from "../theme.ts";

interface TrendPanelProps {
  rows: TrendRow[] | undefined;
  errorMessage: string | undefined;
}

const TF_WIDTH = 4;

function trendLabel(trend: Trend): string {
  return trend === 1 ? "haussier" : trend === -1 ? "baissier" : "range";
}

function trendGlyph(trend: Trend): string {
  return trend === 1 ? UP : trend === -1 ? DOWN : FLAT;
}

function trendColor(trend: Trend): string {
  return trend === 1 ? theme.green : trend === -1 ? theme.red : theme.textDim;
}

/** Verdict principal (méthode événementielle BOS/CHoCH + règle de confirmation) — la ligne la plus
 * importante, en toutes lettres et colorée par direction. "confirmation fragile" est le seul signal
 * qui n'a pas sa place dans DetailsLine ci-dessous (pas un des 5 champs demandés), donc reste ici en
 * annotation, affichée seulement quand elle s'applique. */
function VerdictLine({ label, swing }: { label: string; swing: TrendState }) {
  const weakConfirmation = swing.confirmedEvent !== 0 && swing.lastEventDisplacementOk === false;
  return (
    <text>
      <span fg={theme.text}>{alignLeft(label, TF_WIDTH)}</span>
      <span fg={trendColor(swing.confirmedEvent)}>
        {trendGlyph(swing.confirmedEvent)} {trendLabel(swing.confirmedEvent).toUpperCase()} confirmé
      </span>
      {weakConfirmation && <span fg={theme.textDim}> — confirmation fragile</span>}
    </text>
  );
}

function forcePhrase(swing: TrendState): { text: string; color: string } {
  if (swing.adx === undefined || swing.emaStackBullish === undefined) {
    return { text: "Force —", color: theme.textMuted };
  }
  const direction = swing.emaStackBullish ? "haussier" : "baissier";
  const magnitude = swing.adx > ADX_TRENDING_THRESHOLD ? "tendance" : "plat";
  return {
    text: `Force ${direction} (${magnitude})`,
    color: swing.emaStackBullish ? theme.green : theme.red,
  };
}

function sweepPhrase(swing: TrendState): { text: string; color: string } {
  if (swing.sweepLow) return { text: "Sweep haussier", color: theme.green };
  if (swing.sweepHigh) return { text: "Sweep baissier", color: theme.red };
  return { text: "Sweep aucun", color: theme.textMuted };
}

/**
 * Les 5 signaux bruts toujours visibles, en toutes lettres — STRUCTUREL (méthode HH/HL), INTERNE
 * (fractale plus courte, timing d'entrée), FORCE (ADX+EMA, pas du SMC), SWEEP (liquidity sweep).
 * CONFIRME est la ligne du dessus (VerdictLine) : c'est le verdict principal, pas un champ de plus
 * ici. Comparer visuellement Structurel/Interne au verdict remplace l'ancienne annotation "diverge
 * du structurel" — les deux valeurs sont côte à côte, plus besoin de le calculer pour l'utilisateur.
 */
function DetailsLine({ swing, internal }: { swing: TrendState; internal: TrendState }) {
  const force = forcePhrase(swing);
  const sweep = sweepPhrase(swing);
  return (
    <text>
      {alignLeft("", TF_WIDTH)}
      <span fg={theme.textMuted}>Structurel </span>
      <span fg={trendColor(swing.structural)}>{trendLabel(swing.structural)}</span>
      <span fg={theme.textMuted}> · Interne </span>
      <span fg={trendColor(internal.confirmedEvent)}>{trendLabel(internal.confirmedEvent)}</span>
      <span fg={theme.textMuted}> · </span>
      <span fg={force.color}>{force.text}</span>
      <span fg={theme.textMuted}> · </span>
      <span fg={sweep.color}>{sweep.text}</span>
    </text>
  );
}

function resistancePhrase(level: PendingLevel | undefined): string {
  return level
    ? `Résistance ${formatPrice(level.level)} (${level.kind})`
    : "pas de résistance surveillée";
}

function supportPhrase(level: PendingLevel | undefined): string {
  return level ? `Support ${formatPrice(level.level)} (${level.kind})` : "pas de support surveillé";
}

export interface PickedLevel {
  level: PendingLevel;
  fromInternal: boolean;
}

/**
 * Un niveau "en attente" (swing ou interne) ne l'est que parce qu'il n'a encore jamais été cassé
 * en clôture depuis sa formation (cf. detectStructureEvents) : une résistance en attente est donc
 * toujours ≥ dernière clôture, un support toujours ≤ — invariant garanti par construction, pas
 * juste "généralement vrai". Comparer les deux fractales revient donc à comparer deux prix situés
 * du même côté du prix courant : celui qui s'en approche le plus (le plus petit pour une
 * résistance, le plus grand pour un support) est simplement le plus proche, sans avoir besoin de
 * connaître le prix courant ici. Égalité ⇒ on garde le swing (structure majeure, source par
 * défaut).
 */
function closerOf(
  swingLevel: PendingLevel | undefined,
  internalLevel: PendingLevel | undefined,
  internalIsCloser: (internalPrice: number, swingPrice: number) => boolean,
): PickedLevel | undefined {
  if (!swingLevel) return internalLevel ? { level: internalLevel, fromInternal: true } : undefined;
  if (!internalLevel) return { level: swingLevel, fromInternal: false };
  return internalIsCloser(internalLevel.level, swingLevel.level)
    ? { level: internalLevel, fromInternal: true }
    : { level: swingLevel, fromInternal: false };
}

/** Résistance : plus proche du prix ⇔ prix du niveau plus petit (les deux sont ≥ prix courant). */
export function closerResistance(
  swing: PendingLevels,
  internal: PendingLevels,
): PickedLevel | undefined {
  return closerOf(
    swing.resistance,
    internal.resistance,
    (internalPrice, swingPrice) => internalPrice < swingPrice,
  );
}

/** Support : plus proche du prix ⇔ prix du niveau plus grand (les deux sont ≤ prix courant). */
export function closerSupport(
  swing: PendingLevels,
  internal: PendingLevels,
): PickedLevel | undefined {
  return closerOf(
    swing.support,
    internal.support,
    (internalPrice, swingPrice) => internalPrice > swingPrice,
  );
}

/**
 * Prochains niveaux de structure encore surveillés — résistance = cassure à la hausse, support =
 * cassure à la baisse, avec ce que leur cassure produirait (BOS continue la tendance en cours à
 * cette échelle, CHoCH l'inverse — cf. légende).
 *
 * Le niveau affiché est le plus proche du prix entre "swing" (structure majeure) et "internal"
 * (fractale courte déjà calculée pour le timing d'entrée dans DetailsLine) — cf. closerOf.
 * Sur un mouvement fort et peu corrigé, la fractale swing (large, ex. 30 bougies de chaque côté
 * sur H1) reste souvent bloquée sur un pivot ancien, loin du prix, pendant qu'aucun repli assez
 * long ne valide de nouveau sommet/creux à cette échelle — pas un bug, juste un délai de
 * confirmation plus long. "internal" confirme ses pivots bien plus vite et comble ce trou. Annoté
 * "interne" (en atténué, comme "confirmation fragile" dans VerdictLine) quand c'est lui qui a été
 * retenu, pour signaler que ce n'est pas la structure majeure.
 */
function PendingLevelsLine({ swing, internal }: { swing: TrendState; internal: TrendState }) {
  const resistance = closerResistance(swing.pending, internal.pending);
  const support = closerSupport(swing.pending, internal.pending);

  return (
    <text>
      {alignLeft("", TF_WIDTH)}
      <span fg={resistance ? theme.green : theme.textMuted}>
        {UP} {resistancePhrase(resistance?.level)}
      </span>
      {resistance?.fromInternal && <span fg={theme.textDim}> (interne)</span>}
      <span> </span>
      <span fg={support ? theme.red : theme.textMuted}>
        {DOWN} {supportPhrase(support?.level)}
      </span>
      {support?.fromInternal && <span fg={theme.textDim}> (interne)</span>}
    </text>
  );
}

function TrendRowBlock({ row }: { row: TrendRow }) {
  return (
    <>
      <VerdictLine label={row.label} swing={row.swing} />
      <DetailsLine swing={row.swing} internal={row.internal} />
      <PendingLevelsLine swing={row.swing} internal={row.internal} />
    </>
  );
}

export function TrendPanel({ rows, errorMessage }: TrendPanelProps) {
  return (
    <box
      title=" TENDANCE M5/M15/H1 "
      titleColor={theme.accent}
      style={{
        flexDirection: "column",
        flexGrow: 2,
        flexBasis: 0,
        border: true,
        borderColor: theme.border,
        backgroundColor: theme.bg,
        paddingLeft: 1,
        paddingRight: 1,
      }}
    >
      {errorMessage ? (
        <text fg={theme.red}>{errorMessage}</text>
      ) : rows === undefined ? (
        <text fg={theme.textDim}>chargement…</text>
      ) : (
        <>
          {rows.map((row, i) => (
            <box key={row.label} style={{ flexDirection: "column", marginTop: i === 0 ? 0 : 1 }}>
              <TrendRowBlock row={row} />
            </box>
          ))}
          <text fg={theme.textMuted}>
            BOS = cassure qui continue la tendance · CHoCH = cassure qui l'inverse
          </text>
        </>
      )}
    </box>
  );
}
