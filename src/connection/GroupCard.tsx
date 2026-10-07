import { ChevronDown, ChevronUp, Power } from "lucide-react";
import { Badge, Card, Editable, IconButton, RollingNumber, Text } from "rootik";
import type * as api from "../api";
import { GROUP_KIND, UNKNOWN_KIND } from "../config/kinds";
import type { NodeSpeed } from "../hooks/useTraffic";
import { t, tn } from "../i18n";
import Flag from "../shell/Flag";
import GroupIcon from "./GroupIcon";
import type { GroupEntry } from "./groups";
import { load } from "./groups";
import IconPicker from "./IconPicker";
import NodeTiles from "./NodeTiles";

/// Flags in a folded card: enough to recognise the group, not a list.
const FLAGS = 5;

type Props = {
  entry: GroupEntry;
  label: string;
  icon: string | null;
  open: boolean;
  /// The group is the exit, or MATCH points at it (D-166).
  active: boolean;
  nodes: api.Node[];
  rates: Record<string, NodeSpeed>;
  hidden: boolean;
  selected: string | null;
  onToggle: () => void;
  onChoose: (name: string) => void;
  onConnect: () => void;
  running: boolean;
  onIcon: (id: string | null) => void;
  /// Only own groups are renamed: client group names belong to the build.
  onRename?: (name: string) => void;
  onEdit: (node: string) => void;
};

/// One group of the "Groups" tab (D-172): folded — icon, name, flags, size and kind;
/// open — the members as tiles with their load.
export default function GroupCard({
  entry,
  label,
  icon,
  open,
  active,
  nodes,
  rates,
  hidden,
  selected,
  onToggle,
  onChoose,
  onConnect,
  running,
  onIcon,
  onRename,
  onEdit,
}: Props) {
  const kind = GROUP_KIND[entry.kind] ?? UNKNOWN_KIND;
  const inside = entry.members
    .map((name) => nodes.find((node) => node.name === name))
    .filter((node): node is api.Node => node !== undefined);
  const others = entry.members.filter((name) => !inside.some((node) => node.name === name));
  const countries = [...new Set(inside.map((node) => node.country).filter(Boolean))] as string[];
  const { lead, connections } = load(entry.members, rates);
  const carrier = inside.find((node) => node.name === lead);
  const toggle = (
    <IconButton
      size="sm"
      variant="ghost"
      icon={open ? <ChevronUp /> : <ChevronDown />}
      label={open ? t("Fold") : t("Show nodes")}
      aria-expanded={open}
      onClick={onToggle}
    />
  );

  return (
    <Card
      data-group={entry.name}
      variant="outline"
      padding="sm"
      selected={active}
      dense
      className={`select-none ${open ? "col-span-full" : ""}`}
      icon={open ? undefined : <GroupIcon id={icon} fallback={kind.Icon} />}
      // The picker is a button: in `media`, not in the title — the title is a heading, and
      // its name would read out the whole picker.
      media={
        open && (
          <IconPicker
            value={icon}
            fallback={kind.Icon}
            label={t("Icon of {name}", { name: label })}
            onChange={onIcon}
          />
        )
      }
      title={
        open && onRename ? (
          <Editable value={entry.name} label={t("Rename group")} onChange={onRename} />
        ) : (
          label
        )
      }
      description={`${entry.kind} · ${tn(entry.members.length, "{n} node", "{n} nodes")}`}
      actions={
        <span className="flex items-center gap-2">
          {/* Щелчок по карточке выбирает, а эта кнопка ещё и включает VPN; у подключённой
              группы она нажата. */}
          <IconButton
            size="sm"
            variant="ghost"
            icon={<Power />}
            active={active && running}
            label={active && running ? t("Connected through it") : t("Connect through it")}
            onClick={onConnect}
          />
          {toggle}
        </span>
      }
      // A click picks the group as the exit; one on a node tile picks the node (D-172).
      onAction={() => onChoose(entry.name)}
    >
      {open ? (
        <div className="um-swap flex flex-col gap-3">
          {lead !== null && (
            <div className="flex items-center justify-between gap-2">
              <Text tone="muted" size="xs" className="flex items-center gap-1.5">
                {t("Traffic through")}
                <Flag country={carrier?.country ?? null} />
                <Text size="xs">{lead}</Text>
              </Text>
              <Text size="xs">
                <RollingNumber value={connections} />{" "}
                <Text tone="muted" size="xs">
                  {tn(connections, "connection", "connections")}
                </Text>
              </Text>
            </div>
          )}
          <NodeTiles
            nodes={inside}
            selected={selected}
            hidden={hidden}
            rates={rates}
            onSelect={onChoose}
            onEdit={onEdit}
          />
          {others.length > 0 && (
            <span className="flex flex-wrap gap-1">
              {others.map((name) => (
                <Badge key={name} size="sm" variant="outline">
                  {name}
                </Badge>
              ))}
            </span>
          )}
        </div>
      ) : countries.length > 0 ? (
        <span className="flex items-center gap-1">
          {countries.slice(0, FLAGS).map((country) => (
            <Flag key={country} country={country} />
          ))}
          {countries.length > FLAGS && (
            <Text tone="muted" size="xs">
              +{countries.length - FLAGS}
            </Text>
          )}
        </span>
      ) : undefined}
    </Card>
  );
}
