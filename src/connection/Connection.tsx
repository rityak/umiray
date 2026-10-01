import { useCallback, useMemo, useRef } from "react";
import { Callout, confirm } from "rootik";
import * as api from "../api";
import type { Drafts } from "../config/draft";
import { useCached } from "../hooks/useCached";
import { unchanged, usePoll } from "../hooks/usePoll";
import { useLive } from "../hooks/useTraffic";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import type { AddKind } from "../sources/AddMenu";
import SourceList from "../sources/SourceList";
import ConnectionPath from "./ConnectionPath";
import { choice, chosen, exits } from "./exits";
import Nodes from "./Nodes";
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
  onSchedule: (schedule: api.Refresh) => void;
  drafts: Drafts;
  onDraft: (id: string, text: string) => void;
  onDisk: (id: string, text: string) => void;
  onSourcesChanged: () => Promise<void>;
};

/**
 * Главный раздел: сверху управление и нагрузка, под ними узлы (D-141, D-142).
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
  onSchedule,
  drafts,
  onDraft,
  onDisk,
  onSourcesChanged,
}: Props) {
  const [nodes, setNodes] = useCached<api.Node[]>("connection.nodes", []);
  /// Что справа: узлы или источники (D-160). Помним на сессию, как вид списка (D-079).
  const [panel, setPanel] = useCached<Panel>("connection.panel", "nodes");
  /// Выход от выбранного до узла: `["AUTO", "Poland 1"]` (D-145). Первое звено — что
  /// выбрано, последнее — через кого трафик уходит на самом деле.
  const [route, setRoute] = useCached<string[]>("connection.route", []);
  const [direction, setDirection] = useCached<api.Direction>("connection.direction", "direct");
  /// Узел Manual и цель `MATCH` мимо выбора (D-166) — оба из того же снимка.
  const [node, setNode] = useCached<string | null>("connection.node", null);
  const [fallback, setFallback] = useCached<string | null>("connection.fallback", null);
  /// Чем меряем задержку (D-069). Спрашиваем опросом, а не помним: способ правится
  /// в другом разделе, и вернуться оттуда с устаревшим заголовком колонки нельзя.
  const [ping, setPing] = useCached<api.PingMethod>("connection.ping", "tcp");
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
        setDirection(snapshot.direction);
        setNode(snapshot.node);
        setFallback(snapshot.fallback);
        setPing(snapshot.ping);
      },
      () => {},
    );
  }, []);

  usePoll(poll);

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

  /// DIRECT и AUTO — такие же строки списка, как узлы (D-166): нажатие ставит направление.
  /// Пока `MATCH` набора смотрит мимо выбора, нажатие спрашивает, заменить ли его: молча
  /// выбранный узел решал бы только правила в `umiray`.
  const pick = useCallback(
    async (name: string) => {
      if (fallback === name) return;
      const next = choice(name);
      if (fallback === null) {
        choose(next.direction, next.node);
        return;
      }
      const replace = await confirm({
        title: t("Send everything else through {name}?", { name }),
        description: t(
          "Right now the MATCH rule in Routing sends it to {target}. It will follow the choice here again.",
          { target: fallback },
        ),
        confirmLabel: t("Replace"),
        cancelLabel: t("Cancel"),
      });
      if (replace) choose(next.direction, next.node, true);
    },
    [choose, fallback],
  );
  const own = useMemo(() => exits(nodes.length), [nodes.length]);
  const measure = useMemo(() => ({ method: ping, run: api.nodesPing }), [ping]);
  /// Every subscription at once. One that failed does not cancel the rest — it is named.
  const refreshSources = useCallback(async () => {
    const failures = await api.sourcesRefreshAll();
    if (failures.length > 0) {
      onMessage(notice(t("Some subscriptions didn't refresh."), failures));
    }
  }, [onMessage]);

  // Отмечена строка выбора — DIRECT, AUTO или узел (D-166), — и на остановленном ядре тоже:
  // выбор сделан нажатием и помнится через перезапуск (D-039, D-166).
  // Цепочка кончается группой, если у группы нет одного «текущего»: AUTO — это
  // load-balance, и каждое соединение ядро кладёт на свой узел. Тогда честный ответ —
  // кто везёт трафик сейчас (S-018): самый загруженный, остальные числом. Только в AUTO:
  // с маршрутизацией через те же узлы идёт и то, что назначили другие правила.
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
    direction === "auto" && lead !== null && !nodes.some((node) => node.name === route.at(-1));
  const path = useMemo(() => (spread && lead ? [...route, lead] : route), [spread, lead, route]);
  // Карточка называет узел, через который трафик уходит на самом деле, — конец цепочки.
  const exit = path.at(-1) ?? null;
  const head = (
    <PanelSwitch value={panel} onChange={setPanel} nodes={nodes.length} sources={sources.length} />
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
          selected={nodes.find((node) => node.name === exit) ?? null}
          total={nodes.filter((node) => node.supported).length}
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

      {panel === "sources" ? (
        <SourceList
          head={head}
          sources={sources}
          nodes={nodes}
          hidden={hidden}
          onHidden={onHidden}
          schedule={schedule}
          onSchedule={onSchedule}
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
      ) : (
        <Nodes
          head={head}
          nodes={nodes}
          exits={own}
          // `MATCH` набора не в `umiray` — отмечена его цель: DIRECT, AUTO или узел. Цель-группа
          // строкой списка не бывает, и тогда не отмечено ничего (D-166).
          selected={fallback ?? chosen(direction, node)}
          note={
            fallback !== null && (
              <Callout tone="info">
                {t(
                  "Everything your rules don't catch goes to {target} — set by MATCH in Routing. Pick an exit or a node to send it there instead.",
                  { target: fallback },
                )}
              </Callout>
            )
          }
          sources={sources}
          hidden={hidden}
          rates={live.rates}
          onSelect={pick}
          onChanged={poll}
          onAdd={onAdd}
          onMessage={onMessage}
          onRefresh={refreshSources}
          refreshLabel={t("Refresh subscriptions and measure latency again")}
          emptyHint={t("Add a subscription or a link.")}
          measure={measure}
          onHidden={onHidden}
          editable
        />
      )}
    </div>
  );
}
