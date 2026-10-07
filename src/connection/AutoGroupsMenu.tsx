import { Sparkles } from "lucide-react";
import { useState } from "react";
import { Button, Popover, Switch } from "rootik";
import * as api from "../api";
import { t } from "../i18n";
import { failure, type Message } from "../shell/Banner";

type Props = {
  onStatus: (status: api.Status) => void;
  onChanged: () => void;
  onMessage: (message: Message) => void;
};

type State = { grouping: api.Grouping; udp: api.Udp };

/// «Автогруппы» (D-172): какие группы клиент собирает сам — по стране, по протоколу и из узлов
/// с UDP без TCP (D-113). Группа трафик не уводит; правило «весь UDP туда» — в настройках
/// клиента, и оно включает эту группу, а снятая группа снимает его.
export default function AutoGroupsMenu({ onStatus, onChanged, onMessage }: Props) {
  const [state, setState] = useState<State | null>(null);
  const [busy, setBusy] = useState(false);

  const read = async () => {
    try {
      const [grouping, udp] = await Promise.all([api.groupsAutoGet(), api.udpGet()]);
      setState({ grouping, udp });
    } catch (e) {
      onMessage(failure(e));
    }
  };

  const apply = async (run: () => Promise<api.Status>) => {
    setBusy(true);
    try {
      onStatus(await run());
      await read();
      onChanged();
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setBusy(false);
    }
  };

  const grouping = state?.grouping;
  return (
    <Popover
      title={t("Auto groups")}
      onOpenChange={(open) => open && read()}
      trigger={
        <Button size="sm" icon={<Sparkles />}>
          {t("Auto groups")}
        </Button>
      }
    >
      <div className="flex w-72 flex-col gap-3">
        <Switch
          labelPosition="start"
          label={t("By location")}
          description={t("A group for every country with two or more nodes")}
          checked={grouping?.location ?? false}
          disabled={grouping === undefined || busy}
          onChange={(event) =>
            grouping &&
            apply(() => api.groupsAutoSet({ ...grouping, location: event.target.checked }))
          }
        />
        <Switch
          labelPosition="start"
          label={t("By protocol")}
          description={t("A group for every protocol with two or more nodes")}
          checked={grouping?.protocol ?? false}
          disabled={grouping === undefined || busy}
          onChange={(event) =>
            grouping &&
            apply(() => api.groupsAutoSet({ ...grouping, protocol: event.target.checked }))
          }
        />
        <Switch
          labelPosition="start"
          label={t("UDP over UDP")}
          description={
            state?.udp.nodes === 0
              ? t("No nodes carry UDP as datagrams — nothing to group")
              : state?.udp.on
                ? t(
                    "Nodes whose protocol carries UDP itself, not inside TCP. All UDP goes here: a rule in Settings",
                  )
                : t("Nodes whose protocol carries UDP itself, not inside TCP")
          }
          checked={grouping?.udp ?? false}
          disabled={grouping === undefined || state?.udp.nodes === 0 || busy}
          onChange={(event) =>
            grouping && apply(() => api.groupsAutoSet({ ...grouping, udp: event.target.checked }))
          }
        />
      </div>
    </Popover>
  );
}
