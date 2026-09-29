import { useCallback } from "react";
import * as api from "../api";
import { QD_SETTINGS } from "../engines";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import QdConnection from "./QdConnection";
import QdRouting from "./QdRouting";
import QdSettings from "./QdSettings";
import QdSource from "./QdSource";
import type { Qd } from "./useQd";

type Own = { id: string; render: (start: React.ReactNode) => React.ReactNode };

type Props = {
  /// The open section: a fixed one (`connection`, `sources`) or a config section.
  tab: string;
  section: api.ConfigSection | undefined;
  qd: Qd;
  status: api.Status;
  powering: boolean;
  onPower: () => void;
  hidden: boolean;
  onHidden: () => void;
  focus: boolean;
  onFocused: () => void;
  onAdd: () => void;
  onElevate: () => void;
  onMessage: (message: Message | null) => void;
  /// The shared config editor (client settings live there for every engine), with qd's own
  /// document added to it.
  config: (own: Own) => React.ReactNode;
};

/// What qd shows in the sections (D-154). `App` picks it once; nothing outside `src/qd`
/// knows how qd's tabs are built.
export default function QdSection({
  tab,
  section,
  qd,
  status,
  powering,
  onPower,
  hidden,
  onHidden,
  focus,
  onFocused,
  onAdd,
  onElevate,
  onMessage,
  config,
}: Props) {
  const install = useCallback(async () => {
    onMessage(null);
    try {
      const version = await api.coreInstall("qd");
      onMessage(notice(t("{engine} {version} downloaded", { engine: "qd", version })));
    } catch (e) {
      onMessage(failure(e));
    } finally {
      qd.reload();
    }
  }, [onMessage, qd]);

  const live = Boolean(qd.status?.state);

  if (tab === "connection") {
    return (
      <QdConnection
        status={qd.status}
        started={status.active === "qd" ? status.started : null}
        other={status.active !== "qd" ? status.active : null}
        powering={powering}
        onPower={onPower}
        onChanged={qd.reload}
        onInstall={install}
        onElevate={onElevate}
        onAdd={onAdd}
        onMessage={onMessage}
      />
    );
  }
  if (tab === "sources") {
    return (
      <QdSource
        state={qd.status?.state ?? null}
        hidden={hidden}
        onHidden={onHidden}
        focus={focus}
        onFocused={onFocused}
        onChanged={qd.reload}
        onMessage={onMessage}
      />
    );
  }
  if (section?.id === "rules") return <QdRouting live={live} onMessage={onMessage} />;
  if (section) {
    return config({
      id: QD_SETTINGS,
      render: (start) => (
        <QdSettings live={live} start={start} onInstall={install} onMessage={onMessage} />
      ),
    });
  }
  return null;
}
