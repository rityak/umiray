/// Ядра и подключение (D-154): какое ядро, в каком режиме, сколько трафика — и питание.

import { z } from "zod";
import { call, done } from "./call";

/// Режим, каким его видит ядро. `System` для него неотличим от `local` — разница живёт
/// в реестре Windows.
export const Mode = z.enum(["local", "tun"]);
export type Mode = z.infer<typeof Mode>;

/// Что выбрано в шапке (D-060). `Off` здесь нет: выключение — это кнопка питания,
/// а режим только правит конфиг и ядро не трогает.
export const Choice = z.enum(["local", "system", "tun"]);
export type Choice = z.infer<typeof Choice>;

/// Какое ядро (D-154). Выбранное в шапке — вид и то, что поднимет кнопка питания;
/// работающее приходит в статусе отдельно.
export const Engine = z.enum(["mihomo", "qd"]);
export type Engine = z.infer<typeof Engine>;

export const Status = z.object({
  /// Какое ядро держит трафик сейчас; `null` — никакое. Шапка говорит о нём.
  active: Engine.nullable(),
  /// Работает ли mihomo; режим и порт ниже — тоже его.
  running: z.boolean(),
  /// В каком режиме ядро работает **сейчас**. Пусто — не работает.
  mode: Mode.nullable(),
  /// Что выбрано в шапке. Отличается от `mode` свободно: выбор правит конфиг,
  /// а доезжает он перезапуском (D-060).
  desiredMode: Choice,
  /// Выбор разошёлся с работающим ядром. Правило считает бэкенд — второй его копии
  /// на фронте быть не должно.
  /// Почему работающему ядру нужен подъём заново — готовой строкой. Пусто — не нужен.
  /// Правило живёт в Rust (D-102): окно не знает ни одного имени поля ядра.
  restartReason: z.string().nullable(),
  /// На что жалуется сторож соединения (D-107). Пусто — трафик идёт или ядра нет.
  trouble: z.string().nullable(),
  /// Порт локального прокси в том виде, в каком он попал в конфиг.
  port: z.number().nullable(),
  corePresent: z.boolean(),
  elevated: z.boolean(),
  /// Поднимается ли клиент с правами **всегда** — то есть заведена ли задача в
  /// планировщике (D-087). Факт системы, как и `autostart`: задачу можно убрать мимо нас.
  alwaysAdmin: z.boolean(),
  /// Стоит ли наш адрес в настройках Windows **прямо сейчас** (D-047). Это состояние
  /// реестра, а не желание пользователя: если запись не удалась, здесь будет `false`.
  systemProxy: z.boolean(),
  /// Чужой прокси в системе, если он там есть: другой VPN-клиент может уже занимать
  /// это место. Включение нашего его заменит — об этом надо предупредить, а не молчать.
  foreignProxy: z.string().nullable(),
  /// Стоит ли автозапуск. Это состояние реестра, а не настройка: запись можно убрать
  /// мимо нас через диспетчер задач, и окно обязано показывать, как есть.
  autostart: z.boolean(),
  /// Стоит ли **сейчас** запрет выхода мимо туннеля (D-073). Факт, а не намерение:
  /// намерение живёт в `Settings.killSwitch`, и разойтись им можно — включённый тумблер
  /// в режиме, отличном от TUN, не запирает ничего.
  killSwitch: z.boolean(),
  /// Когда ядро поднялось, в секундах эпохи; `null` — не работает. Время работы окно
  /// считает из неё само: готовая длительность в статусе устаревала бы между опросами.
  started: z.number().nullable(),
});
export type Status = z.infer<typeof Status>;

export const NodeTraffic = z.object({
  node: z.string(),
  up: z.number(),
  down: z.number(),
  connections: z.number(),
});
export type NodeTraffic = z.infer<typeof NodeTraffic>;

/// Сколько прошло с запуска ядра. Скорость считает интерфейс — он знает интервал опроса.
export const Traffic = z.object({
  up: z.number(),
  down: z.number(),
  connections: z.number(),
  /// Через кого трафик идёт прямо сейчас (S-018): сумма по **открытым** соединениям.
  /// Закрытое соединение из ответа ядра пропадает — поэтому это «кто везёт сейчас»,
  /// а не «кто вёз всего».
  nodes: z.array(NodeTraffic),
});
export type Traffic = z.infer<typeof Traffic>;

export const LOCAL_PROXY = "127.0.0.1:3090";

export const coreStatus = () => call(Status, "core_status");

export const coreLogs = (engine: Engine) => call(z.array(z.string()), "core_logs", { engine });

/// Выбрать режим (D-060) и довести его до работающего ядра: TUN — перезапуском,
/// System и Proxy — реестром; открытые соединения рвутся (D-143).
export const modeSet = (mode: Choice) => call(Status, "mode_set", { mode });

/// Скачать ядро; отдаёт версию. Любое ядро (D-154), пока оно не держит трафик.
export const coreInstall = (engine: Engine) => call(z.string(), "core_install", { engine });

export const coreStart = () => call(Status, "core_start");
/// Перезапуск: то, что ядро читает на старте, доезжает только так (D-010).
export const coreRestart = () => call(Status, "core_restart");
export const coreStop = () => call(Status, "core_stop");
export const coreTraffic = () => call(Traffic.nullable(), "core_traffic");
/// Забыть карту подменных адресов (S-021). Только по нажатию: сброс раздаёт пул заново,
/// то есть делает ровно то, от чего `store-fake-ip` бережёт.
export const coreFlushFakeIp = () => call(done, "core_flush_fake_ip");

export const MODE_LABEL: Record<Choice, string> = {
  local: "Proxy",
  system: "System",
  tun: "TUN",
};

/**
 * В каком режиме ядро работает **сейчас**; `null` — не работает.
 *
 * Не то же, что `status.desiredMode`: тот показывает выбранное в шапке, и разойтись
 * они могут свободно — выбор доезжает перезапуском (D-060). Здесь именно факт: режим
 * ядра плюс реестр, потому что System от Local отличает только он (D-047).
 */
export function runningMode(status: Status): Choice | null {
  if (!status.running) return null;
  if (status.mode === "tun") return "tun";
  return status.systemProxy ? "system" : "local";
}

/**
 * Адрес локального прокси — или `null`, когда его нет.
 *
 * Показывать его обязательно: по D-008 клиент не трогает системный прокси, и пока
 * пользователь не пропишет этот адрес сам, «Подключено» не означает, что трафик идёт.
 * В TUN адреса нет вовсе — там перехватывается всё.
 */
export function proxyAddress(status: Status): string | null {
  if (!status.running || status.mode !== "local") return null;
  return status.port === null ? LOCAL_PROXY : `127.0.0.1:${status.port}`;
}
