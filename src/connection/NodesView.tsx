import { Check, Pencil, Settings2, X } from "lucide-react";
import { useState } from "react";
import { Card, IconButton, Switch, Text } from "rootik";
import type * as api from "../api";
import { targetLook } from "../config/kinds";
import { useCached } from "../hooks/useCached";
import type { NodeSpeed } from "../hooks/useTraffic";
import { t } from "../i18n";
import { hide } from "../shell/secret";
import { bypassNote } from "./exits";
import { inAuto } from "./groups";
import NodeTiles from "./NodeTiles";

type Props = {
  directTarget: string;
  nodes: api.Node[];
  sources: api.Source[];
  /// Private mode (D-127): subscription names and addresses under dots.
  hidden: boolean;
  rates: Record<string, NodeSpeed>;
  /// The marked row: DIRECT, AUTO or a node (D-166).
  selected: string | null;
  /// AUTO as the build makes it — its members (D-172). Absent: no nodes yet.
  auto: api.BuiltGroup | undefined;
  exclude: api.Exclude;
  onChoose: (name: string) => void;
  onEdit: (node: string) => void;
  onRename: (source: api.Source) => void;
  onExclude: (exclude: api.Exclude) => Promise<void>;
};

/// «Узлы»: выходы клиента сверху, под ними узлы по источникам (D-172). Шестерёнка AUTO
/// включает выбор его состава — тумблеры у источника и узла и «Сохранить».
export default function NodesView({
  directTarget,
  nodes,
  sources,
  hidden,
  rates,
  selected,
  auto,
  exclude,
  onChoose,
  onEdit,
  onRename,
  onExclude,
}: Props) {
  /// Раскрытые источники помним на сессию, как вид списка (D-079). Пока их не трогали —
  /// раскрыт первый: источники приходят после первого рендера.
  const [touched, setOpened] = useCached<string[] | null>("connection.opened", null);
  const opened = touched ?? (sources[0] ? [sources[0].id] : []);
  /// Черновик состава AUTO; пусто — режим выбора выключен.
  const [draft, setDraft] = useState<api.Exclude | null>(null);
  const [saving, setSaving] = useState(false);
  const live = nodes.filter((node) => node.supported);
  const members = auto?.members ?? [];
  /// Узел, который ядро не поднимет, в AUTO не попадает никогда (D-063).
  const has = (node: api.Node) =>
    node.supported && (draft === null ? members.includes(node.name) : inAuto(node, draft));
  const chosen = live.filter(has).length;

  const toggleNode = (node: api.Node, on: boolean) =>
    setDraft((current) =>
      current === null
        ? current
        : {
            ...current,
            nodes: on
              ? current.nodes.filter((name) => name !== node.name)
              : [...current.nodes, node.name],
          },
    );
  const toggleSource = (id: string, on: boolean) =>
    setDraft((current) =>
      current === null
        ? current
        : {
            ...current,
            sources: on ? current.sources.filter((item) => item !== id) : [...current.sources, id],
          },
    );
  const save = async () => {
    if (draft === null) return;
    setSaving(true);
    try {
      await onExclude(draft);
      setDraft(null);
    } finally {
      setSaving(false);
    }
  };

  const direct = targetLook("DIRECT", []);
  const globe = targetLook("AUTO", []);
  return (
    <div className="flex flex-col gap-3">
      <div className="grid grid-cols-2 gap-3">
        <Card
          data-node="DIRECT"
          variant="outline"
          padding="sm"
          selected={selected === "DIRECT"}
          onAction={() => onChoose("DIRECT")}
          icon={<direct.Icon />}
          title={directTarget}
          description={bypassNote(directTarget) ?? t("Direct, without proxy")}
          // Через обход — видно, что он везёт (D-191): соединения выхода VOLT, как у узлов.
          actions={
            directTarget !== "DIRECT" && (
              <Text size="xs" tone="muted">
                {rates[directTarget]?.connections ?? 0} {t("conn.")}
              </Text>
            )
          }
          className="select-none"
        />
        <Card
          data-node="AUTO"
          variant="outline"
          padding="sm"
          selected={selected === "AUTO"}
          onAction={live.length > 0 ? () => onChoose("AUTO") : undefined}
          icon={<globe.Icon />}
          iconTone="accent"
          title="AUTO"
          description={
            draft === null
              ? t("{n} of {total} nodes", { n: members.length, total: live.length })
              : t("{n} chosen of {total}", { n: chosen, total: live.length })
          }
          className="select-none"
          actions={
            live.length === 0 ? undefined : draft === null ? (
              <IconButton
                size="sm"
                variant="ghost"
                icon={<Settings2 />}
                label={t("Choose nodes for AUTO")}
                onClick={() => setDraft(exclude)}
              />
            ) : (
              <span className="flex gap-1">
                <IconButton
                  size="sm"
                  variant="ghost"
                  icon={<X />}
                  label={t("Cancel")}
                  onClick={() => setDraft(null)}
                />
                {/* Ни одного узла — исключения не действуют, и AUTO молча взял бы всех. */}
                <IconButton
                  size="sm"
                  variant="primary"
                  icon={<Check />}
                  loading={saving}
                  disabled={chosen === 0}
                  label={chosen === 0 ? t("Leave at least one node in AUTO") : t("Save")}
                  onClick={save}
                />
              </span>
            )
          }
        />
      </div>

      {sources.map((source) => {
        const own = nodes.filter((node) => node.source === source.id);
        const usable = own.filter((node) => node.supported);
        const sourceOn = draft === null || !draft.sources.includes(source.id);
        const open = opened.includes(source.id);
        return (
          <Card
            key={source.id}
            variant="outline"
            padding="sm"
            collapsible
            open={open}
            onOpenChange={(next) =>
              setOpened(next ? [...opened, source.id] : opened.filter((id) => id !== source.id))
            }
            headingLevel={3}
            title={hide(source.name, hidden)}
            actions={
              <span className="flex items-center gap-1.5">
                <Text tone="muted" size="xs" className="rk-num">
                  {usable.filter(has).length}/{usable.length}
                </Text>
                <IconButton
                  size="sm"
                  variant="ghost"
                  icon={<Pencil />}
                  label={t("Rename source")}
                  onClick={() => onRename(source)}
                />
                {draft !== null && (
                  <Switch
                    size="sm"
                    aria-label={t("{name} in AUTO", { name: hide(source.name, hidden) ?? "" })}
                    checked={sourceOn}
                    onChange={(event) => toggleSource(source.id, event.target.checked)}
                  />
                )}
              </span>
            }
          >
            {own.length === 0 ? (
              <Text tone="muted" size="xs">
                {t("No nodes — refresh the source or check its response in Code.")}
              </Text>
            ) : (
              <NodeTiles
                nodes={own}
                selected={selected}
                hidden={hidden}
                rates={rates}
                onSelect={onChoose}
                onEdit={onEdit}
                auto={
                  draft === null
                    ? undefined
                    : {
                        has,
                        locked: (node) => draft.sources.includes(node.source),
                        onToggle: toggleNode,
                      }
                }
              />
            )}
          </Card>
        );
      })}
    </div>
  );
}
