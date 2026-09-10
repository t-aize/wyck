import { useKeyboard } from "@opentui/react";
import { useMemo, useState } from "react";
import type { InstrumentSpecs } from "../../instrument/specs.ts";
import { theme } from "../theme.ts";

interface SymbolPickerProps {
  catalog: InstrumentSpecs[];
  current: string | undefined;
  onSelect: (name: string) => void;
  onCancel: () => void;
}

const CLASS_LABEL: Record<InstrumentSpecs["assetClass"], string> = {
  forex: "forex",
  metal: "métal",
  index: "indice",
  crypto: "crypto",
  energy: "énergie",
  other: "autre",
};

export function SymbolPicker({ catalog, current, onSelect, onCancel }: SymbolPickerProps) {
  const [query, setQuery] = useState("");
  const [listFocused, setListFocused] = useState(false);

  const filtered = useMemo(() => {
    const needle = query.trim().toUpperCase();
    const matches = needle
      ? catalog.filter(
          (item) =>
            item.symbolName.toUpperCase().includes(needle) ||
            item.description.toUpperCase().includes(needle) ||
            item.assetClass.toUpperCase().includes(needle),
        )
      : catalog;
    return [...matches].sort((a, b) => {
      if (a.symbolName === current) return -1;
      if (b.symbolName === current) return 1;
      return a.symbolName.localeCompare(b.symbolName);
    });
  }, [catalog, query, current]);

  const options = filtered.map((item) => ({
    name: item.symbolName === current ? `${item.symbolName}  ●` : item.symbolName,
    description: `${CLASS_LABEL[item.assetClass]}  ${item.description || `${item.base}/${item.quote}`}`,
    value: item.symbolName,
  }));

  useKeyboard((key) => {
    if (key.name === "escape") onCancel();
    if (key.name === "tab" && !key.shift) setListFocused((value) => !value);
  });

  return (
    <box
      position="absolute"
      width="100%"
      height="100%"
      justifyContent="center"
      alignItems="center"
      zIndex={10}
    >
      <box
        title=" SYMBOLE "
        titleColor={theme.accent}
        flexDirection="column"
        width="90%"
        maxWidth={56}
        height="80%"
        maxHeight={24}
        border
        borderColor={theme.accent}
        backgroundColor={theme.panelBg}
        paddingLeft={1}
        paddingRight={1}
        paddingTop={1}
        paddingBottom={1}
        rowGap={1}
      >
        <text fg={theme.textDim}>{catalog.length} symboles cTrader — taper pour filtrer</text>
        <input
          focused={!listFocused}
          placeholder="US100, BTCUSD, EURUSD…"
          value={query}
          backgroundColor={theme.bg}
          focusedBackgroundColor={theme.bg}
          textColor={theme.text}
          focusedTextColor={theme.text}
          cursorColor={theme.accent}
          onInput={setQuery}
          onSubmit={() => {
            const first = filtered[0];
            if (first) onSelect(first.symbolName);
          }}
        />
        {options.length === 0 ? (
          <text fg={theme.textMuted}>aucun symbole ne correspond</text>
        ) : (
          <select
            focused={listFocused}
            options={options}
            showDescription
            showScrollIndicator
            height={14}
            backgroundColor={theme.bg}
            focusedBackgroundColor={theme.bg}
            selectedBackgroundColor={theme.accent}
            selectedTextColor={theme.bg}
            descriptionColor={theme.textMuted}
            selectedDescriptionColor={theme.bg}
            onSelect={(_index, option) => {
              const name = option?.value;
              if (typeof name === "string") onSelect(name);
            }}
          />
        )}
        <text fg={theme.textMuted}>
          {listFocused
            ? "↑↓ naviguer · Entrée valider · Échap fermer"
            : "Entrée = premier résultat · Tab liste · Échap fermer"}
        </text>
      </box>
    </box>
  );
}
