import { Plus } from "lucide-react";
import { useEffect, useMemo } from "react";
import { Button } from "rootik";
import * as api from "../api";
import { useCached } from "../hooks/useCached";
import type { NodeSpeed } from "../hooks/useTraffic";
import { t } from "../i18n";
import { failure, type Message } from "../shell/Banner";
import AutoGroupsMenu from "./AutoGroupsMenu";
import GroupCard from "./GroupCard";
import { activeFirst, entries, groupLabel, iconOf } from "./groups";

type Props = {
  nodes: api.Node[];
  sources: api.Source[];
  /// Groups the client builds, with members (D-172).
  built: api.BuiltGroup[];
  /// Own groups from the "Groups" document.
  user: api.Group[];
  hidden: boolean;
  rates: Record<string, NodeSpeed>;
  /// The marked exit: DIRECT, AUTO, a group or a node (D-166, D-172).
  selected: string | null;
  icons: Record<string, string>;
  onIcons: (icons: Record<string, string>) => void;
  onChoose: (name: string) => void;
  /// Выбрать группу выходом и включить VPN, если он выключен.
  onConnect: (name: string) => void;
  /// VPN включён: у выбранной группы вместо кнопки — «Подключено».
  running: boolean;
  onEdit: (node: string) => void;
  onCreate: () => void;
  onStatus: (status: api.Status) => void;
  onChanged: () => void;
  onMessage: (message: Message) => void;
};

/// «Группы» (D-172): AUTO, свои группы из раздела «Группы» и собранные клиентом. Выход
/// выбирается здесь же, не уходя в «Маршрутизацию»; активная группа раскрыта и стоит первой.
export default function GroupsView({
  nodes,
  sources,
  built,
  user,
  hidden,
  rates,
  selected,
  icons,
  onIcons,
  onChoose,
  onConnect,
  running,
  onEdit,
  onCreate,
  onStatus,
  onChanged,
  onMessage,
}: Props) {
  const [opened, setOpened] = useCached<string[]>("connection.groups.opened", []);

  /// Выбранная группа раскрывается сама — её состав и нагрузку и хотят видеть.
  useEffect(() => {
    if (selected !== null)
      setOpened((current) => (current.includes(selected) ? current : [...current, selected]));
  }, [selected]);

  const list = useMemo(
    () => activeFirst(entries(built, user, { sources, nodes }), selected),
    [built, user, sources, nodes, selected],
  );

  const rename = async (from: string, to: string) => {
    try {
      onStatus(await api.groupsRename(from, to));
      setOpened((current) => current.map((name) => (name === from ? to : name)));
      onChanged();
    } catch (e) {
      onMessage(failure(e));
    }
  };

  const setIcon = (name: string, id: string | null) => {
    const next = { ...icons };
    if (id === null) delete next[name];
    else next[name] = id;
    onIcons(next);
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <AutoGroupsMenu onStatus={onStatus} onChanged={onChanged} onMessage={onMessage} />
        <span className="flex-1" />
        <Button size="sm" icon={<Plus />} onClick={onCreate}>
          {t("Create")}
        </Button>
      </div>
      <div className="grid grid-cols-2 gap-3">
        {list.map((entry) => {
          const open = opened.includes(entry.name);
          return (
            <GroupCard
              key={entry.name}
              entry={entry}
              label={groupLabel(entry, nodes)}
              icon={iconOf(entry, icons)}
              open={open}
              active={selected === entry.name}
              nodes={nodes}
              rates={rates}
              hidden={hidden}
              selected={selected}
              onToggle={() =>
                setOpened((current) =>
                  open ? current.filter((name) => name !== entry.name) : [...current, entry.name],
                )
              }
              onChoose={onChoose}
              onConnect={() => onConnect(entry.name)}
              running={running}
              onIcon={(id) => setIcon(entry.name, id)}
              onRename={entry.own ? (to) => rename(entry.name, to) : undefined}
              onEdit={onEdit}
            />
          );
        })}
      </div>
    </div>
  );
}
