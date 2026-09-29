/// Узлы, их замеры и куда идёт трафик (D-056, D-061, D-069).

import { z } from "zod";
import { tk } from "../i18n";
import { call, done } from "./call";
import { Status } from "./core";

/// Чем мерить, сколько до сервера (D-069). Способ выбирает человек — в «Настройках»,
/// документ «Клиент»; здесь же он приходит обратно как то, чем **на самом деле** померено.
export const PingMethod = z.enum(["icmp", "tcp", "proxy", "proxy-keepalive"]);
export type PingMethod = z.infer<typeof PingMethod>;

/// Как способ называется в окне. Заголовок — термин, и он английский (STYLEGUIDE):
/// «через прокси» заставляло бы догадываться, что это `proxy`. Объяснение по-русски идёт
/// строкой ниже — в карточке формы и в подсказке строки.
export const PING_LABEL: Record<PingMethod, string> = {
  icmp: "ICMP",
  tcp: "TCP",
  proxy: "via proxy",
  "proxy-keepalive": "via proxy keep-alive",
};

export const PING_HINT: Record<PingMethod, string> = {
  icmp: tk("ICMP: regular ping to the host"),
  tcp: tk("TCP: time to connect to the node's port"),
  proxy: tk("through the node: best of two requests"),
  "proxy-keepalive": tk("through the node: second request over an established tunnel"),
};

/// Узел — одна конфигурация протокола. Состав списка знает диск, поэтому он одинаков
/// при живом и остановленном ядре (D-061).
export const Node = z.object({
  name: z.string(),
  /// Протокол: Vless, Trojan, Wireguard…
  kind: z.string(),
  /// Идентификатор источника.
  source: z.string(),
  /// Дойдёт ли узел до ядра. Ложь — ссылку не читает ни ядро, ни наш шов (D-063):
  /// такой узел показываем с пометкой и не даём выбрать.
  supported: z.boolean(),
  /// Адресуется ли узел по имени. Истина только у тех, кого клиент положил в конфиг сам:
  /// узлы провайдера в `proxies:` группы не попадают вовсе, и берутся только целым
  /// источником через `use:` (S-012). На этом стоит весь редактор групп.
  /// Сколько до сервера по последнему замеру. Пусто — не мерили или не ответил.
  delay: z.number().nullable(),
  method: PingMethod.nullable(),
  /// Число получено запасным способом: выбранная проверка промолчала, а хост жив (D-069).
  fallback: z.boolean(),
  /// Адрес из самой ссылки.
  address: z.string().nullable(),
  /// Код страны из двух букв — по адресу, а не по имени (D-084). Пусто: не спрашивали,
  /// не узнали или геозапросы выключены.
  country: z.string().nullable(),
  /// Узел правлен пользователем — значит поверх пришедшего лежит наша разница.
  edited: z.boolean(),
});
export type Node = z.infer<typeof Node>;

/// Куда идёт трафик (D-056). Одно решение вместо двух: раньше это были «выбранный узел»
/// и негласный режим, и на вопрос «что будет с выбором, если я перепишу маршрутизацию»
/// ответа не было вовсе.
export const Direction = z.enum(["auto", "manual", "direct", "rules"]);
export type Direction = z.infer<typeof Direction>;

/// Домен `nodes`, а не `core`: список приходит с диска и одинаков при живом
/// и остановленном ядре (D-061).
export const nodesList = () => call(z.array(Node), "nodes_list");
/// Померить, сколько до каждого сервера (D-062). Числа остаются в бэкенде — их принесёт
/// ближайший `nodesList`.
export const nodesPing = () => call(done, "nodes_ping");
/// Правка узла поверх источника (D-114). Храним разницу, а не копию: свежие значения
/// с сервера должны доезжать, а правка — переживать обновление подписки.
/// Конфиг узла документом — переход в код из панели правки: разобранная ссылка, та же,
/// которую прочитает ядро, и только для чтения. `draft` — несохранённая правка опций:
/// код обязан показывать то, что уедет ядру.
/// Код узла: запись, которую клиент пишет сам, либо разложенный документ ссылки (D-119).
/// `editable` говорит, какой из двух это, — и принимает ли редактор ввод.
/// `entry` — та же запись объектом: из неё окно заполняет форму (D-121). Пусто там,
/// где формы быть не может, и тогда `why` говорит почему.
export const NodeCode = z.object({
  text: z.string(),
  editable: z.boolean(),
  entry: z.record(z.string(), z.unknown()).nullable(),
  why: z.string().nullable(),
});
export type NodeCode = z.infer<typeof NodeCode>;

export const nodesCode = (source: string, node: string) =>
  call(NodeCode, "nodes_code", { source, node });
/// Записать узел, собранный формой (D-121). Объектом: YAML знает бэкенд, окно — нет.
export const nodesEntrySet = (source: string, node: string, entry: Record<string, unknown>) =>
  call(done, "nodes_entry_set", { source, node, entry });

/// Убрать узел, который клиент написал сам.
export const nodesDelete = (source: string, node: string) =>
  call(done, "nodes_delete", { source, node });

export const nodesCodeSet = (source: string, node: string, text: string) =>
  call(done, "nodes_code_set", { source, node, text });

export const nodesReset = (source: string, node: string) =>
  call(done, "nodes_reset", { source, node });
/// Всё, что опрашивает «Соединение», одним ответом (D-145): узлы, направление — с поправкой
/// на исчезнувший выбранный узел, — способ замера и выход от выбранного до узла.
export const ConnectionSnapshot = z.object({
  nodes: z.array(Node),
  direction: Direction,
  ping: PingMethod,
  /// `["AUTO", "Poland 1"]`: что выбрано и куда оно ведёт на самом деле. Пусто — выхода нет.
  route: z.array(z.string()),
});
export type ConnectionSnapshot = z.infer<typeof ConnectionSnapshot>;
export const connectionSnapshot = () => call(ConnectionSnapshot, "connection_snapshot");
/// Сменить направление. Узел приходит вместе с ним: нажатие по строке таблицы —
/// это одно действие, а не два.
/// Отдаёт статус: работающее ядро при смене направления перезапускается (D-064),
/// и окно обязано показать результат сразу, а не через опрос.
export const directionSet = (direction: Direction, node?: string) =>
  call(Status, "direction_set", { direction, node: node ?? null });
