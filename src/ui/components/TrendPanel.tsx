import type { PendingLevel, PendingLevels, Trend, TrendState } from "../../domain/smc/trend.ts";
import { alignLeft, formatPrice } from "../format.ts";
import { DOWN, FLAT, UP } from "../glyphs.ts";
import { h1ConfirmedBias, type TrendRow } from "../hooks/useTrend.ts";
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

/**
 * Le verdict événementiel (BOS/CHoCH + règle de confirmation) est la SEULE direction affichée par
 * TF — plus de "Structurel"/"Interne"/"Force" listés à côté comme des votes concurrents : ce sont
 * trois méthodes différentes qui ne mesurent pas la même chose (cf. commentaires de tête de
 * domain/smc/trend.ts), les montrer côte à côte comme si elles répondaient à la même question
 * créait une impression de contradiction plutôt que d'information. Ci-dessous, chacune redevient un
 * qualificatif optionnel du verdict, affiché seulement quand il ajoute un vrai signal :
 * - "fragile" : dernière cassure sans displacement suffisant (ATR) — confirmation faible.
 * - "structure en range"/"structure opposée" : la méthode structurelle (HH/HL) diverge du verdict
 *   événementiel — silencieux quand elle est alignée (le cas normal).
 * - "contre le biais H1" : ce TF va à l'encontre du biais H1 confirmé — même règle que
 *   l'avertissement à la confirmation d'un trade (domain/trading.ts#conflictsWithHtfBias), jamais
 *   affiché sur la ligne H1 elle-même (comparaison à soi-même, toujours vraie).
 * - le sweep, seulement s'il y en a un — "Sweep aucun" sur 3 TF n'apportait rien.
 * "Force" (EMA+ADX) est retiré entièrement : ni SMC, ni décisif ici, et la principale source de
 * "faux désaccord" avec le verdict.
 */
function structuralNote(swing: TrendState): string | undefined {
  if (swing.confirmedEvent === 0 || swing.structural === swing.confirmedEvent) return undefined;
  return swing.structural === 0 ? "structure en range" : "structure opposée";
}

function sweepNote(swing: TrendState): { text: string; color: string } | undefined {
  if (swing.sweepLow) return { text: "sweep haussier", color: theme.green };
  if (swing.sweepHigh) return { text: "sweep baissier", color: theme.red };
  return undefined;
}

function VerdictLine({
  label,
  swing,
  h1Bias,
}: {
  label: string;
  swing: TrendState;
  /** Biais H1 confirmé (cf. h1ConfirmedBias) — sans effet sur la ligne H1 elle-même (toujours
   * alignée avec son propre biais). */
  h1Bias: Trend;
}) {
  const weakConfirmation = swing.confirmedEvent !== 0 && swing.lastEventDisplacementOk === false;
  const structural = structuralNote(swing);
  const sweep = sweepNote(swing);
  const contreH1 =
    label !== "H1" && swing.confirmedEvent !== 0 && h1Bias !== 0 && swing.confirmedEvent !== h1Bias;

  return (
    <text>
      <span fg={theme.text}>{alignLeft(label, TF_WIDTH)}</span>
      <span fg={trendColor(swing.confirmedEvent)}>
        {trendGlyph(swing.confirmedEvent)} {trendLabel(swing.confirmedEvent).toUpperCase()} confirmé
      </span>
      {weakConfirmation && <span fg={theme.textDim}> — fragile</span>}
      {structural && <span fg={theme.textDim}> · {structural}</span>}
      {contreH1 && <span fg={theme.accent}> · ⚠ contre le biais H1</span>}
      {sweep && <span fg={sweep.color}> · {sweep.text}</span>}
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
 * (fractale courte, sert au timing d'entrée) — cf. closerOf.
 * Sur un mouvement fort et peu corrigé, la fractale swing (large, ex. 30 bougies de chaque côté
 * sur H1) reste souvent bloquée sur un pivot ancien, loin du prix, pendant qu'aucun repli assez
 * long ne valide de nouveau sommet/creux à cette échelle — pas un bug, juste un délai de
 * confirmation plus long. "internal" confirme ses pivots bien plus vite et comble ce trou. Annoté
 * "interne" (en atténué, comme les qualificatifs de VerdictLine) quand c'est lui qui a été retenu,
 * pour signaler que ce n'est pas la structure majeure.
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

function TrendRowBlock({ row, h1Bias }: { row: TrendRow; h1Bias: Trend }) {
  return (
    <>
      <VerdictLine label={row.label} swing={row.swing} h1Bias={h1Bias} />
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
              <TrendRowBlock row={row} h1Bias={h1ConfirmedBias(rows)} />
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
