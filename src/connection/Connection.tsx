import { useCallback, useMemo, useRef } from "react";
import * as api from "../api";
import { useCached } from "../hooks/useCached";
import { unchanged, usePoll } from "../hooks/usePoll";
import { useLive } from "../hooks/useTraffic";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import ConnectionPath from "./ConnectionPath";
import Nodes from "./Nodes";
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
  onAdd: () => void;
  onMessage: (message: Message) => void;
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
}: Props) {
  const [nodes, setNodes] = useCached<api.Node[]>("connection.nodes", []);
  /// Выход от выбранного до узла: `["AUTO", "Poland 1"]` (D-145). Первое звено — что
  /// выбрано, последнее — через кого трафик уходит на самом деле.
  const [route, setRoute] = useCached<string[]>("connection.route", []);
  const [direction, setDirection] = useCached<api.Direction>("connection.direction", "direct");
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
        setPing(snapshot.ping);
      },
      () => {},
    );
  }, []);

  usePoll(poll);

  /// Показываем выбор сразу, но правду скажет ближайший опрос: источник истины — бэкенд,
  /// и он же поправит направление, если выбранного узла больше нет.
  const choose = useCallback(
    async (next: api.Direction, node?: string) => {
      generation.current++;
      setDirection(next);
      if (node) setRoute([node]);
      try {
        onStatus(await api.directionSet(next, node));
      } catch (e) {
        onMessage(failure(e));
      } finally {
        generation.current++;
        poll();
      }
    },
    [onMessage, onStatus, poll],
  );

  const pick = useCallback((node: string) => choose("manual", node), [choose]);
  const measure = useMemo(() => ({ method: ping, run: api.nodesPing }), [ping]);
  /// Every subscription at once. One that failed does not cancel the rest — it is named.
  const refreshSources = useCallback(async () => {
    const failures = await api.sourcesRefreshAll();
    if (failures.length > 0) {
      onMessage(notice(t("Some subscriptions could not be refreshed."), failures));
    }
  }, [onMessage]);
  const manual = useCallback(() => choose("manual"), [choose]);

  // Что отмечено в списке, зависит от направления, а не от того, работает ли ядро (D-092).
  // MANUAL — это **выбор пользователя**: он сделан нажатием, помнится через перезапуск
  // (D-039) и существует независимо от процесса; гасить отметку на остановленном ядре
  // значило бы, что нажатие по плитке не оставляет следа. В AUTO наоборот — «текущий»
  // называет само ядро, и на остановленном его просто нет. В DIRECT сервера нет вовсе,
  // в RULES их назначают правила, и одного «текущего» там не существует.
  // Цепочка кончается группой, если у группы нет одного «текущего»: AUTO — это
  // load-balance, и каждое соединение ядро кладёт на свой узел. Тогда честный ответ —
  // кто везёт трафик сейчас (S-018): самый загруженный, остальные числом. Только в AUTO:
  // в Rules через те же узлы идёт и то, что назначили другие правила.
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
  // Отмечен узел, через который трафик уходит на самом деле, — конец цепочки, а не «AUTO».
  const exit = path.at(-1) ?? null;
  const picked = direction === "manual" || (status.running && direction === "auto") ? exit : null;

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
          route={path}
          more={spread ? carrying.length - 1 : 0}
          selected={nodes.find((node) => node.name === exit) ?? null}
          total={nodes.filter((node) => node.supported).length}
          hidden={hidden}
          busy={appBusy}
          powering={powering}
          onPower={onPower}
          onMode={onMode}
          onDirection={choose}
        />
        <Speed
          running={status.running}
          history={live.history}
          current={live.current}
          totals={live.totals}
          connections={live.totals?.connections ?? null}
        />
      </div>

      <Nodes
        nodes={nodes}
        selected={picked}
        // В RULES маршрут за правилами (D-096): нажатие по узлу предупреждает,
        // а строка над списком предлагает уйти в MANUAL.
        rules={direction === "rules"}
        sources={sources}
        hidden={hidden}
        rates={live.rates}
        onSelect={pick}
        onManual={manual}
        onChanged={poll}
        onAdd={onAdd}
        onMessage={onMessage}
        onRefresh={refreshSources}
        refreshLabel={t("Refresh subscriptions and measure latency again")}
        emptyHint={t("Add a subscription or a link — the core will load it.")}
        measure={measure}
        onHidden={onHidden}
        editable
      />
    </div>
  );
}
