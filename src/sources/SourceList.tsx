import { Plus, Rss } from "lucide-react";
import { useCallback, useState } from "react";
import { Button, EmptyState, IconButton, Text } from "rootik";
import * as api from "../api";
import type { Drafts } from "../config/draft";
import { useNow } from "../hooks/useNow";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import AddMenu, { type AddKind } from "./AddMenu";
import SourceCard from "./SourceCard";
import SourceCodeDialog from "./SourceCodeDialog";

type Props = {
  sources: api.Source[];
  /// Узлы всех источников — из опроса «Соединения»: состав источника виден без второго запроса.
  nodes: api.Node[];
  /// Режим «на людях» (D-127): имена подписок и адреса под точками.
  hidden: boolean;
  schedule: api.Refresh;
  drafts: Drafts;
  onDraft: (id: string, text: string) => void;
  onDisk: (id: string, text: string) => void;
  onChanged: () => Promise<void>;
  onAdd: (kind: AddKind) => void;
  onMessage: (message: Message) => void;
};

/// Когда подпишется следующая подписка. Считается здесь: срок у каждого источника свой
/// и отсчитывается от его же `updated` — всё, из чего он складывается, окно уже держит.
function nextRefresh(sources: api.Source[], schedule: api.Refresh, now: number): string {
  if (schedule.everyMinutes <= 0) return t("automatic refresh is off");
  const due = sources
    .map((source) => (source.updated ?? 0) + schedule.everyMinutes * 60)
    .sort((a, b) => a - b)[0];
  if (due === undefined) return t("no subscriptions to refresh");
  const left = Math.round((due * 1000 - now) / 60000);
  if (left <= 0) return t("next refresh is due now");
  if (left < 60) return t("next refresh in {n} min", { n: left });
  return t("next refresh in {hours} h {minutes} min", {
    hours: Math.floor(left / 60),
    minutes: left % 60,
  });
}

/// Источники mihomo — вкладка карточки справа в «Соединении» (D-160, D-172): откуда узлы
/// и как часто они обновляются. Заголовок и «скрыть» — у карточки, общие на три вкладки.
export default function SourceList({
  sources,
  nodes,
  hidden,
  schedule,
  drafts,
  onDraft,
  onDisk,
  onChanged,
  onAdd,
  onMessage,
}: Props) {
  const [busy, setBusy] = useState<string | null>(null);
  /// Раскрыт один источник за раз: раскрытые все сразу — снова простыня.
  const [opened, setOpened] = useState<string | null>(null);
  /// Чей код открыт окном.
  const [coding, setCoding] = useState<string | null>(null);
  const now = useNow(30_000);

  /// Одно действие над источником: занятость, перечитать после, отказ — баннером.
  const act = useCallback(
    async (id: string, run: () => Promise<Message | null>) => {
      setBusy(id);
      try {
        const message = await run();
        await onChanged();
        if (message) onMessage(message);
      } catch (e) {
        onMessage(failure(e));
      } finally {
        setBusy(null);
      }
    },
    [onChanged, onMessage],
  );

  const refresh = (id: string) =>
    act(id, async () => {
      const result = await api.sourcesRefresh(id);
      return notice(api.importSummary(result), result.notices);
    });
  /// Узел «своих» уходит сразу (D-121): завести его снова — то же одно окно.
  const dropNode = (id: string, node: string) =>
    act(id, async () => {
      await api.nodesDelete(id, node);
      return null;
    });
  /// Предупреждение приходит, когда на источник ссылается ваш набор: ядро на ссылку
  /// в никуда отвечает отказом стартовать, и узнать об этом при подключении поздно.
  const remove = (id: string) =>
    act(id, async () => {
      const warning = await api.sourcesDelete(id);
      return warning ? notice(warning) : null;
    });

  const code = sources.find((source) => source.id === coding);

  return (
    <>
      <div className="flex flex-col gap-2">
        <div className="flex shrink-0 flex-wrap items-center gap-1.5">
          {/* Расписание — в «Настройках» → «Обновление», рядом с флагами и проверкой групп. */}
          <Text tone="muted" size="xs">
            {nextRefresh(
              sources.filter((source) => source.url !== null),
              schedule,
              now,
            )}
          </Text>
          <span className="flex-1" />
          <AddMenu
            onPick={onAdd}
            trigger={
              <IconButton size="sm" variant="ghost" icon={<Plus />} label={t("Add source")} />
            }
          />
        </div>
        <div className="flex flex-col gap-2">
          {sources.length === 0 ? (
            <EmptyState
              icon={<Rss />}
              title={t("No sources")}
              hint={t("Add a subscription, a file or a custom node.")}
              action={
                <AddMenu
                  onPick={onAdd}
                  trigger={<Button variant="primary">{t("Add source")}</Button>}
                />
              }
            />
          ) : (
            sources.map((item) => (
              <SourceCard
                key={item.id}
                source={item}
                hidden={hidden}
                nodes={nodes.filter((node) => node.source === item.id)}
                open={opened === item.id}
                busy={busy === item.id}
                onToggle={() => setOpened(opened === item.id ? null : item.id)}
                onRefresh={() => refresh(item.id)}
                onRemove={() => remove(item.id)}
                onDropNode={(node) => dropNode(item.id, node)}
                onCode={() => setCoding(item.id)}
              />
            ))
          )}
        </div>
      </div>
      {code && (
        <SourceCodeDialog
          source={code}
          hidden={hidden}
          drafts={drafts}
          onDraft={onDraft}
          onDisk={onDisk}
          onChanged={onChanged}
          onMessage={onMessage}
          onClose={() => setCoding(null)}
        />
      )}
    </>
  );
}
