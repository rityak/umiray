import type { ReactNode } from "react";
import { PowerButton, Text } from "rootik";
import { t } from "../i18n";

type Props = {
  on: boolean;
  powering: boolean;
  /// The last attempt failed: the button turns red, the words say why.
  failed: boolean;
  disabled?: boolean;
  headline: string;
  /// How the traffic goes and for how long — or why it does not.
  detail: ReactNode;
  onPower: () => void;
};

/// The power button with the state beside it — the top of every engine's connection card
/// (D-060, D-154): one row, the button at the card's leading edge, the words right after it.
export default function PowerRow({
  on,
  powering,
  failed,
  disabled,
  headline,
  detail,
  onPower,
}: Props) {
  return (
    <div className="flex items-center gap-4">
      <PowerButton
        label={t("Connection")}
        size="sm"
        on={on}
        pending={powering}
        tone={failed ? "danger" : "accent"}
        disabled={disabled}
        onChange={onPower}
      />
      {/* A new state fades in, so "Connecting…" → "Connected" reads as a step, not a blink.
          The detail (uptime) ticks in place. */}
      <div key={headline} className="um-swap flex min-w-0 flex-col gap-0.5">
        <span className="um-headline truncate">{headline}</span>
        <Text tone="muted" size="sm" truncate>
          {detail}
        </Text>
      </div>
    </div>
  );
}
