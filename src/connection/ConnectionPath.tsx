import { Server } from "lucide-react";
import { memo } from "react";
import { Badge, Card, CopyButton, Field, Item, PowerButton, SegmentedControl, Text } from "rootik";
import * as api from "../api";
import { t, tk } from "../i18n";
import Flag from "../shell/Flag";
import { hide } from "../shell/secret";
import Uptime from "../shell/Uptime";
import { delayTone } from "./NodeDelay";

type Props = {
  status: api.Status;
  mode: api.Choice;
  running: api.Choice | null;
  direction: api.Direction;
  /// Route chain reported by the core, or remembered for Manual (D-039, D-145).
  route: string[];
  /// Other nodes currently carrying traffic for a load-balanced group.
  more: number;
  /// Last node in the chain supplies protocol, address and latency.
  selected: api.Node | null;
  /// Total available nodes for Auto.
  total: number;
  /// Privacy mode hides the node address (D-127).
  hidden: boolean;
  busy: boolean;
  powering: boolean;
  onPower: () => void;
  onMode: (mode: api.Choice) => void;
  onDirection: (direction: api.Direction) => void;
};

const MODES = [
  { value: "local" as const, label: "Proxy", hint: tk("set the address in your app") },
  { value: "system" as const, label: "System", hint: tk("Windows proxy") },
  { value: "tun" as const, label: "TUN", hint: tk("all device traffic") },
];

/// Ordered from the safe default to full control.
const DIRECTIONS = [
  { value: "direct" as const, label: "Direct", hint: tk("bypass VPN") },
  { value: "auto" as const, label: "Auto", hint: tk("best available node") },
  { value: "manual" as const, label: "Manual", hint: tk("selected node") },
  { value: "rules" as const, label: "Rules", hint: tk("assigned by your rules") },
];

/// Explain routing while no exit is known.
const WAITING: Record<api.Direction, string> = {
  direct: tk("bypass VPN — no server is used"),
  auto: tk("the core will choose the best server"),
  manual: tk("select a node from the list"),
  rules: tk("your rules assign the server"),
};

/// Explain how the current exit was selected.
const CHOSEN: Record<api.Direction, string> = {
  direct: tk("bypass VPN"),
  auto: tk("selected automatically"),
  manual: tk("selected manually"),
  rules: tk("assigned by your rules"),
};

/// Capture guarantees differ between TUN, System and Proxy.
function what(status: api.Status): string {
  const mode = api.runningMode(status);
  if (!status.corePresent) return t("core missing — download it in Settings");
  if (mode === null)
    return t("{mode} selected, but VPN is off", { mode: api.MODE_LABEL[status.desiredMode] });
  if (mode === "tun") return t("all device traffic goes through the adapter");
  if (mode === "system") return t("proxy configured in Windows settings");
  return t("enter the address below in your browser or app");
}

/// Connection state next to the power button.
function headline(status: api.Status, powering: boolean): string {
  if (powering) return t("Connecting…");
  if (!status.corePresent) return t("Core missing");
  return status.running ? t("Connected") : t("Disconnected");
}

/**
 * Separate power, capture and routing controls (D-060, D-142).
 */
/// Traffic ticks do not affect these controls.
export default memo(function ConnectionPath({
  status,
  mode,
  running,
  direction,
  route,
  more,
  selected,
  total,
  hidden,
  busy,
  powering,
  onPower,
  onMode,
  onDirection,
}: Props) {
  const view = api.statusView(status, powering);
  const address = mode === "local" ? api.proxyAddress(status) : null;
  // Manual persists the user's selection; other exits require a running core (D-141).
  const live =
    route.length > 0 && direction !== "direct" && (status.running || direction === "manual");
  const why = [
    selected && `${selected.kind}${selected.address ? ` · ${hide(selected.address, hidden)}` : ""}`,
    `${t(CHOSEN[direction])}${direction === "auto" && total > 0 ? ` ${t("out of {n}", { n: total })}` : ""}`,
    !status.running && t("core is not running"),
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    // The state already serves as a heading; keep room for the chart below.
    <Card padding="sm" aria-label={t("Connection")}>
      <div className="flex flex-col gap-3">
        {/* Keep controls compact to leave height for Traffic. */}
        <div className="flex items-center gap-4">
          <PowerButton
            label="VPN"
            size="sm"
            on={status.running}
            pending={powering}
            tone={view.tone === "error" ? "danger" : "accent"}
            disabled={busy && !powering}
            onChange={onPower}
          />
          <div className="flex min-w-0 flex-col gap-1">
            <span className="um-headline">{headline(status, powering)}</span>
            <Text tone="muted" size="xs" className="block">
              {running !== null ? (
                <>
                  {api.MODE_LABEL[running]} ·{" "}
                  <Uptime started={status.started} fallback={t("just now")} />
                </>
              ) : (
                what(status)
              )}
            </Text>
          </div>
        </div>

        <Item
          className="w-full"
          variant="surface"
          icon={<Server />}
          title={
            live ? (
              <span className="inline-flex items-center gap-1.5">
                {selected && <Flag country={selected.country} />}
                {/* Show the group and its exit: AUTO → Poland 1. */}
                {route.join(" → ")}
                {more > 0 && ` +${more}`}
              </span>
            ) : direction === "direct" ? (
              t("Direct connection")
            ) : (
              t("No exit selected")
            )
          }
          description={live ? why : t(WAITING[direction])}
          meta={
            live &&
            selected &&
            selected.delay !== null && (
              <Badge size="sm" tone={delayTone(selected)} className="rk-num">
                {api.delayLabel(selected)}
              </Badge>
            )
          }
        />

        <div className="flex w-full flex-col gap-2">
          {/* Proxy requires the address to be set in apps (D-008). Reuse the label row. */}
          <Field
            label={t("Capture")}
            aside={
              running !== null && running !== mode ? (
                <Badge size="sm" tone="warn" dot>
                  {t("restart pending")}
                </Badge>
              ) : (
                address && (
                  <span className="inline-flex items-center gap-1">
                    <span className="rk-mono rk-num text-(length:--rk-text-xs)">{address}</span>
                    <CopyButton
                      size="sm"
                      variant="ghost"
                      value={address}
                      label={t("Copy proxy address")}
                    />
                  </span>
                )
              )
            }
          >
            <SegmentedControl
              aria-label={t("Capture")}
              fill
              options={MODES.map((item) => ({ ...item, hint: t(item.hint) }))}
              value={mode}
              disabled={busy}
              onChange={onMode}
            />
          </Field>
          <Field label={t("Route")}>
            <SegmentedControl
              aria-label={t("Route")}
              fill
              options={DIRECTIONS.map((item) => ({ ...item, hint: t(item.hint) }))}
              value={direction}
              disabled={busy}
              onChange={onDirection}
            />
          </Field>
        </div>
      </div>
    </Card>
  );
});
