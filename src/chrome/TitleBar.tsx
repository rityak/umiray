import { getCurrentWindow } from "@tauri-apps/api/window";
import { FlaskConical, Palette, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import {
  TitleBar as Bar,
  IconButton,
  type Tone as KitTone,
  SegmentedControl,
  StatusDot,
} from "rootik";
import { type Status, statusView, type Tone } from "../api";
import { t } from "../i18n";
import type * as qd from "../qd/api";
import Uptime from "../shell/Uptime";

// A plain browser serves as a responsive-layout bench. It has no Tauri window, so the
// window buttons simply do nothing there.
const appWindow = (() => {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
})();

type Props = {
  status: Status;
  powering: boolean;
  engine: "mihomo" | "qd";
  onEngine: (engine: "mihomo" | "qd") => void;
  qd: qd.Status | null;
  onAdd: () => void;
  onSettings: () => void;
  onDev?: () => void;
  dev?: boolean;
};

const TONE: Record<Tone, KitTone> = {
  on: "success",
  connecting: "warn",
  off: "neutral",
  error: "danger",
};

/** The window frame (D-141, D-142). VPN controls live in the connection path. */
export default function TitleBar({
  status,
  powering,
  engine,
  onEngine,
  qd: qdStatus,
  onAdd,
  onSettings,
  onDev,
  dev,
}: Props) {
  const state = qdStatus?.state ?? null;
  const view: { tone: Tone; label: string } =
    engine === "qd"
      ? powering
        ? { tone: "connecting", label: t("Starting…") }
        : state?.failed
          ? { tone: "error", label: t("qd connection failed") }
          : state?.connected
            ? { tone: "on", label: `${t("Connected")} · qd` }
            : { tone: "off", label: t("Disconnected") }
      : statusView(status, powering);
  const maximized = useMaximized();

  return (
    <Bar
      start={
        <div className="flex items-center gap-3" data-tauri-drag-region>
          <img
            src="/ray-mark.svg"
            width={28}
            height={28}
            alt="umiray"
            draggable={false}
            data-tauri-drag-region
          />
          <span role="status" className="flex min-w-0 flex-col" data-tauri-drag-region>
            <StatusDot
              tone={TONE[view.tone]}
              pulse={view.tone === "connecting"}
              label={view.label}
            />
            <span className="truncate text-xs text-(--rk-text-2) max-[819px]:hidden">
              {engine === "qd" ? (
                (state?.connected && state.node?.name) || t("qd stopped")
              ) : status.running ? (
                <Uptime started={status.started} fallback={t("just now")} />
              ) : (
                t("core stopped")
              )}
            </span>
          </span>
        </div>
      }
      end={
        <>
          <SegmentedControl
            size="sm"
            aria-label={t("Engine")}
            options={[
              { value: "mihomo", label: "mihomo" },
              { value: "qd", label: "qd" },
            ]}
            value={engine}
            onChange={onEngine}
          />
          <IconButton
            icon={<Plus />}
            label={t("Add a subscription or a link")}
            variant="ghost"
            className="max-[819px]:hidden"
            onClick={onAdd}
          />
          <IconButton
            icon={<Palette />}
            label={t("Appearance")}
            variant="ghost"
            onClick={onSettings}
          />
          {onDev && (
            <IconButton
              icon={<FlaskConical />}
              label={t("Developer mode")}
              variant="ghost"
              active={dev}
              onClick={onDev}
            />
          )}
        </>
      }
      maximized={maximized}
      onMinimize={() => appWindow?.minimize()}
      onMaximize={() => appWindow?.toggleMaximize()}
      onClose={() => appWindow?.close()}
    />
  );
}

/// Whether the window is maximized: the frame shows the "Restore" caption and icon from it.
function useMaximized() {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    if (!appWindow) return;
    const read = () => appWindow.isMaximized().then(setMaximized, () => {});
    read();
    const off = appWindow.onResized(read);
    return () => {
      off.then((stop) => stop());
    };
  }, []);
  return maximized;
}
