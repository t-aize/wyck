import { useKeyboard } from "@opentui/react";
import type { ReactNode } from "react";
import { theme } from "../theme.ts";

export function Row({ label, value, fg }: { label: string; value: string; fg?: string }) {
  return (
    <box style={{ flexDirection: "row", justifyContent: "space-between" }}>
      <text fg={theme.textDim}>{label}</text>
      <text fg={fg ?? theme.text}>{value}</text>
    </box>
  );
}

interface ConfirmModalProps {
  title: string;
  confirmLabel: string;
  cancelLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
  children: ReactNode;
}

export function ConfirmModal({
  title,
  confirmLabel,
  cancelLabel = "✗ Annuler",
  onConfirm,
  onCancel,
  children,
}: ConfirmModalProps) {
  // Échap annule, en plus de naviguer jusqu'à "Annuler" dans le <select>.
  useKeyboard((key) => {
    if (key.name === "escape") onCancel();
  });

  return (
    <box
      style={{
        position: "absolute",
        width: "100%",
        height: "100%",
        justifyContent: "center",
        alignItems: "center",
      }}
      zIndex={10}
    >
      <box
        title={` ${title} `}
        titleColor={theme.accent}
        style={{
          flexDirection: "column",
          // Responsive plutôt qu'une largeur fixe : sur un terminal plus étroit que 46 colonnes,
          // une largeur fixe déborderait hors de l'écran (recadré par le terminal lui-même,
          // illisible) au lieu de se réduire — 90% s'adapte, maxWidth plafonne sur un grand terminal.
          width: "90%",
          maxWidth: 46,
          border: true,
          borderColor: theme.accent,
          backgroundColor: theme.panelBg,
          paddingLeft: 2,
          paddingRight: 2,
          paddingTop: 1,
          paddingBottom: 1,
          rowGap: 0,
        }}
      >
        {children}

        <box style={{ marginTop: 1 }}>
          <select
            focused
            options={[
              { name: confirmLabel, description: "" },
              { name: cancelLabel, description: "" },
            ]}
            showDescription={false}
            style={{ height: 2 }}
            selectedBackgroundColor={theme.accent}
            selectedTextColor={theme.bg}
            onSelect={(index) => (index === 0 ? onConfirm() : onCancel())}
          />
        </box>

        <text fg={theme.textMuted}>↑↓ naviguer · Entrée valider · Échap annuler</text>
      </box>
    </box>
  );
}
