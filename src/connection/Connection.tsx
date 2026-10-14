import { Eye, EyeOff, Gauge, RefreshCw, Server } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button, Callout, Card, confirm, EmptyState, IconButton } from "rootik";
import * as api from "../api";
import type { Drafts } from "../config/draft";
import { useCached } from "../hooks/useCached";
import { unchanged, usePoll } from "../hooks/usePoll";
import { useLive } from "../hooks/useTraffic";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import AddMenu, { type AddKind } from "../sources/AddMenu";
import SourceDialog from "../sources/SourceDialog";
import SourceList from "../sources/SourceList";
import ConnectionPath from "./ConnectionPath";
import { choice, chosen } from "./exits";
import GroupsView from "./GroupsView";
import { groupName } from "./groups";
import NodeEditor from "./NodeEditor";
import NodesView from "./NodesView";
import PanelSwitch, { type Panel } from "./PanelSwitch";
import Speed from "./Speed";

type Props = {
  status: api.Status;
  mode: api.Choice;
  busy: boolean;
  powering: boolean;
  onMode: (mode: api.Choice) => void;
  onPower: () => void;
  sources: api.Source[];
  /// Режим «на людях»: адреса и имена подписок закрыты точками (D-127).
  hidden: boolean;
  onHidden: () => void;
  /// Статус после смены направления приходит сразу — ждать ближайшего опроса значит
  /// полторы секунды показывать прошлое.
  onStatus: (status: api.Status) => void;
  onAdd: (kind: AddKind) => void;
  onMessage: (message: Message) => void;
  /// Вид «Источники» (D-160): расписание, черновики сырья и перечитать после правки.
  schedule: api.Refresh;
  drafts: Drafts;
  onDraft: (id: string, text: string) => void;
  onDisk: (id: string, text: string) => void;
  onSourcesChanged: () => Promise<void>;
  /// Значки групп (D-172) — вид окна, хранятся в настройках.
  icons: Record<string, string>;
  onIcons: (icons: Record<string, string>) => void;
  /// «Создать» во вкладке «Группы»: новая группа — в разделе «Группы».
  onCreateGroup: () => void;
};

/**
 * Главный раздел: слева управление и нагрузка, справа узлы, группы и источники (D-141,
 * D-142, D-172).
 *
 * Выбор набора отсюда ушёл (D-075): три направления из четырёх собирает клиент целиком,
 * и набор к ним отношения не имеет. Он живёт там, где правится, — в «Маршрутизации».
 */
export default function Connection({
  status,
  mode,
  busy: appBusy,
  powering,
  onMode,
  onPower,
  sources,
  hidden,
  onHidden,
  onStatus,
  onAdd,
  onMessage,
  schedule,
  drafts,
  onDraft,
  onDisk,
  onSourcesChanged,
  icons,
  onIcons,
  onCreateGroup,
}: Props) {
  const [nodes, setNodes] = useCached<api.Node[]>("connection.nodes", []);
  /// Что справа: узлы, группы или источники (D-160, D-172). Помним на сессию (D-079).
  const [panel, setPanel] = useCached<Panel>("connection.panel", "nodes");
  /// Выход от выбранного до узла: `["AUTO", "Poland 1"]` (D-145). Первое звено — что
  /// выбрано, последнее — через кого трафик уходит на самом деле.
  const [route, setRoute] = useCached<string[]>("connection.route", []);
  const [direction, setDirection] = useCached<api.Direction>("connection.direction", "direct");
  /// Узел или группа Manual и цель `MATCH` мимо выбора (D-166, D-172) — из того же снимка.
  const [node, setNode] = useCached<string | null>("connection.node", null);
  const [fallback, setFallback] = useCached<string | null>("connection.fallback", null);
  /// Группы клиента с составом и кого нет в AUTO (D-172).
  const [built, setBuilt] = useCached<api.BuiltGroup[]>("connection.built", []);
  const [exclude, setExclude] = useCached<api.Exclude>("connection.exclude", {
    sources: [],
    nodes: [],
  });
  /// Свои группы из документа «Группы» — для вкладки и её счётчика.
  const [user, setUser] = useCached<api.Group[]>("connection.groups", []);
  /// Чем меряем задержку (D-069). Спрашиваем опросом, а не помним: способ правится
  /// в другом разделе, и вернуться оттуда с устаревшей подсказкой нельзя.
  const [ping, setPing] = useCached<api.PingMethod>("connection.ping", "tcp");
  const [measuring, setMeasuring] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  /// Правый щелчок по узлу — правка (D-114); карандаш источника — его название (D-172).
  const [editing, setEditing] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<api.Source | null>(null);
  /// Трафик — из хранилища, которое наполняет опрос `App` (D-076): история графика
  /// переживает уход в другой раздел, а перерисовывается от неё только этот.
  const live = useLive();

  /// Спрашиваем всегда, пока раздел открыт, а не только при живом ядре: на остановленном
  /// узлы читаются с диска, и список серверов должен быть виден до подключения.
  ///
  /// Направление тоже спрашиваем у бэкенда, а не помним сами: выбранный узел мог исчезнуть
  /// после обновления подписки, и тогда оттуда честно придёт `auto` вместо `manual` (D-056).
  /// Всё одним вызовом (D-145), а раскладываем по своим состояниям: смена выхода не должна
  /// перерисовывать список узлов, который не менялся.
  const generation = useRef(0);
  const poll = useCallback(() => {
    const asked = generation.current;
    api.connectionSnapshot().then(
      (snapshot) => {
        if (asked !== generation.current) return;
        unchanged(setNodes)(snapshot.nodes);
        unchanged(setRoute)(snapshot.route);
        unchanged(setBuilt)(snapshot.groups);
        unchanged(setExclude)(snapshot.exclude);
        setDirection(snapshot.direction);
        setNode(snapshot.node);
        setFallback(snapshot.fallback);
        setPing(snapshot.ping);
      },
      () => {},
    );
  }, []);

  usePoll(poll);

  /// Свои группы — с диска, а не из черновика раздела «Группы»: показываем собранное.
  const readGroups = useCallback(async () => {
    try {
      unchanged(setUser)(await api.groupsParse(await api.configRead("groups")));
    } catch (e) {
      onMessage(failure(e));
    }
  }, [onMessage]);
  useEffect(() => {
    readGroups();
  }, [readGroups]);

  /// Показываем выбор сразу, но правду скажет ближайший опрос: источник истины — бэкенд,
  /// и он же поправит направление, если выбранного узла больше нет. `takeMatch` — заодно
  /// вернуть выбору `MATCH` набора (D-166).
  const choose = useCallback(
    async (next: api.Direction, picked?: string, takeMatch = false) => {
      generation.current++;
      setDirection(next);
      if (picked) {
        setNode(picked);
        setRoute([picked]);
      }
      if (takeMatch) setFallback(null);
      try {
        onStatus(
          await (takeMatch ? api.directionTakeMatch(next, picked) : api.directionSet(next, picked)),
        );
      } catch (e) {
        onMessage(failure(e));
      } finally {
        generation.current++;
        poll();
      }
    },
    [onMessage, onStatus, poll],
  );

  /// DIRECT, AUTO, группа и узел — выходы одного списка (D-166, D-172): выбор ставит
  /// направление. Пока `MATCH` набора смотрит мимо выбора, выбор спрашивает, заменить ли
  /// его: молча выбранный узел решал бы только правила в `umiray`.
  const pick = useCallback(
    // Отдаёт, выбран ли выход: «Подключиться» не включает VPN, если человек передумал.
    async (name: string): Promise<boolean> => {
      if (fallback === name) return true;
      const next = choice(name);
      if (fallback === null) {
        choose(next.direction, next.node);
        return true;
      }
      const replace = await confirm({
        title: t("Send everything else through {name}?", { name: groupName(name, nodes) }),
        description: t(
          "Right now the MATCH rule in Routing sends it to {target}. It will follow the choice here again.",
          { target: groupName(fallback, nodes) },
        ),
        confirmLabel: t("Replace"),
        cancelLabel: t("Cancel"),
      });
      if (replace) choose(next.direction, next.node, true);
      return replace;
    },
    [choose, fallback, nodes],
  );

  /// «Подключиться» у группы: сделать её выходом и, если VPN выключен, включить его.
  const connect = useCallback(
    async (name: string) => {
      if ((await pick(name)) && !status.running) onPower();
    },
    [pick, status.running, onPower],
  );

  /// Замер по просьбе говорит об отказе; сам по себе — молчит: замер через прокси без
  /// подключения иначе показывал бы баннер на каждом заходе (D-062, D-069).
  const measure = useCallback(
    async (report: boolean) => {
      setMeasuring(true);
      try {
        await api.nodesPing();
        poll();
      } catch (e) {
        if (report) onMessage(failure(e));
      } finally {
        setMeasuring(false);
      }
    },
    [onMessage, poll],
  );

  /// Все подписки разом, потом заново замер. Упавшая не отменяет остальных — она названа.
  const refresh = async () => {
    setRefreshing(true);
    try {
      const failures = await api.sourcesRefreshAll();
      if (failures.length > 0) {
        onMessage(notice(t("Some subscriptions didn't refresh."), failures));
      }
      await onSourcesChanged();
      poll();
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setRefreshing(false);
    }
    await measure(false);
  };

  /// Непомеренный список меряем один раз сами, дальше — только по просьбе.
  const measured = useRef(false);
  useEffect(() => {
    if (measured.current || nodes.length === 0) return;
    measured.current = true;
    if (!nodes.some((item) => item.delay !== null)) measure(false);
  }, [nodes, measure]);

  const exclusions = async (next: api.Exclude) => {
    try {
      onStatus(await api.groupsAutoExclude(next));
      poll();
    } catch (e) {
      onMessage(failure(e));
      throw e;
    }
  };

  // Отмечен выход — DIRECT, AUTO, группа или узел (D-166, D-172), — и на остановленном ядре:
  // выбор сделан нажатием и помнится через перезапуск (D-039). Цепочка кончается группой,
  // если у группы нет одного «текущего»: AUTO — это load-balance, и каждое соединение ядро
  // кладёт на свой узел. Тогда честный ответ — кто везёт трафик сейчас (S-018): самый
  // загруженный, остальные числом. Только в AUTO: с маршрутизацией через те же узлы идёт
  // и то, что назначили другие правила.
  const carrying = useMemo(
    () =>
      Object.entries(live.rates)
        .filter(([, rate]) => rate.connections > 0)
        .sort(([, a], [, b]) => b.down + b.up - (a.down + a.up))
        .map(([name]) => name),
    [live.rates],
  );
  const lead = carrying[0] ?? null;
  const spread =
    direction === "auto" && lead !== null && !nodes.some((item) => item.name === route.at(-1));
  const path = useMemo(() => (spread && lead ? [...route, lead] : route), [spread, lead, route]);
  // Карточка называет узел, через который трафик уходит на самом деле, — конец цепочки.
  const exit = path.at(-1) ?? null;
  // `MATCH` набора не в `umiray` — отмечена его цель: выход, группа или узел (D-166).
  const selected = fallback ?? chosen(direction, node);
  const groups = built.length + user.length;

  const view =
    panel === "sources" ? (
      <SourceList
        sources={sources}
        nodes={nodes}
        hidden={hidden}
        schedule={schedule}
        drafts={drafts}
        onDraft={onDraft}
        onDisk={onDisk}
        onChanged={async () => {
          await onSourcesChanged();
          poll();
        }}
        onAdd={onAdd}
        onMessage={onMessage}
      />
    ) : panel === "groups" ? (
      <GroupsView
        nodes={nodes}
        sources={sources}
        built={built}
        user={user}
        hidden={hidden}
        rates={live.rates}
        selected={selected}
        icons={icons}
        onIcons={onIcons}
        onChoose={pick}
        onConnect={connect}
        running={status.running}
        onEdit={setEditing}
        onCreate={onCreateGroup}
        onStatus={onStatus}
        onChanged={() => {
          readGroups();
          poll();
        }}
        onMessage={onMessage}
      />
    ) : (
      <NodesView
        nodes={nodes}
        sources={sources}
        hidden={hidden}
        rates={live.rates}
        selected={selected}
        auto={built.find((group) => group.name === "AUTO")}
        exclude={exclude}
        onChoose={pick}
        onEdit={setEditing}
        onRename={setRenaming}
        onExclude={exclusions}
      />
    );

  return (
    // Раздел занимает ровно окно между шапкой и dock: слева управление и нагрузка на всю
    // высоту, справа узлы на всю высоту со своей прокруткой. Страница целиком не едет.
    // Прокрутка левой колонки — запас на случай баннера сверху, а не обычный вид.
    <div className="grid min-h-0 flex-1 grid-cols-[340px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)] gap-3">
      <div className="flex min-h-0 flex-col gap-3 overflow-y-auto">
        <ConnectionPath
          status={status}
          mode={mode}
          running={api.runningMode(status)}
          direction={direction}
          fallback={fallback}
          route={path}
          more={spread ? carrying.length - 1 : 0}
          selected={nodes.find((item) => item.name === exit) ?? null}
          total={nodes.filter((item) => item.supported).length}
          hidden={hidden}
          busy={appBusy}
          powering={powering}
          onPower={onPower}
          onMode={onMode}
        />
        <Speed
          running={status.running}
          history={live.history}
          current={live.current}
          totals={live.totals}
          connections={live.totals?.connections ?? null}
        />
      </div>

      <Card
        padding="sm"
        className="h-full min-h-0"
        title={
          <PanelSwitch
            value={panel}
            onChange={setPanel}
            nodes={nodes.length}
            groups={groups}
            sources={sources.length}
          />
        }
        actions={
          <span className="flex items-center gap-1">
            <IconButton
              size="sm"
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
            <IconButton
              size="sm"
              variant="ghost"
              icon={<RefreshCw />}
              loading={refreshing}
              disabled={measuring}
              label={t("Refresh subscriptions and measure latency again")}
              onClick={refresh}
            />
            <IconButton
              size="sm"
              variant="ghost"
              icon={<Gauge />}
              loading={measuring}
              disabled={refreshing || nodes.length === 0}
              label={t("Check latency: {method}. Change the method in Settings", {
                method: api.PING_LABEL[ping],
              })}
              onClick={() => measure(true)}
            />
          </span>
        }
      >
        {/* Прокручивается только содержимое вкладки под заголовком. */}
        <div className="flex h-full min-h-0 flex-col gap-2">
          {fallback !== null && panel !== "sources" && (
            <Callout tone="info">
              {t(
                "Everything your rules don't catch goes to {target} — set by MATCH in Routing. Pick an exit or a node to send it there instead.",
                { target: fallback },
              )}
            </Callout>
          )}
          <div className="-mx-1 min-h-0 flex-1 overflow-y-auto px-1 pb-1">
            <div key={panel} className="um-swap">
              {view}
            </div>
            {/* Без узлов выходы одни — это не список: карточка говорит, откуда берутся узлы. */}
            {nodes.length === 0 && panel === "nodes" && (
              <EmptyState
                icon={<Server />}
                title={t("No nodes")}
                hint={t("Add a subscription or a link.")}
                action={
                  <AddMenu
                    onPick={onAdd}
                    trigger={<Button variant="primary">{t("Add source")}</Button>}
                  />
                }
              />
            )}
          </div>
        </div>
      </Card>

      {editing !== null &&
        (() => {
          // Из свежего списка: сохранённая правка видна сразу.
          const item = nodes.find((entry) => entry.name === editing);
          return item === undefined ? null : (
            <NodeEditor node={item} onClose={() => setEditing(null)} onChanged={poll} />
          );
        })()}
      {renaming !== null && (
        <SourceDialog
          source={renaming}
          hidden={hidden}
          onClose={() => setRenaming(null)}
          onSaved={onSourcesChanged}
        />
      )}
    </div>
  );
}
