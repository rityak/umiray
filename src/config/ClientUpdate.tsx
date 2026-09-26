import { Download, RefreshCw } from "lucide-react";
import { Button, ConfirmButton, Progress, Text } from "rootik";
import type { UpdateInfo, UpdateProgress } from "../api";
import { formatBytes } from "../api";
import { t } from "../i18n";

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
      <Text tone="muted" size="sm">
        {progress?.phase === "install"
          ? t("Installing… The client will restart.")
          : progress
            ? t("Downloading update…")
            : info?.version
              ? t("umiray {version} is available", { version: info.version })
              : info?.enabled === false
                ? t("Updates are unavailable in this build.")
                : info
                  ? t("You have the latest version.")
                  : t("Checks on launch. Installation requires your confirmation.")}
      </Text>
      {progress ? (
        <Progress
          aria-label={t("Client updates")}
          value={progress.total ? progress.downloaded : undefined}
          max={progress.total ?? undefined}
          showValue={formatBytes(progress.downloaded)}
        />
      ) : (
        <div className="flex flex-wrap gap-2">
          <Button icon={<RefreshCw />} loading={checking} disabled={busy} onClick={onCheck}>
            {t("Check for updates")}
          </Button>
          {info?.version && (
            <ConfirmButton
              icon={<Download />}
              variant="primary"
              disabled={busy || checking}
              confirmLabel={t("Disconnect VPN and install?")}
              onConfirm={onInstall}
            >
              {t("Install update")}
            </ConfirmButton>
          )}
        </div>
      )}
      {info?.notes && (
        <Text className="selectable whitespace-pre-wrap" size="sm">
          {info.notes}
        </Text>
      )}
    </div>
  );
}
