import { useState } from "react";
import { theme } from "./theme.ts";

interface CommandBarProps {
  feedback: string;
  onSubmit: (command: string) => void;
}

export function CommandBar({ feedback, onSubmit }: CommandBarProps) {
  const [value, setValue] = useState("");

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
        height: 4,
      }}
    >
      <text fg={theme.textMuted}>{feedback}</text>
      <box style={{ flexDirection: "row", alignItems: "center", columnGap: 1 }}>
        <text fg={theme.gold}>›</text>
        <input
          style={{ flexGrow: 1 }}
          placeholder="commande… (/help)"
          focused
          value={value}
          onInput={setValue}
          onSubmit={(submitted) => {
            // `InputProps["onSubmit"]` is typed as `string | SubmitEvent` due to an upstream
            // options-merge artifact (SubmitEvent is an empty marker type); <input> (unlike
            // <textarea>) always calls back with the string value.
            if (typeof submitted === "string") onSubmit(submitted);
            setValue("");
          }}
        />
      </box>
    </box>
  );
}
