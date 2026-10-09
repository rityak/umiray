import { Download, RefreshCw } from "lucide-react";
import { Button, Code, ConfirmButton, Progress, Text } from "rootik";
import type { UpdateInfo, UpdateProgress } from "../api";
import { formatBytes, VERSION } from "../api";
import { has } from "../features";
import { t } from "../i18n";
import ReleaseNotes from "./ReleaseNotes";

type Props = {
  info: UpdateInfo | null;
  checking: boolean;
  progress: UpdateProgress | null;
  busy: boolean;
  onCheck: () => void;
  onInstall: () => void;
};

export default function ClientUpdate({
  info,
  checking,
  progress,
  busy,
  onCheck,
  onInstall,
}: Props) {
  return (
    <div className="flex flex-col gap-2" aria-live="polite">
      <div className="flex flex-wrap items-center gap-2">
        <Code className="selectable" aria-label={t("Client version")}>
          {VERSION}
        </Code>
        {!progress && (
          <div className="flex flex-wrap gap-2">
            <Button icon={<RefreshCw />} loading={checking} disabled={busy} onClick={onCheck}>
              {t("Check for updates")}
            </Button>
            {/* Поставленный чужим менеджером пакетов (Arch) обновляет он же (D-174). */}
            {info?.version && has("selfUpdate") && (
              <ConfirmButton
                icon={<Download />}
                variant="primary"
                disabled={busy || checking}
                confirmLabel={t("Disconnect and install?")}
                onConfirm={onInstall}
              >
                {t("Install update")}
              </ConfirmButton>
            )}
          </div>
        )}
      </div>
      <Text tone="muted" size="sm">
        {progress?.phase === "install"
          ? t("Installing… The client will restart.")
          : progress
            ? t("Downloading update…")
            : info?.version
              ? has("selfUpdate")
                ? t("umiray {version} is available", { version: info.version })
                : t("umiray {version} is available — update it with your package manager", {
                    version: info.version,
                  })
              : info?.enabled === false
                ? t("Updates are unavailable in this build.")
                : info
                  ? t("You have the latest version.")
                  : t("Checks on launch; installs only when you say so.")}
      </Text>
      {progress && (
        <Progress
          aria-label={t("Client updates")}
          value={progress.total ? progress.downloaded : undefined}
          max={progress.total ?? undefined}
          showValue={formatBytes(progress.downloaded)}
        />
      )}
      {info?.notes && <ReleaseNotes notes={info.notes} />}
    </div>
  );
}
