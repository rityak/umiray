import { Eye, EyeOff, FilePlus2, Rss, SquarePen } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button, Card, EmptyState, Field, IconButton, NumberInput, Select, Text } from "rootik";
import * as api from "../api";
import { importSummary } from "../api";
import type { Drafts } from "../config/draft";
import { useCached } from "../hooks/useCached";
import { useNow } from "../hooks/useNow";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import SaveActions from "../shell/SaveActions";
import Scroll from "../shell/Scroll";
import SectionBar from "../shell/SectionBar";
import { hide } from "../shell/secret";
import ViewSwitch, { type View } from "../shell/ViewSwitch";
import LinkDialog from "./LinkDialog";
import NodeDialog from "./NodeDialog";
import SourceCard from "./SourceCard";
import SourceCode from "./SourceCode";

type Props = {
  sources: api.Source[];
  /// Режим «на людях» (D-127): имена подписок, их адреса и адреса серверов закрыты
  /// точками, чтобы раздел можно было показать на экране.
  hidden: boolean;
  onHidden: () => void;
  /// Просьба из шапки: нажали «+», значит курсор должен стоять в поле, а не ждать
  /// второго нажатия уже здесь.
  focus: boolean;
  /// Просьба выполнена. Без этого фокус уезжал бы в поле при каждом возврате в раздел.
  onFocused: () => void;
  schedule: api.Refresh;
  onSchedule: (schedule: api.Refresh) => void;
  drafts: Drafts;
  onDraft: (id: string, text: string) => void;
  onDisk: (id: string, text: string) => void;
  onChanged: () => Promise<void>;
  onMessage: (message: Message) => void;
};

/// Когда подпишется следующая подписка. Считается здесь, а не спрашивается у бэкенда:
/// срок у каждого источника свой и отсчитывается от его же `updated` — всё, из чего он
/// складывается, окно уже держит.
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

/// Источники: откуда приезжают узлы и как часто обновляются.
export default function Sources({
  sources,
  hidden,
  onHidden,
  focus,
  onFocused,
  schedule,
  onSchedule,
  drafts,
  onDraft,
  onDisk,
  onChanged,
  onMessage,
}: Props) {
  /// Какое окно добавления открыто. Три действия — три окна: «Ссылка» ходит в сеть,
  /// «Из файла» читает диск, «Вручную» собирает узел по полям (D-120).
  const [adding, setAdding] = useState<"link" | "node" | null>(null);
  const [importing, setImporting] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [amount, setAmount] = useState<number | null>(30);
  const [unit, setUnit] = useState<"minutes" | "hours">("minutes");
  /// Список или текст того, что прислала панель (D-065). Здесь по умолчанию список:
  /// код — второй способ смотреть на то же самое, а не основной.
  const [view, setView] = useState<View>("visual");
  /// Чей текст открыт в коде. Источник по умолчанию — первый; исчез (удалили) — снова первый.
  const [sourceId, setSourceId] = useState<string | null>(null);
  const current = sources.find((source) => source.id === sourceId) ?? sources[0];
  /// Какой источник раскрыт в список своих узлов. Один за раз: раскрытые все сразу
  /// превращают колонку обратно в простыню.
  const [opened, setOpened] = useState<string | null>(null);
  /// Узлы — чтобы показать, из чего источник состоит. Спрашиваем при открытии раздела
  /// и после каждой правки состава, а не опросом: список меняется только от них.
  const [nodes, setNodes] = useCached<api.Node[]>("sources.nodes", []);
  /// «Через 20 мин» считается от часов, а не от данных — пересчитываем раз в полминуты.
  const now = useNow(30_000);

  /// Состав узлов меняется только от того, что происходит здесь же: добавили источник,
  /// обновили, удалили. Поэтому не опрос, а вызов после каждого из трёх.
  const reloadNodes = useCallback(() => {
    api.nodesList().then(setNodes, () => setNodes([]));
  }, []);

  useEffect(reloadNodes, [reloadNodes]);

  // «+» в шапке просит добавить источник. Раньше это значило «поставь курсор в поле»;
  // поля больше нет, и просьба означает то же самое действие — открыть подписку.
  useEffect(() => {
    if (!focus) return;
    // Окно добавления живёт в виде списка: из кода туда сначала надо вернуться.
    setView("visual");
    setAdding("link");
    onFocused();
  }, [focus, onFocused]);

  /// Готовый вариант по номеру; -1 — «своё», его считаем из поля и единицы.
  const fromPreset = (index: number): api.Refresh => {
    if (index >= 0) return api.REFRESH_PRESETS[index].value;
    const every = Math.max(1, amount ?? 1);
    return { onStart: true, everyMinutes: unit === "hours" ? every * 60 : every };
  };

  /// Источник приехал — откуда бы он ни приехал. Одно место на три окна: список узлов
  /// и полоса сообщений про способ добавления ничего не знают.
  const added = useCallback(
    async (result: api.Import) => {
      setAdding(null);
      await onChanged();
      reloadNodes();
      onMessage(notice(importSummary(result), result.notices));
    },
    [onChanged, onMessage, reloadNodes],
  );

  /// Файл спрашивает система, а не окно: своего списка «Недавних» и ввода пути
  /// с клавиатуры мы не напишем.
  const fromFile = useCallback(async () => {
    setImporting(true);
    try {
      const result = await api.sourcesAddFile();
      // Закрыли окно выбора — это не отказ и сообщением не является.
      if (result !== null) await added(result);
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setImporting(false);
    }
  }, [added, onMessage]);

  const refresh = useCallback(
    async (id: string) => {
      setBusy(id);
      try {
        const result = await api.sourcesRefresh(id);
        await onChanged();
        reloadNodes();
        onMessage(notice(importSummary(result), result.notices));
      } catch (e) {
        onMessage(failure(e));
      } finally {
        setBusy(null);
      }
    },
    [onChanged, onMessage, reloadNodes],
  );

  /// Убрать один узел из источника записей (D-121). Второй раз не переспрашиваем:
  /// узел здесь заводили руками, и завести его снова — то же одно окно.
  const dropNode = useCallback(
    async (id: string, node: string) => {
      setBusy(id);
      try {
        await api.nodesDelete(id, node);
        await onChanged();
        reloadNodes();
      } catch (e) {
        onMessage(failure(e));
      } finally {
        setBusy(null);
      }
    },
    [onChanged, onMessage, reloadNodes],
  );

  const remove = useCallback(
    async (id: string) => {
      try {
        const warning = await api.sourcesDelete(id);
        await onChanged();
        reloadNodes();
        // Предупреждение приходит, только когда ваш набор ссылается на удалённый источник:
        // ядро на такую ссылку отвечает отказом стартовать, и узнать об этом при следующем
        // подключении — слишком поздно.
        if (warning) onMessage(notice(warning));
      } catch (e) {
        onMessage(failure(e));
      }
    },
    [onChanged, onMessage, reloadNodes],
  );

  const preset = api.refreshPreset(schedule);
  const withUrl = sources.filter((source) => source.url !== null);

  /// Записать текст источника. Живёт здесь, а не в редакторе: кнопка записи стоит
  /// в полосе раздела, как во всех разделах (`SaveActions`).
  const draft = current ? drafts[current.id] : undefined;
  const [saving, setSaving] = useState(false);
  const saveCode = async () => {
    if (current === undefined || draft === undefined) return;
    setSaving(true);
    try {
      const result = await api.sourcesWrite(current.id, draft.text);
      onDisk(current.id, draft.text);
      await onChanged();
      onMessage(notice(importSummary(result), result.notices));
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setSaving(false);
    }
  };

  const eye = (
    <IconButton
      variant="ghost"
      icon={hidden ? <EyeOff /> : <Eye />}
      active={hidden}
      label={
        hidden
          ? t("Show addresses and subscription names")
          : t("Hide addresses and subscription names")
      }
      onClick={onHidden}
    />
  );

  if (view === "code") {
    return (
      <>
        <SectionBar
          start={
            <>
              <ViewSwitch list value={view} onChange={setView} />
              {sources.length > 1 && current && (
                <Select
                  className="w-[220px]"
                  aria-label={t("Source")}
                  value={current.id}
                  onChange={setSourceId}
                  options={sources.map((source) => ({
                    value: source.id,
                    label: hide(source.name, hidden) ?? source.name,
                  }))}
                />
              )}
            </>
          }
          end={
            <>
              {eye}
              {current && draft !== undefined && !hidden && (
                <SaveActions
                  dirty={draft.text !== draft.saved}
                  busy={saving}
                  onUndo={() => onDraft(current.id, draft.saved)}
                  onSave={saveCode}
                />
              )}
            </>
          }
          hint={t(
            "The original provider response. Edits use the same name normalization as refreshed subscriptions.",
          )}
        />
        <SourceCode
          source={current}
          hidden={hidden}
          draft={draft}
          onDraft={onDraft}
          onDisk={onDisk}
          onMessage={onMessage}
        />
      </>
    );
  }

  return (
    <>
      {adding === "link" && (
        <LinkDialog
          onSubmit={async (link) => added(await api.sourcesAdd(link))}
          onClose={() => setAdding(null)}
        />
      )}
      {adding === "node" && (
        <NodeDialog
          onDone={added}
          onClose={() => setAdding(null)}
          onFailed={(error) => onMessage(failure(error))}
        />
      )}
      <SectionBar start={<ViewSwitch list value={view} onChange={setView} />} end={eye} />
      {/* Две колонки, каждая со своей прокруткой: страница окна не прокручивается. */}
      <div className="grid min-h-0 flex-1 grid-cols-[280px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)] gap-3 max-[819px]:grid-cols-1">
        <Scroll>
          {/* Три действия — три кнопки: подписка ходит в сеть, файл читает диск,
              «вручную» собирает узел по полям (D-120). */}
          <Card title={t("Add source")}>
            <div className="flex flex-col gap-1.5">
              <Button variant="primary" block icon={<Rss />} onClick={() => setAdding("link")}>
                {t("Link")}
              </Button>
              <Button block icon={<FilePlus2 />} loading={importing} onClick={fromFile}>
                {t("From file")}
              </Button>
              <Button block icon={<SquarePen />} onClick={() => setAdding("node")}>
                {t("Manually")}
              </Button>
              <Text tone="muted" size="xs" className="mt-1 block">
                {t(
                  "Subscriptions create separate sources; individual links go to My links. Supported protocols are normalized into core records.",
                )}
              </Text>
            </div>
          </Card>

          <Card title={t("Subscription refresh")} description={nextRefresh(withUrl, schedule, now)}>
            <div className="flex flex-col gap-2">
              <Select
                aria-label={t("Subscription refresh schedule")}
                value={String(preset)}
                onChange={(value) => onSchedule(fromPreset(Number(value)))}
                options={[
                  ...api.REFRESH_PRESETS.map((item, index) => ({
                    value: String(index),
                    label: t(item.label),
                  })),
                  { value: "-1", label: t("Custom…") },
                ]}
              />
              {preset < 0 && (
                <Field label={t("Every")}>
                  <div className="flex items-center gap-1.5">
                    <NumberInput
                      aria-label={t("Refresh interval")}
                      min={1}
                      value={amount}
                      onChange={setAmount}
                    />
                    <Select
                      aria-label={t("Interval unit")}
                      value={unit}
                      onChange={setUnit}
                      options={[
                        { value: "minutes", label: t("minutes") },
                        { value: "hours", label: t("hours") },
                      ]}
                    />
                    <Button onClick={() => onSchedule(fromPreset(-1))}>{t("Apply")}</Button>
                  </div>
                </Field>
              )}
            </div>
          </Card>
        </Scroll>

        <Scroll className="gap-2">
          {sources.length === 0 ? (
            <Card>
              <EmptyState
                icon={<Rss />}
                title={t("No sources")}
                hint={t("Add a subscription, a file or a custom node.")}
              />
            </Card>
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
              />
            ))
          )}
        </Scroll>
      </div>
    </>
  );
}
