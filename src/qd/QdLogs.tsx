import { ScrollText } from "lucide-react";
import { useMemo } from "react";
import { Card, EmptyState, type LogLine, LogView } from "rootik";
import { useCached } from "../hooks/useCached";
import { unchanged, usePoll } from "../hooks/usePoll";
import { t } from "../i18n";
import * as qd from "./api";

export default function QdLogs({ hidden }: { hidden: boolean }) {
  const [raw, setRaw] = useCached<string[]>("qd.logs", []);

  usePoll(() => {
    qd.logs().then(unchanged(setRaw), () => {});
  }, !hidden);

  const lines = useMemo<LogLine[]>(() => raw.map((message) => ({ message, source: "qd" })), [raw]);

  if (hidden) {
    return (
      <Card>
        <EmptyState
          title={t("Log hidden")}
          hint={t("Turn off private mode: lines may contain node addresses and names.")}
        />
      </Card>
    );
  }
  if (raw.length === 0) {
    return (
      <Card>
        <EmptyState
          icon={<ScrollText />}
          title={t("Empty")}
          hint={t("qd has not run yet — nothing to show.")}
        />
      </Card>
    );
  }
  return (
    <Card padding="none" className="flex min-h-0 flex-1 flex-col">
      <LogView lines={lines} className="min-h-0 flex-1" label={t("qd log")} />
    </Card>
  );
}
