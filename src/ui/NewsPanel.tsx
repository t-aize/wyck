import { TextAttributes } from "@opentui/core";
import { useMemo } from "react";
import { type CalendarEvent, classifyImpact, isGoldRelevant } from "../news.ts";
import { alignLeft, formatRelative } from "./format.ts";
import { theme } from "./theme.ts";

interface NewsPanelProps {
  events: CalendarEvent[];
  errorMessage: string | undefined;
  now: Date;
}

// Le calendrier est toujours affiché en heure de Paris, indépendamment du fuseau système
// (utile pour une référence stable, peu importe où l'app tourne).
const PARIS_TZ = "Europe/Paris";
const dayKeyFormat = new Intl.DateTimeFormat("en-CA", {
  timeZone: PARIS_TZ,
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});
const timeFormat = new Intl.DateTimeFormat("fr-FR", {
  timeZone: PARIS_TZ,
  hour: "2-digit",
  minute: "2-digit",
});
const dayLabelFormat = new Intl.DateTimeFormat("fr-FR", {
  timeZone: PARIS_TZ,
  weekday: "long",
  day: "numeric",
  month: "long",
});

function parisDayKey(date: Date): string {
  return dayKeyFormat.format(date);
}

type Row =
  | { kind: "day"; key: string; label: string }
  | { kind: "event"; key: string; event: CalendarEvent; isNext: boolean };

/**
 * Ne garde qu'aujourd'hui (Paris) et les jours à venir : le panel n'a pas besoin
 * de défiler pour être utile, et l'historique déjà passé encombre plus qu'il n'aide.
 */
function buildRows(events: CalendarEvent[], now: Date): Row[] {
  const todayKey = parisDayKey(now);
  const upcoming = events.filter((event) => parisDayKey(new Date(event.timestamp)) >= todayKey);

  const rows: Row[] = [];
  let currentDayKey: string | undefined;
  let nextMarked = false;

  for (const event of upcoming) {
    const eventDate = new Date(event.timestamp);
    const dayKey = parisDayKey(eventDate);

    if (dayKey !== currentDayKey) {
      currentDayKey = dayKey;
      const label =
        dayKey === todayKey ? "AUJOURD'HUI" : dayLabelFormat.format(eventDate).toUpperCase();
      rows.push({ kind: "day", key: `day-${dayKey}`, label });
    }

    const isNext = !nextMarked && event.timestamp >= now.getTime();
    if (isNext) nextMarked = true;

    rows.push({
      kind: "event",
      key: `${event.date}-${event.country}-${event.title}`,
      event,
      isNext,
    });
  }

  return rows;
}

function impactColor(impact: ReturnType<typeof classifyImpact>): string {
  if (impact === "high") return theme.red;
  if (impact === "medium") return theme.gold;
  if (impact === "low") return theme.textDim;
  return theme.textMuted;
}

/** Icône par palier — distincte du ● "or" (colonne différente, information différente). */
function impactIcon(impact: ReturnType<typeof classifyImpact>): string {
  if (impact === "high") return "▲";
  if (impact === "medium") return "◆";
  if (impact === "low") return "·";
  return " ";
}

function DayHeader({ label }: { label: string }) {
  return (
    <text fg={theme.textMuted} attributes={TextAttributes.BOLD}>
      {`── ${label} `.padEnd(40, "─")}
    </text>
  );
}

/** "F 20K  P 15K" — n'affiche que les valeurs présentes et non vides. */
function formatFigures(event: CalendarEvent): string {
  const parts: string[] = [];
  if (event.forecast?.trim()) parts.push(`F ${event.forecast}`);
  if (event.previous?.trim()) parts.push(`P ${event.previous}`);
  return parts.join("  ");
}

function NewsRow({ event, isNext, now }: { event: CalendarEvent; isNext: boolean; now: Date }) {
  const impact = classifyImpact(event.impact);
  const gold = isGoldRelevant(event);
  const time = timeFormat.format(new Date(event.timestamp));
  const relative = formatRelative(event.timestamp - now.getTime());
  const figures = formatFigures(event);
  const bold = impact === "high" || isNext ? TextAttributes.BOLD : TextAttributes.NONE;
  const titleColor = isNext ? theme.gold : impact === "high" ? theme.text : theme.textDim;

  return (
    <text truncate attributes={bold}>
      <span fg={isNext ? theme.gold : theme.textMuted}>{isNext ? "▸ " : "  "}</span>
      <span fg={theme.textMuted}>{alignLeft(time, 6)}</span>
      <span fg={gold ? theme.gold : theme.textMuted}>{gold ? "● " : "  "}</span>
      <span fg={theme.textDim}>{alignLeft(event.country, 5)}</span>
      <span fg={impactColor(impact)}>
        {alignLeft(impact === "other" ? "" : `${impactIcon(impact)} ${impact.toUpperCase()}`, 9)}
      </span>
      <span fg={titleColor}>{event.title}</span>
      {figures && <span fg={theme.textMuted}>{`  ${figures}`}</span>}
      <span fg={theme.textMuted}>{`  (${relative})`}</span>
    </text>
  );
}

export function NewsPanel({ events, errorMessage, now }: NewsPanelProps) {
  const rows = useMemo(() => buildRows(events, now), [events, now]);

  return (
    <box
      title=" CALENDAR — à venir (heure de Paris) "
      titleColor={theme.gold}
      bottomTitle=" ▸prochain  ●or  ▲high ◆med ·low "
      bottomTitleAlignment="right"
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
      ) : rows.length === 0 ? (
        <text fg={theme.textDim}>Rien de programmé d'ici la fin de la semaine.</text>
      ) : (
        <scrollbox style={{ flexGrow: 1 }}>
          {rows.map((row) =>
            row.kind === "day" ? (
              <DayHeader key={row.key} label={row.label} />
            ) : (
              <NewsRow key={row.key} event={row.event} isNext={row.isNext} now={now} />
            ),
          )}
        </scrollbox>
      )}
    </box>
  );
}
