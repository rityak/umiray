import { Channel, type InvokeArgs, invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { locale, t, tk, tn } from "./i18n";

/// Режим, каким его видит ядро. `System` для него неотличим от `local` — разница живёт
/// в реестре Windows.
export const Mode = z.enum(["local", "tun"]);
export type Mode = z.infer<typeof Mode>;

/// Что выбрано в шапке (D-060). `Off` здесь нет: выключение — это кнопка питания,
/// а режим только правит конфиг и ядро не трогает.
export const Choice = z.enum(["local", "system", "tun"]);
export type Choice = z.infer<typeof Choice>;

/// Тема прежнего оформления. Бэкенд её хранит, окно не читает: вид теперь у rootik (D-142).
export const Theme = z.enum(["midnight", "green", "purple"]);
export type Theme = z.infer<typeof Theme>;

export const Status = z.object({
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

/// Настройки: поведение, живущее на диске (D-024). `version` — схема файла, фронтенд её
/// не трогает: поля меняет бэкенд отдельными командами.
/// Когда обновлять подписки. Два поля, а не одно: «только при запуске» — это поведение,
/// а не период.
export const Refresh = z.object({
  onStart: z.boolean(),
  /// Ноль — по времени не обновлять.
  everyMinutes: z.number(),
});
export type Refresh = z.infer<typeof Refresh>;

/// Как открывается окно при запуске (D-088, D-129).
export const Launch = z.enum(["smart", "window", "tray"]);
export type Launch = z.infer<typeof Launch>;

export const Settings = z.object({
  version: z.number(),
  refresh: Refresh,
  theme: Theme,
  // Намерение «прописывать ли прокси в систему» бэкенд тоже хранит, но окну оно не нужно
  // и здесь не объявлено намеренно: рядом живёт `Status.systemProxy` — **факт из реестра**,
  // и два поля с одним именем в одном файле читались как одно и то же (см. карту, §7.6).
  // Окно спрашивает у реестра, а не у намерения: если запись не удалась, показать надо правду.
  /// Пиксельная сцена фоном (D-093). Отдельно от дождя: картинка — это вид, движение —
  /// это дождь, и гасят их по разным поводам.
  scene: z.boolean(),
  /// Размытие сцены, пиксели (D-132). Дождь не трогает.
  sceneBlur: z.number(),
  /// Дождь (D-045). Системный `prefers-reduced-motion` гасит его поверх этого флага:
  /// настройка означает «пользователь так захотел», а не «так решила система».
  effects: z.boolean(),
  /// Режим «на людях»: адреса серверов и имена подписок закрыты точками, чтобы клиент
  /// можно было показать на экране, не выдав, куда человек подключён.
  private: z.boolean(),
  /// Запирать ли выход мимо туннеля, пока работает TUN (D-073). Здесь **намерение** —
  /// в отличие от системного прокси, у которого окну нужен только факт: тумблер может
  /// стоять и не быть в силе, и об этом надо сказать словами, а не снимать галку.
  killSwitch: z.boolean(),
  /// Подключаться сразу при запуске клиента (D-088). Что поднимать, помнить не нужно:
  /// направление и режим перехвата переживают перезапуск сами.
  autoConnect: z.boolean(),
  /// Показывать ли окно при запуске (D-088).
  launch: Launch,
  /// Предлагать ли «всегда от администратора», когда клиент запущен с правами, а задачи
  /// ещё нет (D-087). Отказ помнится: предложение, возвращающееся каждый запуск, —
  /// это уже не предложение.
  adminOffer: z.boolean(),
});
export type Settings = z.infer<typeof Settings>;

/// Что меняем в настройках. Присылаем только изменившееся — остальное бэкенд не тронет.
export type SettingsPatch = {
  refresh?: Refresh;
  theme?: Theme;
  scene?: boolean;
  sceneBlur?: number;
  effects?: boolean;
  private?: boolean;
  autoConnect?: boolean;
  launch?: Launch;
  adminOffer?: boolean;
};

/// Готовые варианты для выпадающего списка. «Своё» — не пресет, его считает форма.
export const REFRESH_PRESETS: { label: string; value: Refresh }[] = [
  { label: tk("Never refresh"), value: { onStart: false, everyMinutes: 0 } },
  { label: tk("Only on startup"), value: { onStart: true, everyMinutes: 0 } },
  { label: tk("Every hour"), value: { onStart: true, everyMinutes: 60 } },
  { label: tk("Every 6 hours"), value: { onStart: true, everyMinutes: 360 } },
  { label: tk("Every day"), value: { onStart: true, everyMinutes: 1440 } },
];

/** Совпадает ли текущая настройка с готовым вариантом. Нет — значит выбрано «Своё». */
export function refreshPreset(refresh: Refresh): number {
  return REFRESH_PRESETS.findIndex(
    (preset) =>
      preset.value.onStart === refresh.onStart &&
      preset.value.everyMinutes === refresh.everyMinutes,
  );
}

export const Stack = z.enum(["system", "gvisor", "mixed"]);
export type Stack = z.infer<typeof Stack>;
export const LogLevel = z.enum(["silent", "error", "warning", "info", "debug"]);
export type LogLevel = z.infer<typeof LogLevel>;
export const Enhanced = z.enum(["fake-ip", "redir-host"]);
export type Enhanced = z.infer<typeof Enhanced>;

/// Поля конфига ядра, у которых есть форма (D-086). Плоские: раскладку по `tun` и `dns`
/// знает бэкенд, а окну важно поле, а не то, в каком разделе файла оно лежит.
export const Advanced = z.object({
  logLevel: LogLevel,
  mixedPort: z.number(),
  sniffer: z.boolean(),
  stack: Stack,
  /// Пусто — как назовёт адаптер само ядро (`Meta`).
  device: z.string(),
  /// Ноль — не задавать, решает ядро.
  mtu: z.number(),
  strictRoute: z.boolean(),
  dnsHijack: z.array(z.string()),
  dnsEnable: z.boolean(),
  enhancedMode: Enhanced,
  nameserver: z.array(z.string()),
});
export type Advanced = z.infer<typeof Advanced>;

export const advancedGet = () => call(Advanced, "advanced_get");
/// Возвращает **прочитанное с диска**, а не присланное: форма показывает файл.
export const advancedSet = (options: Advanced) => call(Advanced, "advanced_set", { options });

/// Источник — откуда взялись узлы: подписка или «мои ссылки» (D-032).
export const Source = z.object({
  id: z.string(),
  name: z.string(),
  /// Адрес подписки. Пусто у «моих ссылок» — обновлять их неоткуда.
  url: z.string().nullable(),
  /// Секунды эпохи. Форматирует интерфейс, бэкенд времени не знает.
  updated: z.number().nullable(),
  nodes: z.number(),
  /// Источник хранит записи `proxies:`, а не ссылки: узлы в нём написал клиент, и их
  /// можно убрать (D-121). У подписки удаление было бы враньём — узел вернётся.
  records: z.boolean(),
});
export type Source = z.infer<typeof Source>;

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

const PING_HINT: Record<PingMethod, string> = {
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

/// Результат добавления источника.
export const Import = z.object({
  /// Служебные сообщения провайдера — лимит устройств и подобное.
  notices: z.array(z.string()),
  source: Source,
});
export type Import = z.infer<typeof Import>;

/// Ошибка с бэкенда (D-028). `kind` — машинно-читаемая причина, `details` — строки лога
/// ядра или сообщения провайдера: их показываем списком, а не склеиваем в текст.
export const AppError = z.object({
  kind: z.string(),
  message: z.string(),
  details: z.array(z.string()).catch([]),
});
export type AppError = z.infer<typeof AppError>;

/**
 * Всё, что прилетает из Rust, — недоверенные данные. Команда может отвергнуть промис и
 * обычной строкой (например, если упал сам мост Tauri), поэтому форму проверяем, а не приводим.
 */
export function asAppError(raw: unknown): AppError {
  const parsed = AppError.safeParse(raw);
  if (parsed.success) return parsed.data;
  // Не `String(raw)`: наружу это вываливалось как «Error: …» — чужой текст в русском
  // баннере. Сырую строку кладём в `details`, где ей и место (D-028).
  return {
    kind: "unknown",
    message: t("Something went wrong inside the client."),
    details: [String(raw)],
  };
}

/// Команда, которая ничего не отдаёт: проверять в ответе нечего.
const done = z.unknown().transform((): void => undefined);

export const UpdateInfo = z.object({
  enabled: z.boolean(),
  version: z.string().nullable(),
  notes: z.string().nullable(),
});
export type UpdateInfo = z.infer<typeof UpdateInfo>;
export const UpdateProgress = z.object({
  phase: z.enum(["download", "install"]),
  downloaded: z.number().nonnegative(),
  total: z.number().nonnegative().nullable(),
});
export type UpdateProgress = z.infer<typeof UpdateProgress>;
export const updatesCheck = () => call(UpdateInfo, "updates_check");
export const updatesInstall = (onProgress: (progress: UpdateProgress) => void) => {
  const progress = new Channel<unknown>();
  progress.onmessage = (raw) => {
    const parsed = UpdateProgress.safeParse(raw);
    if (parsed.success) onProgress(parsed.data);
  };
  return call(done, "updates_install", { progress });
};

/**
 * Вызвать команду и **проверить** ответ по схеме: данные из Rust не доверенные,
 * и `invoke<T>` только делал вид, что знает их форму. Поле, переименованное на одной
 * стороне границы, раньше доезжало до окна как `undefined` и ломало его где-то в глубине;
 * теперь команда отказывает сразу, с именем поля в подробностях — тем же путём, что
 * и любая ошибка бэкенда (D-028, D-144). Лишние поля схема отбрасывает: окну они не обещаны.
 */
async function call<T extends z.ZodType>(
  schema: T,
  command: string,
  args?: InvokeArgs,
): Promise<z.output<T>> {
  const parsed = schema.safeParse(await invoke<unknown>(command, args));
  if (parsed.success) return parsed.data;
  throw {
    kind: "unexpected",
    message: t("Unexpected backend response — update the whole client."),
    details: [command, z.prettifyError(parsed.error)],
  } satisfies AppError;
}

export const LOCAL_PROXY = "127.0.0.1:3090";

/// Куда идёт трафик (D-056). Одно решение вместо двух: раньше это были «выбранный узел»
/// и негласный режим, и на вопрос «что будет с выбором, если я перепишу маршрутизацию»
/// ответа не было вовсе.
export const Direction = z.enum(["auto", "manual", "direct", "rules"]);
export type Direction = z.infer<typeof Direction>;

/** Как часто окно опрашивает бэкенд. Одно число на все опросы: разнобой заметен глазом. */
export const POLL_MS = 1500;

export const coreStatus = () => call(Status, "core_status");
export const settingsGet = () => call(Settings, "settings_get");
export const settingsUpdate = (patch: SettingsPatch) =>
  call(Settings, "settings_update", { patch });
export const coreLogs = () => call(z.array(z.string()), "core_logs");
/// Правимый документ (D-044). Содержимое приходит отдельно: документов несколько,
/// открыт один.
export const ConfigDoc = z
  .object({
    id: z.string(),
    label: z.string(),
    hint: z.string(),
    /// Уходит ли документ ядру. У настроек клиента собранного вида нет вовсе (D-068).
    core: z.boolean(),
    /// Этот набор применён — его документы сейчас и решают маршрут (D-071).
    applied: z.boolean(),
  })
  .transform((doc) => ({
    ...doc,
    label: doc.id.startsWith("rules/") ? doc.label : t(doc.label),
    hint: t(doc.hint),
  }));
export type ConfigDoc = z.infer<typeof ConfigDoc>;

/// Раздел окна с документами внутри (D-070). Раздел перестал быть синонимом файла,
/// когда в «Настройках» их стало два.
export const ConfigSection = z
  .object({
    id: z.string(),
    label: z.string(),
    /// Документы раздела — наборы: их заводят, применяют и удаляют (D-071).
    presets: z.boolean(),
    docs: z.array(ConfigDoc),
  })
  .transform((section) => ({ ...section, label: t(section.label) }));
export type ConfigSection = z.infer<typeof ConfigSection>;

/// Группа узлов так, как её видит окно (D-074). Имена полей нейтральные: спеллинг ядра
/// (`type`, `use`) живёт в Rust и на границу не выходит.
export const Group = z.object({
  name: z.string(),
  /// `select`, `url-test`, `fallback`, `load-balance` — или что-то ещё: незнакомый тип
  /// форма показывает, но не правит.
  kind: z.string(),
  /// Источники целиком. Живой список: новые узлы подписки приезжают в группу сами.
  sources: z.array(z.string()),
  /// Узлы и группы по имени.
  proxies: z.array(z.string()),
  filter: z.string().nullable(),
  url: z.string().nullable(),
  interval: z.number().nullable(),
  tolerance: z.number().nullable(),
  strategy: z.string().nullable(),
  /// Поля, которых форма не знает. Она их не трогает — но и не молчит про них.
  extra: z.array(z.string()),
  /// Место группы в исходном документе; пусто — группу завела форма. Ездит вместе
  /// со строкой, поэтому незнакомое поле переживает и переименование, и перестановку.
  origin: z.number().nullable(),
});
export type Group = z.infer<typeof Group>;

/// Правило маршрутизации. Одно правило окна держит несколько значений и разворачивается
/// в столько же строк конфига.
export const Rule = z.object({
  kind: z.string(),
  values: z.array(z.string()),
  target: z.string(),
  /// Хвост правила: `no-resolve` и подобное. Форма его не трогает, но и не теряет.
  options: z.array(z.string()),
});
export type Rule = z.infer<typeof Rule>;

/// Документ маршрутизации целиком: правила по порядку и судьба всего остального.
export const Routing = z.object({
  rules: z.array(Rule),
  /// Цель `MATCH`.
  fallback: z.string(),
});
export type Routing = z.infer<typeof Routing>;

/// Разбор и сборка идут через бэкенд, а не через свой YAML в окне (D-074): поля ядра
/// пишутся в одном месте, и `cargo test` их проверяет. Диска здесь нет — обе команды
/// работают с черновиком, который потом пишет `configWrite`.
export const groupsParse = (text: string) => call(z.array(Group), "groups_parse", { text });
export const groupsRender = (text: string, groups: Group[]) =>
  call(z.string(), "groups_render", { text, groups });
export const rulesParse = (text: string) => call(Routing, "rules_parse", { text });
export const rulesRender = (text: string, routing: Routing) =>
  call(z.string(), "rules_render", { text, routing });

export const configList = () => call(z.array(ConfigSection), "config_list");
export const configRead = (id: string) => call(z.string(), "config_read", { id });
/// Записать документ и довести до работающего ядра то, что до него доходит (D-143):
/// правку применённого набора или общих групп — да, неприменённого набора — нет (D-071).
export const configWrite = (id: string, text: string) => call(Status, "config_write", { id, text });
export const configReset = (id: string) => call(z.string(), "config_reset", { id });
/// Выбрать режим (D-060) и довести его до работающего ядра: TUN — перезапуском,
/// System и Proxy — реестром; открытые соединения рвутся (D-143).
export const modeSet = (mode: Choice) => call(Status, "mode_set", { mode });
/// Собранный конфиг в части одного раздела — то, что уходит ядру. Только для показа:
/// группы AUTO и umiray в файле пользователя не лежат, и увидеть их иначе негде.
/// Собранный конфиг целиком — то, что уходит ядру (D-130).
export const configAssembled = () => call(z.string(), "config_assembled");
export const coreInstall = () => call(z.string(), "core_install");
export const systemRelaunchElevated = () => call(done, "system_relaunch_elevated");
/// Сброс всего, кроме скачанного ядра и идентификатора устройства. Ядро останавливает сам.
export const systemReset = () => call(Status, "system_reset");
export const systemAutostartSet = (on: boolean) => call(Status, "system_autostart_set", { on });

/// «Всегда от администратора» (D-087). Отдельная команда, а не поле настроек: это задача
/// в планировщике, и завести её можно только с правами — отказ приезжает как ошибка
/// с кнопкой, а не как молча не сохранившийся тумблер.
export const systemAlwaysAdminSet = (on: boolean) =>
  call(Status, "system_always_admin_set", { on });

/// Отдельная команда, а не поле в `settingsUpdate`: тумблер меняет состояние машины
/// и обязан сработать сейчас, а не при следующем подключении (D-073).
export const systemKillSwitchSet = (on: boolean) => call(Status, "system_kill_switch_set", { on });
/// Идентификатор устройства — справочно, в настройках. Отдельным запросом, а не полем
/// статуса: он не меняется никогда, а статус опрашивается каждую секунду.
export const systemDevice = () => call(z.string(), "system_device");
export const systemLanguage = () => call(z.enum(["en", "ru"]), "system_language");
export const sourcesList = () => call(z.array(Source), "sources_list");
export const sourcesAdd = (input: string) => call(Import, "sources_add", { input });
export const sourcesRefresh = (id: string) => call(Import, "sources_refresh", { id });
/// Обновить все подписки разом. Отдаёт строки о том, что **не** получилось: одна упавшая
/// подписка не отменяет остальных.
export const sourcesRefreshAll = () => call(z.array(z.string()), "sources_refresh_all");
/// Отдаёт предупреждение, если на удаляемый источник ссылаются ваши группы или любой набор
/// правил (D-075), иначе `null`. Пересобрать чужой документ клиент не вправе, а ядро на ссылку
/// в никуда отвечает отказом стартовать целиком — молчать об этом нельзя.
export const sourcesDelete = (id: string) => call(z.string().nullable(), "sources_delete", { id });
/// Что прислала панель, слово в слово (D-065). Правится именно это: собранный файл,
/// который читает ядро, пересобирается из него при каждом обновлении.
export const sourcesRead = (id: string) => call(z.string(), "sources_read", { id });
export const sourcesWrite = (id: string, text: string) =>
  call(Import, "sources_write", { id, text });
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
/// Узел, собранный руками или принесённый файлом (D-120). Запись, а не ссылку: ссылку
/// пришлось бы выдумать, а имена полей записи берутся из документации ядра.
export const sourcesAddProxy = (entry: Record<string, unknown>) =>
  call(Import, "sources_add_proxy", { entry });
export const sourcesAddProxyText = (text: string) =>
  call(Import, "sources_add_proxy_text", { text });
/// Тот же узел текстом — для «кода» в окне сборки. Рендерит бэкенд: второй YAML в окне
/// разошёлся бы с первым.
export const sourcesProxyYaml = (entry: Record<string, unknown>) =>
  call(z.string(), "sources_proxy_yaml", { entry });
/// Системное окно выбора файла и разбор того, что выбрали. `null` — закрыли окно.
export const sourcesAddFile = () => call(Import.nullable(), "sources_add_file");

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
/// Набор правил: пара «группы + маршрутизация» под направление RULES (D-056).
export const Preset = z.object({
  id: z.string(),
  name: z.string(),
  /// Секунды эпохи. Форматирует интерфейс, бэкенд времени не знает.
  created: z.number().nullable(),
});
export type Preset = z.infer<typeof Preset>;

/// Завести набор — копию того, что клиент собирает из ваших источников (D-071).
export const presetsCreate = () => call(Preset, "presets_create");
/// Применить набор: с этого момента маршрут решают его документы, а направление встаёт
/// в RULES — это и значит «применить» (D-071).
export const presetsSelect = (id: string) => call(done, "presets_select", { id });
export const presetsRename = (id: string, name: string) =>
  call(Preset, "presets_rename", { id, name });
/// Применённый набор удалить нельзя — маршрут остался бы без документов; последний тоже —
/// разделу «Маршрутизация» нечего было бы показать (D-071).
export const presetsDelete = (id: string) => call(done, "presets_delete", { id });

/// Встроенный набор правил (D-083): файл в папке `rulesets/` плюс отметка «включён».
export const Ruleset = z.object({
  id: z.string(),
  title: z.string(),
  titleEn: z.string().nullish(),
  on: z.boolean(),
  rules: z.array(z.string()),
});
export type Ruleset = z.infer<typeof Ruleset>;

export const rulesetsList = () => call(z.array(Ruleset), "rulesets_list");
/// Включение доезжает до живого ядра перезагрузкой (D-102), поэтому команда отдаёт статус.
export const rulesetsSet = (id: string, on: boolean) => call(Status, "rulesets_set", { id, on });
/// Текст набора для правки в окне (D-104): тот же файл, что правят руками.
export const rulesetsRead = (id: string) => call(z.string(), "rulesets_read", { id });
/// «Сохранить» здесь **применяет**: отдельного «применить» у встроенного набора нет,
/// его роль играет тумблер.
export const rulesetsWrite = (id: string, text: string) =>
  call(Status, "rulesets_write", { id, text });
/// Свой набор. Ядру ничего не доезжает — новый выключен, — поэтому возвращается
/// идентификатор, а не статус: окно сразу раскрывает его редактором.
export const rulesetsCreate = (title: string) => call(z.string(), "rulesets_create", { title });
/// Удаление доезжает до ядра: включённый набор уносит свои правила из сборки.
export const rulesetsDelete = (id: string) => call(Status, "rulesets_delete", { id });

/// UDP через узлы, несущие его датаграммой (D-113). `nodes` — сколько таких нашлось:
/// ноль значит, что включать нечего, а не что выключено.
export const Udp = z.object({ on: z.boolean(), nodes: z.number() });
export type Udp = z.infer<typeof Udp>;
export const udpGet = () => call(Udp, "udp_get");
export const udpSet = (on: boolean) => call(Status, "udp_set", { on });

/// Чем мерить задержку (D-069). Живёт в `client.yaml`, а не в настройках: это документ,
/// который правится и руками тоже (D-068).
/// Через сколько часов перепрашивать страну узла; 0 — не спрашивать (D-084).
export const clientGeoGet = () => call(z.number(), "client_geo_get");
export const clientGeoSet = (hours: number) => call(done, "client_geo_set", { hours });

/// Чем маскировать рукопожатие WireGuard (D-118). Ноль в поле — «не трогать»: у самого
/// AmneziaWG отдельного флага нет, выключенное состояние и есть ноль.
///
/// Два яруса, и разница между ними не косметическая: `jc/jmin/jmax` работают с **любым**
/// сервером WireGuard, а `s*` и `h*` меняют формат пакетов и требуют сервера с AmneziaWG.
export const Mask = z.object({
  jc: z.number(),
  jmin: z.number(),
  jmax: z.number(),
  s1: z.number(),
  s2: z.number(),
  s3: z.number(),
  s4: z.number(),
  h1: z.number(),
  h2: z.number(),
  h3: z.number(),
  h4: z.number(),
});
export type Mask = z.infer<typeof Mask>;
export const MASK_FIELDS = [
  "jc",
  "jmin",
  "jmax",
  "s1",
  "s2",
  "s3",
  "s4",
  "h1",
  "h2",
  "h3",
  "h4",
] as const;
export const clientMaskGet = () => call(Mask, "client_mask_get");
export const clientMaskSet = (mask: Mask) => call(Mask, "client_mask_set", { mask });

export const clientPingGet = () => call(PingMethod, "client_ping_get");
export const clientPingSet = (method: PingMethod) => call(done, "client_ping_set", { method });

/// Куда бьёт проверка живости — одна цель на ядро и на замер клиента (D-108).
export const clientHealthGet = () => call(z.string(), "client_health_get");
export const clientHealthSet = (url: string) => call(done, "client_health_set", { url });

/// Готовые цели. Все трое отдают 204 без тела; какую из них не режут в конкретной
/// стране, знает только пользователь — оттого это список, а не константа. Свою можно
/// вписать в документ «Клиент» кодом: форма покажет её отдельной строкой.
export const HEALTH_TARGETS: { url: string; label: string }[] = [
  { url: "http://cp.cloudflare.com/generate_204", label: "Cloudflare" },
  { url: "http://www.gstatic.com/generate_204", label: "Google (gstatic)" },
  { url: "http://www.google.com/generate_204", label: "Google" },
];

/// --- Инструменты (D-097, D-115) ---------------------------------------------
///
/// Утилита и её отчёт приходят одним типом: окно читает строки и таблицу, а код —
/// типизированный замер рядом с ней (D-097).

/// Версия клиента. Из сборки, а не руками: отчёт диагностики называет её, и разъехаться
/// с настоящей она не должна.
export const VERSION: string = __APP_VERSION__;

export const Verdict = z.enum(["ok", "warn", "bad", "idle"]);
export type Verdict = z.infer<typeof Verdict>;
/// Тон строки консоли. Цвет подбирает окно: модуль про темы ничего не знает.
/// Имя со словом «строка», потому что `Tone` в этом файле уже занят состоянием
/// подключения — а это разные вещи, и путать их нельзя.
export const LineTone = z.enum(["info", "ok", "warn", "bad", "dim"]);
export type LineTone = z.infer<typeof LineTone>;

export const Tool = z.object({
  id: z.string(),
  title: z.string(),
  /// Группа в списке слева.
  group: z.string(),
  hint: z.string(),
  /// Какие параметры принимает — по ним рисуется полоса над консолью.
  params: z.array(z.string()),
  /// Ходит ли в сеть.
  network: z.boolean(),
});
export type Tool = z.infer<typeof Tool>;

export const DiagLine = z.object({ tone: LineTone, text: z.string() });
export type DiagLine = z.infer<typeof DiagLine>;
/// Строка таблицы. `mark` — то, что утилита предлагает взять: на нём стоит действие.
export const DiagRow = z.object({
  cells: z.array(z.string()),
  verdict: Verdict,
  mark: z.boolean(),
});
export type DiagRow = z.infer<typeof DiagRow>;

export const Report = z.object({
  tool: z.string(),
  verdict: Verdict,
  /// Одна строка для «Проверки»: не «ок», а что именно нашлось.
  headline: z.string(),
  ms: z.number(),
  columns: z.array(z.string()),
  rows: z.array(DiagRow),
  lines: z.array(DiagLine),
});
export type Report = z.infer<typeof Report>;

/// Параметры запуска. Всё необязательное: утилита без параметров ничего отсюда не читает.
export type DiagArgs = {
  domain?: string;
  domains?: string[];
  /// Имена для рукопожатия по SNI.
  hosts?: string[];
  /// До кого мерить наибольший пакет.
  host?: string;
  timeoutMs?: number;
  all?: boolean;
  /// Мерить ли шифрованные точки через стенд — каждая стоит запуска ядра.
  core?: boolean;
};

export const diagTools = () => call(z.array(Tool), "diag_tools");
export const diagRun = (id: string, args?: DiagArgs) => call(Report, "diag_run", { id, args });
/// Сделать то, что утилита предлагает: прописать отмеченное в документ пользователя
/// и довести до живого ядра (D-105). Запуск ничего не меняет, а это — запись.
export const diagApply = (id: string, args?: DiagArgs) => call(Report, "diag_apply", { id, args });

/// Справочник резолверов — тот же, из которого берёт кандидатов `dns-race` (D-097).
export const DnsServer = z.object({ proto: z.string(), addr: z.string(), ipv6: z.boolean() });
export type DnsServer = z.infer<typeof DnsServer>;
export const DnsVariant = z.object({
  id: z.string(),
  name: z.string(),
  filter: z.string(),
  servers: z.array(DnsServer),
});
export type DnsVariant = z.infer<typeof DnsVariant>;
export const DnsProvider = z.object({
  id: z.string(),
  name: z.string(),
  note: z.string(),
  site: z.string(),
  variants: z.array(DnsVariant),
});
export type DnsProvider = z.infer<typeof DnsProvider>;
export const diagProviders = () =>
  call(z.object({ version: z.number(), providers: z.array(DnsProvider) }), "diag_providers");

export const coreStart = () => call(Status, "core_start");
/// Перезапуск: то, что ядро читает на старте, доезжает только так (D-010).
export const coreRestart = () => call(Status, "core_restart");
export const coreStop = () => call(Status, "core_stop");
export const coreTraffic = () => call(Traffic.nullable(), "core_traffic");
/// Забыть карту подменных адресов (S-021). Только по нажатию: сброс раздаёт пул заново,
/// то есть делает ровно то, от чего `store-fake-ip` бережёт.
export const coreFlushFakeIp = () => call(done, "core_flush_fake_ip");

/// Четыре состояния из STYLEGUIDE, и ровно четыре: «подключается» и «ошибка» — разные вещи,
/// раньше они делили один `warn`.
export type Tone = "on" | "connecting" | "off" | "error";

/** Состояние показываем текстом и цветом, а не одним цветом (STYLEGUIDE). */
export function statusView(status: Status, busy: boolean): { label: string; tone: Tone } {
  if (busy) return { label: t("Starting…"), tone: "connecting" };
  if (!status.corePresent) return { label: t("Core not found"), tone: "error" };
  const mode = runningMode(status);
  if (mode === null) return { label: t("Disconnected"), tone: "off" };
  // Адрес здесь не повторяем: он показан отдельно и с кнопкой копирования, а в двух
  // местах сразу одно и то же значение только сбивает.
  return { label: `${t("Connected")} · ${MODE_LABEL[mode]}`, tone: "on" };
}

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

/**
 * Сколько ядро работает, словами. Считается от отметки в статусе, а не таймером в окне:
 * окно живёт дольше ядра и переживает его перезапуск (D-057).
 *
 * Секунд не показываем дальше первой минуты: строка в шапке меняется раз в полторы
 * секунды, и бегущие цифры в ней читались бы как тревога, а не как справка.
 */
export function formatUptime(started: number | null, now = Date.now()): string | null {
  if (started === null) return null;
  const seconds = Math.max(0, Math.floor(now / 1000) - started);
  if (seconds < 60) return t("{n} s", { n: seconds });
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return t("{n} min", { n: minutes });
  const hours = Math.floor(minutes / 60);
  return t("{hours} h {minutes} min", { hours, minutes: minutes % 60 });
}

const UNITS = [tk("B"), tk("KB"), tk("MB"), tk("GB"), tk("TB")];

/** Объём человеческим языком. Ядро отдаёт байты, читать их глазами невозможно. */
export function formatBytes(bytes: number): string {
  let value = Math.max(0, bytes);
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toFixed(digits)} ${t(UNITS[unit])}`;
}

/** Итог импорта одной строкой. */
export function importSummary(result: Import): string {
  return `«${result.source.name}»: ${tn(result.source.nodes, "{n} node", "{n} nodes")}`;
}

/** Когда источник последний раз обновляли. Никогда — значит собран из отдельных ссылок. */
export function updatedLabel(source: Source): string {
  if (source.updated === null) return t("never refreshed");
  return new Date(source.updated * 1000).toLocaleString(locale());
}

/**
 * Сколько до сервера. Ядро тут ни при чём — меряем сами, и работает это при выключенном
 * VPN тоже (D-062).
 */
export function delayLabel(node: Node): string {
  return node.delay === null ? "—" : t("{n} ms", { n: node.delay });
}

/// Чем именно померено — словом, а не только цветом (STYLEGUIDE).
///
/// Запасной замер называет себя первым: число в такой строке отвечает не на тот вопрос,
/// который задавали, и молча выдавать его за заказанный нельзя (D-069).
export function delayHint(node: Node): string {
  if (node.method === null) return t("not measured or the server did not respond");
  if (node.fallback) {
    // У `hysteria2`, `tuic` и `wireguard` TCP-порта нет вовсе, и «проверка не дала
    // результата» про них — не про сервер, а про сам способ. Такие меряет ядро.
    if (node.method === "proxy" || node.method === "proxy-keepalive") {
      return t("This protocol has no TCP response — measured through the node: {method}", {
        method: t(PING_HINT[node.method]),
      });
    }
    return t("The selected check gave no result, but the host is reachable — {method}", {
      method: t(PING_HINT[node.method]),
    });
  }
  return t(PING_HINT[node.method]);
}
