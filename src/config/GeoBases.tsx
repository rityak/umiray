import { Globe, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { Button, Card, KeyValue, Tooltip } from "rootik";
import * as api from "../api";
import { useCached } from "../hooks/useCached";
import { locale, t } from "../i18n";
import { failure, type Message } from "../shell/Banner";

type Props = {
  /// Only a running core updates its own databases.
  running: boolean;
  onMessage: (message: Message | null) => void;
};

function date(seconds: number): string {
  return new Date(seconds * 1000).toLocaleDateString(locale());
}

/**
 * The core's GeoIP/GeoSite databases (D-157): what `GEOIP`/`GEOSITE` rules and ready sets
 * like ad blocking read. Not a route line, so it sits apart from build-in, last on the page:
 * the core owns them, the window only asks.
 */
export default function GeoBases({ running, onMessage }: Props) {
  const [geo, setGeo] = useCached<api.GeoFile[]>("lists.geo", []);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.geoFiles().then(setGeo, () => setGeo([]));
  }, []);

  const update = async () => {
    setBusy(true);
    try {
      const fresh = await api.geoUpdate();
      // The core rewrites only what changed at the source (it compares hashes), so an
      // unchanged date is an answer too — say it, or the button looks broken.
      const changed = fresh
        .filter((file) => geo.find((was) => was.name === file.name)?.modified !== file.modified)
        .map((file) => file.name);
      setGeo(fresh);
      onMessage({
        tone: "info",
        text:
          changed.length > 0
            ? t("Updated: {names}", { names: changed.join(", ") })
            : t("The databases are up to date — nothing new at the source."),
        details: [],
      });
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setBusy(false);
    }
  };

  if (geo.length === 0) return null;
  return (
    <Card
      padding="sm"
      icon={<Globe />}
      title="GeoIP · GeoSite"
      description={t("what GEOIP and GEOSITE rules read")}
      actions={
        <Tooltip
          content={
            running
              ? t("The core downloads them from its geox-url.")
              : t("Turn the VPN on — the core updates its databases itself.")
          }
        >
          <Button
            size="sm"
            variant="ghost"
            icon={<RefreshCw />}
            loading={busy}
            disabled={!running || busy}
            onClick={update}
          >
            {t("Update")}
          </Button>
        </Tooltip>
      }
    >
      <KeyValue
        items={geo.map((file) => ({
          label: file.name,
          value: file.modified ? date(file.modified) : "—",
        }))}
      />
    </Card>
  );
}
