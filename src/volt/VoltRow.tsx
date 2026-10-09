import { Settings2 } from "lucide-react";
import { IconButton, Switch, Text, type TextTone } from "rootik";
import type * as api from "../api";
import { locale, t } from "../i18n";
import type { VoltController } from "./useVolt";

type Snapshot = NonNullable<VoltController["snapshot"]>;

/**
 * VOLT in the Connection card (D-182, D-186): one switch for the bypass, the line under it
 * says what stands in the way, or else the mode (D-187); the gear opens the full settings.
 */
export default function VoltRow({
  volt,
  disabled,
  running,
  direction,
  onSettings,
}: {
  volt: VoltController;
  disabled: boolean;
  /** The core is running: only then can Relay be missing. */
  running: boolean;
  direction: api.Direction;
  onSettings: () => void;
}) {
  const snapshot = volt.snapshot;
  const options = snapshot?.options;
  const line = snapshot
    ? volt.busy
      ? { tone: "muted" as const, text: t("applying…") }
      : (trouble(snapshot, running) ?? { tone: "muted" as const, text: mode(snapshot, direction) })
    : { tone: "muted" as const, text: "…" };
  return (
    <div className="flex items-center gap-2">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-sm">{t("Bypass")}</span>
        <Text
          tone={line.tone}
          size="sm"
          truncate
          title={[line.text, snapshot && activity(snapshot)].filter(Boolean).join(" · ")}
        >
          {line.text}
        </Text>
      </div>
      <IconButton
        icon={<Settings2 />}
        label={t("VOLT settings")}
        size="sm"
        variant="ghost"
        onClick={onSettings}
      />
      <Switch
        aria-label={t("Bypass")}
        checked={options?.directEnabled ?? false}
        disabled={disabled || volt.busy || !options}
        onChange={(event) => {
          if (options) void volt.save({ ...options, directEnabled: event.target.checked });
        }}
      />
    </div>
  );
}

/** What running Relay has done so far (D-188): proof the bypass works, not just runs. */
export function activity(snapshot: Snapshot): string | null {
  const stats = snapshot.relayStats;
  if (!stats) return null;
  const auto = stats.autoDirect + stats.autoBypassed;
  return [
    t("connections: {n}", { n: stats.connections }),
    t("packets changed: {n}", { n: stats.modified }),
    auto > 0 &&
      t("directly: {direct}, bypassed: {bypassed}", {
        direct: stats.autoDirect,
        bypassed: stats.autoBypassed,
      }),
    t("errors: {n}", { n: stats.failures }),
  ]
    .filter(Boolean)
    .join(" · ");
}

/** What keeps the bypass from working, first; nothing — the mode can be shown. */
export function trouble(
  snapshot: Snapshot,
  running: boolean,
): { tone: TextTone; text: string } | null {
  const { options } = snapshot;
  if (!options.directEnabled && !options.vpnEnabled) return null;
  if (!snapshot.available) return { tone: "warn", text: t("VOLT files are not downloaded") };
  if (!snapshot.elevated) return { tone: "warn", text: t("needs administrator rights") };
  if (options.directEnabled && snapshot.relayError)
    return { tone: "danger", text: t("Relay did not start: {why}", { why: snapshot.relayError }) };
  if (options.directEnabled && running && !snapshot.relayRunning)
    return { tone: "danger", text: t("Relay is not running") };
  return null;
}

/** The mode in the words of the settings page: lists and what is in them, or all DIRECT and how. */
function mode(snapshot: Snapshot, direction: api.Direction): string {
  const { options } = snapshot;
  if (!options.directEnabled) return options.vpnEnabled ? t("off · proxy traffic only") : t("off");
  if (options.scope === "direct") {
    if (direction !== "direct") return t("all DIRECT traffic · idle: the exit is not DIRECT");
    return options.mode === "auto"
      ? t("all DIRECT traffic · auto")
      : t("all DIRECT traffic · always");
  }
  const english = !locale().startsWith("ru");
  const names = snapshot.services
    .filter((service) => options.services.includes(service.id))
    .map((service) => (english && service.title_en) || service.title);
  if (options.domains.length) names.push(t("your domains"));
  return names.length ? t("lists · {list}", { list: names.join(", ") }) : t("nothing selected");
}
