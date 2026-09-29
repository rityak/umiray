import type { ReactNode } from "react";
import { PowerButton, Text } from "rootik";

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

/// The power button with the state next to it — the top of every engine's connection card
/// (D-060, D-154). Compact, to leave height for Traffic.
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
        label="VPN"
        size="sm"
        on={on}
        pending={powering}
        tone={failed ? "danger" : "accent"}
        disabled={disabled}
        onChange={onPower}
      />
      <div className="flex min-w-0 flex-col gap-1">
        <span className="um-headline">{headline}</span>
        <Text tone="muted" size="xs" className="block">
          {detail}
        </Text>
      </div>
    </div>
  );
}
