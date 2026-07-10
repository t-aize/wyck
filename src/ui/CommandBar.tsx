import type { InputRenderable } from "@opentui/core";
import { useKeyboard } from "@opentui/react";
import { useRef, useState } from "react";
import { theme } from "./theme.ts";

export type FeedbackKind = "info" | "success" | "error";
export interface Feedback {
  kind: FeedbackKind;
  message: string;
}

interface CommandBarProps {
  feedback: Feedback;
  onSubmit: (command: string) => void;
  /** Désactivé pendant qu'une popup (ex: confirmation de trade) a le focus clavier. */
  focused?: boolean;
}

const FEEDBACK_ICON: Record<FeedbackKind, string> = { info: "›", success: "✓", error: "✗" };
const FEEDBACK_COLOR: Record<FeedbackKind, string> = {
  info: theme.textMuted,
  success: theme.green,
  error: theme.red,
};

const COMMANDS = ["trade", "refresh", "clear", "help"] as const;

/** Autocomplétion sur le premier mot seulement — une fois un espace tapé, on est dans les arguments. */
function matchCommands(value: string): string[] {
  if (value.includes(" ")) return [];
  const lower = value.toLowerCase();
  if (!lower) return [];
  return COMMANDS.filter((c) => c.startsWith(lower) && c !== lower);
}

export function CommandBar({ feedback, onSubmit, focused = true }: CommandBarProps) {
  const [value, setValue] = useState("");
  const inputRef = useRef<InputRenderable>(null);
  const suggestions = matchCommands(value);

  function complete(command: string) {
    const completed = `${command} `;
    setValue(completed);
    if (inputRef.current) inputRef.current.value = completed;
  }

  useKeyboard((key) => {
    if (!focused || key.name !== "tab" || suggestions.length === 0) return;
    const first = suggestions[0];
    if (first) complete(first);
  });

  return (
    <box
      style={{
        flexDirection: "column",
        flexShrink: 0,
        backgroundColor: theme.panelBg,
        paddingLeft: 2,
        paddingRight: 2,
        paddingTop: 0,
        paddingBottom: 0,
        height: 5,
      }}
    >
      <text fg={FEEDBACK_COLOR[feedback.kind]}>
        {feedback.message && `${FEEDBACK_ICON[feedback.kind]} ${feedback.message}`}
      </text>
      <text fg={theme.textMuted}>
        {suggestions.length > 0 && (
          <>
            <span fg={theme.gold}>Tab</span>
            {` → ${suggestions.join("  ")}`}
          </>
        )}
      </text>
      <box style={{ flexDirection: "row", alignItems: "center", columnGap: 1 }}>
        <text fg={theme.gold}>›</text>
        <input
          ref={inputRef}
          style={{ flexGrow: 1 }}
          placeholder="commande… (help)"
          focused={focused}
          value={value}
          onInput={setValue}
          onSubmit={(submitted) => {
            // `InputProps["onSubmit"]` is typed as `string | SubmitEvent` due to an upstream
            // options-merge artifact (SubmitEvent is an empty marker type); <input> (unlike
            // <textarea>) always calls back with the string value.
            if (typeof submitted === "string") onSubmit(submitted);
            // Le state contrôlé seul ne suffit pas à vider le champ après submit — vérifié
            // en pratique (l'ancien texte reste affiché et les frappes suivantes s'y accumulent).
            // Il faut aussi vider la valeur interne du renderable directement via la ref.
            setValue("");
            if (inputRef.current) inputRef.current.value = "";
          }}
        />
      </box>
    </box>
  );
}
