import { Download, Gauge } from "lucide-react";
import { useCallback, useState } from "react";
import { Button, NumberInput } from "rootik";
import Page, { type Group } from "../config/Page";
import { useCached } from "../hooks/useCached";
import { unchanged, usePoll } from "../hooks/usePoll";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import SectionBar from "../shell/SectionBar";
import * as qd from "./api";

type Props = {
  live: boolean;
  start: React.ReactNode;
  onInstall: () => Promise<void>;
  onMessage: (message: Message | null) => void;
};

export default function QdSettings({ live, start, onInstall, onMessage }: Props) {
  const [settings, setSettings] = useCached<qd.Settings | null>("qd.settings", null);
  const [updating, setUpdating] = useState(false);

  usePoll(() => {
    qd.settings().then(unchanged(setSettings), () => {});
  }, live);

  const save = useCallback(
    async (patch: Partial<Pick<qd.Settings, "fixedRate">>) => {
      try {
        setSettings(await qd.saveSettings(patch));
      } catch (e) {
        onMessage(failure(e));
      }
    },
    [onMessage],
  );

  const groups: Group[] = [
    {
      id: "qd-transport",
      label: t("qd transport"),
      icon: Gauge,
      parts: [
        {
          id: "qd-rate",
          label: t("Fixed send rate"),
          settings: [
            {
              id: "qd-rate-value",
              label: t("Fixed send rate"),
              hint: t(
                "0 lets qd pick the rate itself. A fixed rate ignores loss — set it only on a line you know.",
              ),
              control: (
                <NumberInput
                  size="sm"
                  aria-label={t("Fixed send rate")}
                  min={0}
                  max={10000}
                  unit={t("Mbit/s")}
                  disabled={!settings}
                  value={settings?.fixedRate ?? 0}
                  onChange={(value) => save({ fixedRate: value ?? 0 })}
                />
              ),
            },
          ],
        },
      ],
    },
    {
      id: "qd-binary",
      label: t("qd binary"),
      icon: Download,
      parts: [
        {
          id: "qd-update",
          label: t("Update qd"),
          settings: [
            {
              id: "qd-update-now",
              label: t("Update qd"),
              hint: t(
                "Downloads the latest qd release and checks it against the published checksum. The connection drops for a moment.",
              ),
              control: (
                <Button
                  icon={<Download />}
                  loading={updating}
                  onClick={async () => {
                    setUpdating(true);
                    try {
                      await onInstall();
                      onMessage(notice(t("qd updated")));
                    } finally {
                      setUpdating(false);
                    }
                  }}
                >
                  {t("Update")}
                </Button>
              ),
            },
          ],
        },
      ],
    },
  ];

  return <Page groups={groups} bar={<SectionBar start={start} hint={t("qd client settings")} />} />;
}
