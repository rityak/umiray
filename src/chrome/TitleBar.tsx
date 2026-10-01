import { getCurrentWindow } from "@tauri-apps/api/window";
import { FlaskConical, Palette, Plus, WandSparkles } from "lucide-react";
import { useEffect, useState } from "react";
import {
  TitleBar as Bar,
  IconButton,
  type Tone as KitTone,
  SegmentedControl,
  StatusDot,
} from "rootik";
import type { Engine, Tone } from "../api";
import { ENGINES, type Headline } from "../engines";
import { t } from "../i18n";
import AddMenu, { type AddKind } from "../sources/AddMenu";

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
  /// State of the running engine, in words (D-154): the window names it, the frame shows it.
  headline: Headline;
  /// The engine the sections show. A view switch, not power: the running one keeps running.
  engine: Engine;
  /// Engines on disk. One — no switch: qd shows up once it is downloaded (D-161).
  engines: Engine[];
  onEngine: (engine: Engine) => void;
  /// One add menu for every "+" (D-160).
  onAdd: (kind: AddKind) => void;
  /// Мастер настройки (D-162): тот же, что открывается при первом запуске.
  onSetup: () => void;
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
  headline: view,
  engine,
  engines,
  onEngine,
  onAdd,
  onSetup,
  onSettings,
  onDev,
  dev,
}: Props) {
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
              {view.detail}
            </span>
          </span>
        </div>
      }
      end={
        <>
          {engines.length > 1 && (
            <SegmentedControl
              size="sm"
              aria-label={t("Engine")}
              options={engines.map((id) => ({ value: id, label: ENGINES[id].label }))}
              value={engine}
              onChange={onEngine}
            />
          )}
          <AddMenu
            onPick={onAdd}
            trigger={
              <IconButton
                icon={<Plus />}
                label={t("Add a subscription or a link")}
                variant="ghost"
                className="max-[819px]:hidden"
              />
            }
          />
          <IconButton
            icon={<WandSparkles />}
            label={t("Setup wizard")}
            variant="ghost"
            onClick={onSetup}
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
