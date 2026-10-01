import { AppWindow, MonitorCog, Network, Server } from "lucide-react";
import { memo } from "react";
import { Badge, Card, CopyButton, Field, Item, SegmentedControl } from "rootik";
import * as api from "../api";
import { t, tk } from "../i18n";
import Flag from "../shell/Flag";
import { hide } from "../shell/secret";
import Uptime from "../shell/Uptime";
import { delayTone } from "./NodeDelay";
import PowerRow from "./PowerRow";

type Props = {
  status: api.Status;
  mode: api.Choice;
  running: api.Choice | null;
  direction: api.Direction;
  /// Where routing sends the rest when MATCH is not the choice (D-166).
  fallback: string | null;
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
};

/// Shared with the setup wizard (D-162): one wording for the same choice. `hint` fits the
/// switch here; `about` is what the wizard says to someone choosing for the first time.
export const MODES = [
  {
    value: "local" as const,
    label: "Proxy",
    hint: tk("set the address in your app"),
    about: tk(
      "Only apps where you enter the umiray address go through VPN. Everything else goes direct.",
    ),
    icon: <AppWindow />,
  },
  {
    value: "system" as const,
    label: "System",
    hint: tk("Windows proxy"),
    about: tk(
      "umiray becomes the Windows proxy. Browsers and most apps pick it up, but games and some programs don't.",
    ),
    icon: <MonitorCog />,
  },
  {
    value: "tun" as const,
    label: "TUN",
    hint: tk("all device traffic"),
    about: tk(
      "All traffic on this computer goes through VPN, games and UDP included. No setup in apps.",
    ),
    icon: <Network />,
  },
];

/// Explain routing while no exit is known.
const WAITING: Record<api.Direction, string> = {
  direct: tk("bypass VPN — no server"),
  // AUTO — `load-balance` по живым узлам (D-053): «лучший» был бы неправдой —
  // медленный узел получает сайты наравне с быстрым.
  auto: tk("the core spreads sites across working servers"),
  manual: tk("select a node from the list"),
};

/// Explain how the current exit was selected.
const CHOSEN: Record<api.Direction, string> = {
  direct: tk("bypass VPN"),
  auto: tk("selected automatically"),
  manual: tk("selected manually"),
};

/// Capture guarantees differ between TUN, System and Proxy.
function what(status: api.Status): string {
  const mode = api.runningMode(status);
  if (!status.corePresent) return t("core missing — download it in Settings");
  if (mode === null)
    return t("{mode} turns on with VPN", { mode: api.MODE_LABEL[status.desiredMode] });
  if (mode === "tun") return t("all device traffic goes through the adapter");
  if (mode === "system") return t("proxy set in Windows settings");
  return t("enter the address below in your browser or app");
}

/// Connection state next to the power button.
function headline(status: api.Status, powering: boolean): string {
  if (powering) return t("Connecting…");
  if (!status.corePresent) return t("Core missing");
  return status.running ? t("Connected") : t("Disconnected");
}

/**
 * Power, capture and the exit card (D-060, D-142). The exit is picked in the node list
 * (D-166).
 */
/// Traffic ticks do not affect these controls.
export default memo(function ConnectionPath({
  status,
  mode,
  running,
  direction,
  fallback,
  route,
  more,
  selected,
  total,
  hidden,
  busy,
  powering,
  onPower,
  onMode,
}: Props) {
  const view = api.statusView(status, powering);
  // System listens on the same port as Proxy: Windows points apps at it, and an app that
  // ignores the system proxy takes the address from here. Only TUN has no address.
  const address = mode !== "tun" ? api.proxyAddress(status) : null;
  // Manual persists the user's selection; other exits require a running core (D-141).
  // MATCH set in Routing is written down, so it is known without the core (D-166).
  const live =
    route.length > 0 &&
    (fallback !== null || (direction !== "direct" && (status.running || direction === "manual")));
  const how =
    fallback !== null
      ? t("assigned by your rules")
      : `${t(CHOSEN[direction])}${direction === "auto" && total > 0 ? ` ${t("out of {n}", { n: total })}` : ""}`;
  const why = [
    selected && `${selected.kind}${selected.address ? ` · ${hide(selected.address, hidden)}` : ""}`,
    how,
    !status.running && t("core is not running"),
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    // The state already serves as a heading; keep room for the chart below.
    <Card padding="sm" aria-label={t("Connection")}>
      <div className="flex flex-col gap-3">
        <PowerRow
          on={status.running}
          powering={powering}
          failed={view.tone === "error"}
          disabled={busy && !powering}
          headline={headline(status, powering)}
          detail={
            running !== null ? (
              <>
                {api.MODE_LABEL[running]} ·{" "}
                <Uptime started={status.started} fallback={t("just now")} />
              </>
            ) : (
              what(status)
            )
          }
          onPower={onPower}
        />

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
          {/* Proxy and System: the address for apps (D-008). Reuse the label row. */}
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
              options={MODES.map((item) => ({
                value: item.value,
                label: item.label,
                hint: t(item.hint),
              }))}
              value={mode}
              disabled={busy}
              onChange={onMode}
            />
          </Field>
        </div>
      </div>
    </Card>
  );
});
