import {
  ArrowRightLeft,
  Database,
  Download,
  Eraser,
  type LucideIcon,
  Plug,
  RotateCcw,
  Wand2,
  Waves,
  Zap,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Button, ChoiceCards, Code, ConfirmButton, Select, Switch, Tooltip } from "rootik";
import * as api from "../api";
import { ENGINES } from "../engines";
import { has, unsupportedRows } from "../features";
import { useCached } from "../hooks/useCached";
import { type LanguagePreference, languagePreference, t, tk } from "../i18n";
import { failure, type Message } from "../shell/Banner";
import SectionBar from "../shell/SectionBar";
import RefreshSchedule from "../sources/RefreshSchedule";
import type { VoltController } from "../volt/useVolt";
import { voltPart } from "../volt/VoltSettings";
import ClientUpdate from "./ClientUpdate";
import MaskForm from "./MaskForm";
import Page, { type Group } from "./Page";

/// Всё, что форма не может узнать сама: это опрашивает `App` — он же держит шапку,
/// и второй опрос статуса рядом с первым разошёлся бы с ним на такт.
///
/// Одним объектом, а не десятком отдельных свойств: по дороге сюда они проходят через
/// `ConfigEditor`, которому до них дела нет, и десять транзитных строк в его сигнатуре
/// были бы чистым шумом.
export type ClientProps = {
  volt: VoltController;
  settings: api.Settings;
  status: api.Status;
  /// Окно чем-то занято: пока идёт запуск, скачивание или сброс, отсюда ничего
  /// начинать нельзя.
  busy: boolean;
  /// Занято **именно скачиванием ядра**. Отдельно от `busy`, иначе кнопка ядра
  /// подписывается «Скачивание…» на сбросе, который её не касается.
  installing: boolean;
  updateInfo: api.UpdateInfo | null;
  checkingUpdate: boolean;
  updateProgress: api.UpdateProgress | null;
  onCheckUpdate: () => void;
  onClientUpdate: () => void;
  onUnsavedChange: (dirty: boolean) => void;
  onChange: (patch: api.SettingsPatch) => void;
  onInstall: () => void;
  onAutostart: (on: boolean) => void;
  onAlwaysAdmin: (on: boolean) => void;
  onKillSwitch: (on: boolean) => void;
  onReset: () => void;
  onElevate: () => void;
  /// Тумблер «qd»: включить — скачать, выключить — удалить бинарь (D-161).
  onQd: (on: boolean) => void;
  /// Тумблер маршрутизации (D-166) — в полосе раздела «Маршрутизация».
  onRouting: (on: boolean) => void;
  /// Открыть мастер настройки снова (D-162).
  onSetup: () => void;
  /// Настройки архивом (D-163).
  onExport: () => void;
  onStatus: (status: api.Status) => void;
  onLanguageChange: (language: LanguagePreference) => void;
};

type Props = ClientProps & {
  focusVolt?: boolean;
  onMessage: (message: Message | null) => void;
  /// Форма правит тот же документ, что открыт в коде (D-052): после записи его надо
  /// перечитать, иначе вкладка «Код» показывала бы текст до нажатия.
  onSaved: () => void;
  /// Левая часть полосы: переключатель документов (D-117).
  /// Левая половина полосы раздела: вид и документ (`SectionBar`).
  start: React.ReactNode;
  hint: string;
};

/// Способы по порядку «дешевле и грубее → дороже и точнее»: сверху меряется хост,
/// снизу — путь через сам узел.
const PINGS: { id: api.PingMethod; label: string; hint: string; icon: LucideIcon }[] = [
  {
    id: "icmp",
    label: api.PING_LABEL.icmp,
    hint: tk("Fastest, but many ISPs block it"),
    icon: Waves,
  },
  {
    id: "tcp",
    label: api.PING_LABEL.tcp,
    hint: tk("Also checks whether the port is open"),
    icon: Plug,
  },
  {
    id: "proxy",
    label: api.PING_LABEL.proxy,
    hint: tk("Closest to real use. Needs a connection"),
    icon: ArrowRightLeft,
  },
  {
    id: "proxy-keepalive",
    label: api.PING_LABEL["proxy-keepalive"],
    hint: tk("Leaves out the handshake. Needs a connection"),
    icon: Zap,
  },
];

/// Как часто перепрашивать страну узла (D-084). Ноль — не спрашивать вовсе: тогда наружу
/// не уходит ни один адрес.
/// Как часто группы клиента перепроверяют узлы (`health-interval`, секунд).
const RECHECK = [
  { seconds: 60, label: tk("Every minute") },
  { seconds: 300, label: tk("Every 5 minutes") },
  { seconds: 900, label: tk("Every 15 minutes") },
  { seconds: 3600, label: tk("Every hour") },
];

const GEO = [
  { hours: 0, label: tk("Never check") },
  { hours: 24, label: tk("Every day") },
  { hours: 168, label: tk("Every week") },
  { hours: 720, label: tk("Every month") },
];

/// Drop what the engine does not have (D-154): the list is the engine's, the form only obeys.
function without(groups: Group[], hidden: ReadonlySet<string>): Group[] {
  return groups
    .filter((group) => !hidden.has(group.id))
    .map((group) => ({
      ...group,
      parts: group.parts
        .filter((part) => !hidden.has(part.id))
        .map((part) => ({
          ...part,
          settings: part.settings.filter((setting) => !hidden.has(setting.id)),
        })),
    }));
}

const LAUNCHES: { id: api.Launch; label: string; hint: string }[] = [
  {
    id: "smart",
    label: tk("Smart"),
    hint: tk("At sign-in: start in tray. From a shortcut: open the window"),
  },
  { id: "window", label: tk("Open window"), hint: tk("Start with the window open") },
  {
    id: "tray",
    label: tk("Start in tray"),
    hint: tk("Start in tray; click the tray icon to open the window"),
  },
];

/// Идентификатор коротко: целиком это тридцать шесть символов, и строка превращается
/// в строку про них. Полностью — в подсказке.
const shortId = (id: string) => (id.length > 12 ? `${id.slice(0, 4)}…${id.slice(-4)}` : id);

/// Переключатель строки настройки. Подпись и подсказка у него уже есть — их несёт сама
/// строка (`Page.Row`), поэтому здесь только ввод и его имя для диктора.
const check = (label: string, on: boolean, act: (on: boolean) => void, off = false) => (
  <Switch
    aria-label={label}
    checked={on}
    disabled={off}
    onChange={(event) => act(event.target.checked)}
  />
);

/// Карточки выбора из списка «id, подпись, пояснение, значок».
const cards = <T extends string>(
  items: { id: T; label: string; hint: string; icon?: LucideIcon }[],
) =>
  items.map(({ id, label, hint, icon: Icon }) => ({
    value: id,
    label: t(label),
    description: t(hint),
    icon: Icon ? <Icon /> : undefined,
  }));

/**
 * Umiray Settings: всё, что решает сам клиент, а не ядро mihomo (D-068, D-089, D-117).
 *
 * Страница с оглавлением, а не две колонки карточек: список настроек стал длинным, и
 * вопрос «есть ли тут такое» важнее, чем плотность. Группы — о чём, подгруппы — про что
 * конкретно; глубже двух уровней не вкладываемся.
 *
 * Часть настроек живёт в `client.yaml` (замер, флаги, маска рукопожатия), часть —
 * в `settings.json` (запуск, защита), а часть вообще в системе (автозапуск, задача
 * в планировщике). Пользователю это различие не показано намеренно: он видит один
 * список решений про клиент, а не три хранилища.
 */
export default function ClientForm({
  focusVolt,
  volt,
  settings,
  status,
  busy,
  installing,
  updateInfo,
  checkingUpdate,
  updateProgress,
  onCheckUpdate,
  onClientUpdate,
  onUnsavedChange,
  onChange,
  onInstall,
  onAutostart,
  onAlwaysAdmin,
  onKillSwitch,
  onReset,
  onElevate,
  onQd,
  onSetup,
  onExport,
  onStatus,
  onLanguageChange,
  onMessage,
  onSaved,
  start,
  hint,
}: Props) {
  useEffect(() => {
    if (focusVolt) document.getElementById("antidpi-volt")?.scrollIntoView({ block: "start" });
  }, [focusVolt]);
  const [method, setMethod] = useCached<api.PingMethod | null>("client.ping", null);
  const [health, setHealth] = useCached<string | null>("client.health", null);
  const [udp, setUdp] = useCached<api.Udp | null>("client.udp", null);
  const [geo, setGeo] = useCached<number | null>("client.geo", null);
  const [recheck, setRecheckSeconds] = useCached<number | null>("client.recheck", null);
  const [device, setDevice] = useCached<string | null>("client.device", null);
  const [flushing, setFlushing] = useState(false);

  useEffect(() => {
    api.clientPingGet().then(setMethod, (e) => onMessage(failure(e)));
    api.clientGeoGet().then(setGeo, () => setGeo(null));
    api.clientHealthIntervalGet().then(setRecheckSeconds, () => setRecheckSeconds(null));
    api.clientHealthGet().then(setHealth, () => setHealth(null));
    api.udpGet().then(setUdp, () => setUdp(null));
    // Идентификатор устройства не меняется никогда — спрашиваем один раз при открытии,
    // а не держим в статусе, который опрашивается каждую секунду.
    api.systemDevice().then(setDevice, () => setDevice(null));
  }, [onMessage]);

  /// Показываем выбор сразу, но правду скажет диск: не записалось — возвращаем то,
  /// что там лежит. Окно не должно врать про содержимое документа.
  const choose = async (next: api.PingMethod) => {
    setMethod(next);
    onMessage(null);
    try {
      await api.clientPingSet(next);
      onSaved();
    } catch (e) {
      onMessage(failure(e));
      api.clientPingGet().then(setMethod, () => {});
    }
  };

  const setTarget = async (url: string) => {
    setHealth(url);
    onMessage(null);
    try {
      await api.clientHealthSet(url);
      onSaved();
    } catch (e) {
      onMessage(failure(e));
      api.clientHealthGet().then(setHealth, () => {});
    }
  };

  const setUdpGroup = async (on: boolean) => {
    setUdp((was) => (was === null ? was : { ...was, on }));
    onMessage(null);
    try {
      onStatus(await api.udpSet(on));
    } catch (e) {
      onMessage(failure(e));
    } finally {
      api.udpGet().then(setUdp, () => {});
    }
  };

  const setRecheck = async (seconds: number) => {
    setRecheckSeconds(seconds);
    try {
      onStatus(await api.clientHealthIntervalSet(seconds));
    } catch (e) {
      onMessage(failure(e));
      api.clientHealthIntervalGet().then(setRecheckSeconds, () => {});
    }
  };

  const setGeoHours = async (hours: number) => {
    setGeo(hours);
    try {
      await api.clientGeoSet(hours);
      onSaved();
    } catch (e) {
      onMessage(failure(e));
      api.clientGeoGet().then(setGeo, () => {});
    }
  };

  const flush = async () => {
    setFlushing(true);
    try {
      await api.coreFlushFakeIp();
      onMessage({ tone: "info", text: t("Fake-IP mappings cleared."), details: [] });
    } catch (error) {
      onMessage(failure(error));
    } finally {
      setFlushing(false);
    }
  };

  const groups: Group[] = [
    {
      id: "launch",
      label: t("Startup"),
      parts: [
        {
          id: "launch-windows",
          label: t("Sign-in"),
          settings: [
            {
              id: "autostart",
              label: t("Launch at sign-in"),
              inline: true,
              hint: status.alwaysAdmin
                ? t("via Windows Task Scheduler")
                : t("via the system startup list"),
              control: check(t("Launch at sign-in"), status.autostart, onAutostart),
            },
            {
              id: "always-admin",
              label: t("Always run as administrator"),
              inline: true,
              hint: status.alwaysAdmin
                ? t("Task installed: no UAC prompt, TUN starts right away.")
                : t("TUN needs admin rights. Creating the task asks for them now."),
              control: check(t("Always run as administrator"), status.alwaysAdmin, onAlwaysAdmin),
            },
          ],
        },
        {
          id: "launch-window",
          label: t("Window"),
          settings: [
            {
              id: "language",
              label: t("Interface language"),
              hint: t(
                "Automatic picks Russian if a Russian keyboard layout is installed, English otherwise.",
              ),
              control: (
                <Select
                  aria-label={t("Interface language")}
                  value={languagePreference()}
                  onChange={onLanguageChange}
                  options={[
                    { value: "auto", label: t("Automatic") },
                    { value: "en", label: "English" },
                    { value: "ru", label: t("Russian") },
                  ]}
                />
              ),
            },
            {
              id: "auto-connect",
              label: t("Connect on launch"),
              inline: true,
              hint: t("Same route and mode as last time."),
              control: check(t("Connect on launch"), settings.autoConnect, (autoConnect) =>
                onChange({ autoConnect }),
              ),
            },
            {
              id: "launch-mode",
              label: t("Launch behavior"),
              control: (
                <ChoiceCards
                  aria-label={t("Window behavior on launch")}
                  value={settings.launch}
                  options={cards(LAUNCHES)}
                  onChange={(launch) => onChange({ launch })}
                />
              ),
            },
          ],
        },
      ],
    },
    {
      id: "guard",
      label: t("Protection"),
      parts: [
        {
          id: "guard-firewall",
          label: t("Firewall"),
          settings: [
            {
              id: "kill-switch",
              label: "Kill-switch",
              inline: true,
              hint: settings.killSwitch
                ? status.killSwitch
                  ? t("Active: all traffic outside the tunnel is blocked")
                  : t("On but idle: works only in TUN mode while connected")
                : t(
                    "Blocks traffic that bypasses the tunnel. If the client crashes, internet stays blocked until restart",
                  ),
              // Галка показывает **намерение**, а подпись — в силе ли оно. Снимать галку
              // из-за того, что режим не TUN, значило бы отменять выбор пользователя за него.
              control: check("Kill-switch", settings.killSwitch, onKillSwitch),
            },
          ],
        },
      ],
    },
    {
      id: "antidpi",
      label: "Anti-DPI",
      parts: [
        voltPart(volt, busy, onSaved, onUnsavedChange, onElevate),
        {
          id: "antidpi-wireguard",
          label: "WireGuard",
          hint: t(
            "WireGuard can be spotted and blocked by its first packet. Junk packets before the handshake hide it.",
          ),
          settings: [
            {
              id: "mask",
              control: (
                <MaskForm
                  onMessage={onMessage}
                  onSaved={onSaved}
                  onUnsavedChange={onUnsavedChange}
                />
              ),
            },
          ],
        },
      ],
    },
    {
      id: "nodes",
      label: t("Nodes"),
      parts: [
        {
          id: "nodes-ping",
          label: t("Latency check"),
          hint: t(
            "If the selected check fails, ICMP is tried. Fallback results appear in blue with an icon.",
          ),
          settings: [
            {
              id: "ping",
              control: (
                <ChoiceCards
                  aria-label={t("Latency check method")}
                  value={method ?? undefined}
                  options={cards(PINGS.filter((ping) => ping.id !== "icmp" || has("icmpPing")))}
                  disabled={method === null}
                  onChange={choose}
                />
              ),
            },
          ],
        },
        {
          id: "nodes-health",
          label: t("Availability check"),
          settings: [
            {
              id: "health",
              label: t("Health check URL"),
              hint: t("Groups check availability at this URL. Only HTTP 204 counts as success"),
              control: (
                <Select
                  aria-label={t("Availability check URL")}
                  value={health ?? undefined}
                  disabled={health === null}
                  onChange={setTarget}
                  options={[
                    ...api.HEALTH_TARGETS.map((target) => ({
                      value: target.url,
                      label: target.label,
                    })),
                    ...(health !== null &&
                    !api.HEALTH_TARGETS.some((target) => target.url === health)
                      ? [{ value: health, label: t("Custom URL") }]
                      : []),
                  ]}
                />
              ),
            },
          ],
        },
        {
          id: "nodes-udp",
          label: "UDP",
          settings: [
            {
              id: "udp-group",
              label: t("UDP through compatible servers"),
              inline: true,
              hint:
                udp === null
                  ? t("Reading nodes…")
                  : udp.nodes === 0
                    ? t("No compatible nodes: add a hysteria2, tuic or wireguard source")
                    : t(
                        "Calls, games and QUIC go through hysteria2, tuic or wireguard ({n} nodes). Over vless and trojan, UDP rides inside TCP, and one lost packet holds up the rest.",
                        { n: udp.nodes },
                      ),
              control: check(
                t("Route UDP through compatible servers"),
                udp?.on ?? false,
                setUdpGroup,
                udp === null || udp.nodes === 0,
              ),
            },
          ],
        },
        {
          // Всё, что клиент обновляет сам, — рядом: подписки, флаги, проверка групп.
          id: "nodes-updates",
          label: t("Updates"),
          settings: [
            {
              id: "refresh",
              label: t("Subscriptions"),
              hint: t("New nodes from a subscription go into AUTO and auto groups by themselves"),
              control: (
                <span className="flex flex-wrap items-center justify-end gap-1.5">
                  <RefreshSchedule
                    schedule={settings.refresh}
                    onSchedule={(refresh) => onChange({ refresh })}
                  />
                </span>
              ),
            },
            {
              id: "recheck",
              label: t("Group re-check"),
              hint: t(
                "How often AUTO and auto groups check their nodes and set dead ones aside. Your own groups have their own interval",
              ),
              control: (
                <Select
                  aria-label={t("Group re-check interval")}
                  value={String(recheck ?? 300)}
                  disabled={recheck === null}
                  onChange={(value) => setRecheck(Number(value))}
                  options={[
                    ...RECHECK.map((item) => ({
                      value: String(item.seconds),
                      label: t(item.label),
                    })),
                    ...(recheck !== null && !RECHECK.some((item) => item.seconds === recheck)
                      ? [{ value: String(recheck), label: t("{n} s", { n: recheck }) }]
                      : []),
                  ]}
                />
              ),
            },
            {
              id: "geo",
              label: t("Country flags"),
              hint:
                geo === 0
                  ? t("Off: no addresses are sent anywhere, so no flags")
                  : t(
                      "Only the server address is sent to ipinfo.io, at most once per selected interval",
                    ),
              control: (
                <Select
                  aria-label={t("Country lookup interval")}
                  value={String(geo ?? 168)}
                  disabled={geo === null}
                  onChange={(value) => setGeoHours(Number(value))}
                  options={GEO.map((item) => ({ value: String(item.hours), label: t(item.label) }))}
                />
              ),
            },
          ],
        },
      ],
    },
    {
      id: "service",
      label: t("Maintenance"),
      parts: [
        {
          id: "service-client",
          label: t("Client"),
          settings: [
            {
              id: "client-update",
              label: t("Client version"),
              hint: (
                <>
                  {t("This version is also sent to subscription providers:")}{" "}
                  <Code>umiray/{api.VERSION}</Code>
                </>
              ),
              control: (
                <ClientUpdate
                  info={updateInfo}
                  checking={checkingUpdate}
                  progress={updateProgress}
                  busy={busy}
                  onCheck={onCheckUpdate}
                  onInstall={onClientUpdate}
                />
              ),
            },
            {
              id: "setup",
              label: t("Setup wizard"),
              hint: t("Subscription, capture and route, step by step. Your sources stay"),
              control: (
                <Button icon={<Wand2 />} disabled={busy} onClick={onSetup}>
                  {t("Open")}
                </Button>
              ),
            },
          ],
        },
        {
          id: "service-qd",
          label: "qd",
          settings: [
            {
              id: "qd",
              label: t("qd engine"),
              inline: true,
              hint: status.qdPresent
                ? t("Turning it off deletes the qd program; its link and settings stay")
                : t("Downloads qd from GitHub and shows the engine switch in the header"),
              control: check(t("qd engine"), status.qdPresent, onQd, busy),
            },
          ],
        },
        {
          id: "service-core",
          label: t("Mihomo core"),
          settings: [
            {
              id: "install",
              label: t("Update core"),
              hint: t("From the official GitHub release. Disconnect before replacing the core"),
              control: (
                <Button
                  icon={<Download />}
                  loading={installing}
                  disabled={busy}
                  onClick={onInstall}
                >
                  {t("Update")}
                </Button>
              ),
            },
          ],
        },
        {
          id: "service-data",
          label: t("Data"),
          settings: [
            {
              id: "device",
              label: t("Device ID"),
              hint: t(
                "HWID is how your provider knows this device. Each new one takes a device slot in the subscription",
              ),
              control: (
                <Tooltip content={settings.private ? undefined : (device ?? undefined)}>
                  <Code className="selectable">
                    {device === null ? "—" : settings.private ? "••••••••" : shortId(device)}
                  </Code>
                </Tooltip>
              ),
            },
            {
              id: "flush",
              label: t("Clear fake-IP mappings"),
              hint: status.running
                ? t("Hands out addresses again. An app holding an old one may hit the wrong rule")
                : t(
                    "Nothing to clear: domain-to-address mappings exist only while the core is running",
                  ),
              control: (
                <Button
                  icon={<Eraser />}
                  loading={flushing}
                  disabled={busy || !status.running}
                  onClick={flush}
                >
                  {t("Clear")}
                </Button>
              ),
            },
            {
              id: "export",
              label: t("Export settings"),
              hint: t(
                "A copy of the database with documents, sources and presets. Subscription addresses are inside; HWID is not",
              ),
              control: (
                <Button icon={<Database />} disabled={busy} onClick={onExport}>
                  {t("Export")}
                </Button>
              ),
            },
            {
              id: "reset",
              label: t("Reset to defaults"),
              hint: t("Deletes sources, rules, groups and settings. The core and HWID are kept"),
              control: (
                // Двойное подтверждение: действие необратимое и уносит источники.
                <ConfirmButton
                  variant="danger"
                  icon={<RotateCcw />}
                  disabled={busy}
                  confirmLabel={t("Reset everything?")}
                  onConfirm={onReset}
                >
                  {t("Reset")}
                </ConfirmButton>
              ),
            },
          ],
        },
      ],
    },
  ];

  return (
    <Page
      groups={without(
        groups,
        new Set([...ENGINES[settings.engine].hiddenClientSettings, ...unsupportedRows()]),
      )}
      // Кнопок записи нет: каждая настройка клиента пишется сразу (D-117).
      bar={<SectionBar start={start} hint={hint} />}
    />
  );
}
