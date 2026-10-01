/// Документы пользователя (D-044, D-070): разделы, группы, маршрутизация, наборы,
/// встроенные наборы правил, поля ядра с формой.

import { z } from "zod";
import { t } from "../i18n";
import { call, done } from "./call";
import { Status } from "./core";

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
  /// `endpoint-independent-nat`: «открытый» NAT для игр и звонков в TUN (D-169).
  openNat: z.boolean(),
  dnsEnable: z.boolean(),
  enhancedMode: Enhanced,
  nameserver: z.array(z.string()),
  /// `prefer-h3`: DoH сначала по HTTP/3 (D-169).
  preferH3: z.boolean(),
});
export type Advanced = z.infer<typeof Advanced>;

export const advancedGet = () => call(Advanced, "advanced_get");
/// Возвращает **прочитанное с диска**, а не присланное: форма показывает файл.
export const advancedSet = (options: Advanced) => call(Advanced, "advanced_set", { options });

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

/// Где запись встаёт в маршруте (D-158): high → medium → low; без поля — medium.
export const Priority = z.enum(["high", "medium", "low"]);
export type Priority = z.infer<typeof Priority>;

/// Скачанный список в маршруте (D-157, D-158). `url` — только у своего, по адресу.
export const RuleSetUse = z.object({
  id: z.string(),
  url: z.string().optional(),
  target: z.string(),
  priority: Priority.optional(),
});
export type RuleSetUse = z.infer<typeof RuleSetUse>;

/// Готовый набор в маршруте (D-158). Без `target` — выход самого набора.
export const ReadyUse = z.object({
  id: z.string(),
  target: z.string().optional(),
  priority: Priority.optional(),
});
export type ReadyUse = z.infer<typeof ReadyUse>;

/// Документ маршрутизации целиком (D-158): свои правила, rule sets, готовые наборы и MATCH.
export const Routing = z.object({
  rules: z.array(Rule),
  /// Цель `MATCH`.
  fallback: z.string(),
  ruleSets: z.array(RuleSetUse),
  ready: z.array(ReadyUse),
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
export const rulesProcesses = () =>
  call(z.array(z.object({ name: z.string() })), "rules_processes");

export const configList = () => call(z.array(ConfigSection), "config_list");
export const configRead = (id: string) => call(z.string(), "config_read", { id });
/// Записать документ и довести до работающего ядра то, что до него доходит (D-143):
/// правку применённого набора или общих групп — да, неприменённого набора — нет (D-071).
export const configWrite = (id: string, text: string) => call(Status, "config_write", { id, text });
export const configReset = (id: string) => call(z.string(), "config_reset", { id });

/// Собранный конфиг в части одного раздела — то, что уходит ядру. Только для показа:
/// группы AUTO и umiray в файле пользователя не лежат, и увидеть их иначе негде.
/// Собранный конфиг целиком — то, что уходит ядру (D-130).
export const configAssembled = () => call(z.string(), "config_assembled");

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

/// Готовый набор правил (D-083): файл в `collections/rules`. В маршрут его выбирает
/// раздел `ready` документа (D-158); `target` — общий выход его строк, `null` — разные.
export const Ruleset = z.object({
  id: z.string(),
  title: z.string(),
  titleEn: z.string().nullish(),
  target: z.string().nullable(),
  rules: z.array(z.string()),
});
export type Ruleset = z.infer<typeof Ruleset>;

export const rulesetsList = () => call(z.array(Ruleset), "rulesets_list");
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
