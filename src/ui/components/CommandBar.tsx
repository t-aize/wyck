import type { InputRenderable } from "@opentui/core";
import { useKeyboard } from "@opentui/react";
import { forwardRef, useImperativeHandle, useRef, useState } from "react";
import { COMMANDS } from "../../commands/registry.ts";
import type { TrendbarPeriod } from "../../ctrader/protocol/TrendbarPeriod.ts";
import type { Feedback, FeedbackKind } from "../feedback.ts";
import { theme } from "../theme.ts";

interface CommandBarProps {
  feedback: Feedback;
  onSubmit: (command: string) => void;
  /** Désactivé pendant qu'une popup (ex: confirmation de trade) a le focus clavier. */
  focused?: boolean;
  /** Mode ATR de `trade`, basculé par Shift+Tab (cf. useTerminalShortcuts.ts) — purement pour
   * l'affichage ici, la bascule elle-même est gérée globalement, pas par ce composant. */
  atrMode?: boolean;
  /** Période/timeframe courants (`settings atrperiod`/`settings atrtimeframe`, cf.
   * commands/settings.ts) — affichés dans le titre uniquement en mode ATR, pour voir d'un coup
   * d'œil ce que `trade` en mode ATR va utiliser sans avoir à taper `settings`. */
  atrPeriod?: number;
  atrTimeframe?: TrendbarPeriod;
  /** Réglage `settings atrrefresh` (cf. useAtrAutoRefresh.ts) — sans rapport avec `atrMode`
   * ci-dessus : celui-ci contrôle la boucle de fond qui rafraîchit les ordres ATR déjà en attente,
   * `atrMode` ne fait qu'influencer comment le *prochain* `trade` tapé est interprété. */
  atrRefreshEnabled?: boolean;
  /** Secondes avant la prochaine passe de refresh ATR auto, `undefined` avant la toute première. */
  atrRefreshSecondsRemaining?: number;
}

const FEEDBACK_ICON: Record<FeedbackKind, string> = { info: "›", success: "✓", error: "✗" };
const FEEDBACK_COLOR: Record<FeedbackKind, string> = {
  info: theme.textMuted,
  success: theme.green,
  error: theme.red,
};

/** Exposé au parent pour que Ctrl+C (géré globalement, cf. useTerminalShortcuts) vide la ligne. */
export interface CommandBarHandle {
  /** Vide le champ s'il contient du texte. Retourne true si quelque chose a été effacé. */
  clearIfNotEmpty: () => boolean;
}

/** Autocomplétion sur le premier mot seulement — une fois un espace tapé, on est dans les arguments.
 * Noms tirés du registre (commands/registry.ts) : une commande ajoutée là apparaît ici sans autre
 * changement. */
function matchCommands(value: string): string[] {
  if (value.includes(" ")) return [];
  const lower = value.toLowerCase();
  if (!lower) return [];
  return COMMANDS.map((c) => c.name).filter((name) => name.startsWith(lower) && name !== lower);
}

export const CommandBar = forwardRef<CommandBarHandle, CommandBarProps>(function CommandBar(
  {
    feedback,
    onSubmit,
    focused = true,
    atrMode = false,
    atrPeriod,
    atrTimeframe,
    atrRefreshEnabled = false,
    atrRefreshSecondsRemaining,
  },
  ref,
) {
  const [value, setValue] = useState("");
  const inputRef = useRef<InputRenderable>(null);
  const suggestions = matchCommands(value);

  // Historique façon shell : flèche haut/bas pour parcourir les commandes passées.
  // -1 = pas en train de naviguer (ligne courante = brouillon).
  const historyRef = useRef<string[]>([]);
  const historyIndexRef = useRef(-1);
  const draftRef = useRef("");

  function setInputValue(next: string) {
    setValue(next);
    if (inputRef.current) inputRef.current.value = next;
  }

  function complete(command: string) {
    setInputValue(`${command} `);
  }

  useImperativeHandle(ref, () => ({
    clearIfNotEmpty() {
      if (!value) return false;
      setInputValue("");
      return true;
    },
  }));

  useKeyboard((key) => {
    if (!focused) return;

    if (key.name === "tab" && !key.shift && suggestions.length > 0) {
      const first = suggestions[0];
      if (first) complete(first);
      return;
    }

    if (key.name === "up") {
      const history = historyRef.current;
      if (history.length === 0) return;
      if (historyIndexRef.current === -1) draftRef.current = value;
      historyIndexRef.current =
        historyIndexRef.current === -1
          ? history.length - 1
          : Math.max(0, historyIndexRef.current - 1);
      setInputValue(history[historyIndexRef.current] ?? "");
      return;
    }

    if (key.name === "down") {
      if (historyIndexRef.current === -1) return;
      const nextIndex = historyIndexRef.current + 1;
      const history = historyRef.current;
      if (nextIndex >= history.length) {
        historyIndexRef.current = -1;
        setInputValue(draftRef.current);
      } else {
        historyIndexRef.current = nextIndex;
        setInputValue(history[nextIndex] ?? "");
      }
    }
  });

  const atrModeTitle =
    atrPeriod !== undefined && atrTimeframe !== undefined
      ? ` COMMANDE · MODE ATR · ATR(${atrPeriod}) ${atrTimeframe} `
      : " COMMANDE · MODE ATR ";

  return (
    <box
      title={atrMode ? atrModeTitle : " COMMANDE "}
      titleColor={atrMode ? theme.atrMode : focused ? theme.accent : theme.textMuted}
      flexDirection="column"
      flexShrink={0}
      border
      // Le seul repère visuel de "qui a le clavier" : gris clair quand la barre est active,
      // neutre quand une popup de confirmation a pris le focus (cf. `focused` dans App.tsx) —
      // sauf en mode ATR, où la bordure reste orange tant que la barre est active, pour rester
      // visible même si l'utilisateur ne regarde pas le titre.
      borderColor={focused ? (atrMode ? theme.atrMode : theme.borderActive) : theme.border}
      backgroundColor={theme.panelBg}
      paddingLeft={2}
      paddingRight={2}
      paddingTop={0}
      paddingBottom={0}
    >
      <text fg={FEEDBACK_COLOR[feedback.kind]}>
        {feedback.message && `${FEEDBACK_ICON[feedback.kind]} ${feedback.message}`}
      </text>
      <box flexDirection="row" justifyContent="space-between">
        <text fg={theme.textMuted}>
          {suggestions.length > 0 ? (
            <>
              <span fg={theme.accent}>Tab</span>
              {` → ${suggestions.join("  ")}`}
            </>
          ) : (
            <>
              <span fg={theme.atrMode}>Shift+Tab</span>
              {atrMode ? " → mode manuel" : " → mode ATR"}
            </>
          )}
        </text>
        <text fg={atrRefreshEnabled ? theme.atrMode : theme.textMuted}>
          {atrRefreshEnabled
            ? `refresh ATR ${atrRefreshSecondsRemaining === undefined ? "—" : `${atrRefreshSecondsRemaining}s`}`
            : "refresh ATR désactivé"}
        </text>
      </box>
      <box flexDirection="row" alignItems="center" columnGap={1}>
        <text fg={atrMode ? theme.atrMode : theme.accent}>›</text>
        <input
          ref={inputRef}
          flexGrow={1}
          placeholder="commande… (help)"
          focused={focused}
          value={value}
          onInput={setValue}
          onSubmit={(submitted) => {
            // `InputProps["onSubmit"]` is typed as `string | SubmitEvent` due to an upstream
            // options-merge artifact (SubmitEvent is an empty marker type); <input> (unlike
            // <textarea>) always calls back with the string value.
            if (typeof submitted === "string") {
              onSubmit(submitted);
              const trimmed = submitted.trim();
              if (trimmed && historyRef.current[historyRef.current.length - 1] !== trimmed) {
                historyRef.current.push(trimmed);
              }
            }
            historyIndexRef.current = -1;
            draftRef.current = "";
            // Le state contrôlé seul ne suffit pas à vider le champ après submit — vérifié
            // en pratique (l'ancien texte reste affiché et les frappes suivantes s'y accumulent).
            // Il faut aussi vider la valeur interne du renderable directement via la ref.
            setInputValue("");
          }}
        />
      </box>
    </box>
  );
});
