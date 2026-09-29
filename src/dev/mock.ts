/**
 * Бэкенд-заглушка для `npm run dev` в обычном браузере (D-142).
 *
 * umiray — полигон rootik, и окно смотрят в браузере чаще, чем в Tauri: без бэкенда
 * там видно только ошибку моста. Заглушка отвечает на каждую команду правдоподобными
 * данными и держит состояние, чтобы нажатия что-то меняли. В сборку не попадает:
 * `main.tsx` грузит её под `import.meta.env.DEV` и только когда Tauri нет.
 *
 * Потолок: данные и правила выдуманы и знают о бэкенде ровно столько, сколько видно
 * из `api.ts`; поведение ядра проверяется только в настоящем окне.
 */
import type * as api from "../api";

const now = () => Math.floor(Date.now() / 1000);

let status: api.Status = {
  active: null,
  running: false,
  mode: null,
  desiredMode: "local",
  restartReason: null,
  trouble: null,
  port: 2080,
  corePresent: true,
  elevated: true,
  alwaysAdmin: false,
  systemProxy: false,
  foreignProxy: null,
  autostart: true,
  killSwitch: false,
  started: null,
};

let settings: api.Settings = {
  version: 1,
  engine: "mihomo",
  refresh: { onStart: true, everyMinutes: 1440 },
  theme: "midnight",
  scene: true,
  sceneBlur: 3,
  effects: true,
  private: false,
  killSwitch: false,
  autoConnect: false,
  launch: "smart",
  adminOffer: true,
};

const sources: api.Source[] = [
  {
    id: "demo",
    name: "Demo VPN",
    url: "https://panel.example.com/sub/abcdef123456",
    updated: now() - 3600,
    nodes: 9,
    records: false,
  },
  { id: "links", name: "My links", url: null, updated: null, nodes: 3, records: true },
];

const node = (
  name: string,
  kind: string,
  source: string,
  country: string | null,
  delay: number | null,
  extra: Partial<api.Node> = {},
): api.Node => ({
  name,
  kind,
  source,
  supported: true,
  delay,
  method: delay === null ? null : "tcp",
  fallback: false,
  address: `${name.toLowerCase().replace(/\W+/g, "")}.example.org:443`,
  country,
  edited: false,
  ...extra,
});

let nodes: api.Node[] = [
  node("Poland 1", "Hysteria2", "demo", "PL", 118),
  node("Netherlands 1", "Vless", "demo", "NL", 120),
  node("Netherlands 2", "Vless", "demo", "NL", null),
  node("Estonia 0", "Trojan", "demo", "EE", 90, { fallback: true, method: "icmp" }),
  node("Finland 0", "Vless", "demo", "FI", 102),
  node("Sweden 1", "Hysteria2", "demo", "SE", 92),
  node("Germany LTE", "Hysteria2", "demo", "DE", 193),
  node("UAE", "Vless", "demo", "AE", 95),
  node("Old SSR", "SSR", "demo", null, null, { supported: false }),
  node("vless-reality", "Vless", "links", "RU", 74, { edited: true }),
  node("vless-tls", "Vless", "links", "RU", 98),
  node("hy-tls", "Hysteria2", "links", "RU", 453),
];

let selected: string | null = "Poland 1";
let direction: api.Direction = "rules";
let ping: api.PingMethod = "tcp";
let health = "http://www.gstatic.com/generate_204";
let geo = 168;
let udp: api.Udp = { on: false, nodes: 4 };
let mask: api.Mask = {
  jc: 0,
  jmin: 0,
  jmax: 0,
  s1: 0,
  s2: 0,
  s3: 0,
  s4: 0,
  h1: 0,
  h2: 0,
  h3: 0,
  h4: 0,
};
let advanced: api.Advanced = {
  logLevel: "info",
  mixedPort: 2080,
  sniffer: true,
  stack: "mixed",
  device: "",
  mtu: 0,
  strictRoute: false,
  dnsHijack: ["any:53"],
  dnsEnable: true,
  enhancedMode: "fake-ip",
  nameserver: ["https://1.1.1.1/dns-query", "tls://8.8.8.8"],
};

const groups: api.Group[] = [
  {
    name: "Europe",
    kind: "url-test",
    sources: ["demo"],
    proxies: [],
    filter: null,
    url: "http://www.google.com/generate_204",
    interval: 300,
    tolerance: 150,
    strategy: null,
    extra: [],
    origin: 0,
  },
  {
    name: "My servers",
    kind: "select",
    sources: [],
    proxies: ["vless-reality", "vless-tls", "hy-tls"],
    filter: null,
    url: null,
    interval: null,
    tolerance: null,
    strategy: null,
    extra: [],
    origin: 1,
  },
  {
    name: "Backup",
    kind: "fallback",
    sources: [],
    proxies: ["Finland 0", "Sweden 1", "Estonia 0"],
    filter: null,
    url: "http://www.google.com/generate_204",
    interval: 300,
    tolerance: null,
    strategy: null,
    extra: ["lazy: true"],
    origin: 2,
  },
];

let presets = [
  { id: "default", name: "Default", applied: true },
  { id: "work", name: "Work", applied: false },
];

const routing: Record<string, api.Routing> = {
  default: {
    rules: [
      {
        kind: "DOMAIN-SUFFIX",
        values: ["github.com", "githubusercontent.com"],
        target: "umiray",
        options: [],
      },
      { kind: "DOMAIN-SUFFIX", values: ["ya.ru", "yandex.ru"], target: "DIRECT", options: [] },
      { kind: "GEOIP", values: ["RU"], target: "DIRECT", options: [] },
      { kind: "PROCESS-NAME", values: ["Telegram.exe"], target: "Europe", options: [] },
    ],
    fallback: "umiray",
  },
  work: { rules: [], fallback: "DIRECT" },
};

let rulesets: api.Ruleset[] = [
  {
    id: "ai",
    title: "AI services",
    on: true,
    rules: ["DOMAIN-SUFFIX,openai.com", "DOMAIN-SUFFIX,chatgpt.com"],
  },
  { id: "streaming", title: "Streaming", on: false, rules: ["DOMAIN-SUFFIX,netflix.com"] },
];

const docs: Record<string, string> = {
  client: "# настройки клиента\nping: tcp\n",
  advanced: "mixed-port: 2080\nlog-level: info\n",
};

const text = (value: unknown) => JSON.stringify(value, null, 2);

const sections = (): api.ConfigSection[] => [
  {
    id: "groups",
    label: "Groups",
    presets: false,
    docs: [
      {
        id: "groups",
        label: "Groups",
        hint: "your node groups, shared across routes; AUTO and umiray are assembled by the client",
        core: true,
        applied: true,
      },
    ],
  },
  {
    id: "rules",
    label: "Routing",
    presets: true,
    docs: presets.map((preset) => ({
      id: `rules/${preset.id}`,
      label: preset.name,
      hint: "where traffic goes; MATCH handles everything else",
      core: true,
      applied: preset.applied,
    })),
  },
  {
    id: "advanced",
    label: "Settings",
    presets: false,
    docs: [
      { id: "client", label: "Umiray Settings", hint: "", core: false, applied: true },
      { id: "advanced", label: "Mihomo Settings", hint: "", core: true, applied: true },
    ],
  },
];

let up = 0;
let down = 0;
const traffic = (): api.Traffic | null => {
  if (!status.running) return null;
  down += 40_000 + Math.random() * 900_000;
  up += 5_000 + Math.random() * 90_000;
  return {
    up,
    down,
    connections: 5,
    nodes: [{ node: "Poland 1", up, down, connections: 3 }],
  };
};

const log = (level: string, msg: string) =>
  `time="2026-09-26T10:${String(Math.floor(Math.random() * 60)).padStart(2, "0")}:00Z" level=${level} msg="${msg}"`;

const logs = [
  log("info", "Start initial configuration in progress"),
  log("info", "Mixed(http+socks) proxy listening at: 127.0.0.1:2080"),
  log("warning", "[DNS] resolve timeout: example.invalid"),
  log("error", "dial tcp 1.2.3.4:443: i/o timeout"),
  log("info", "umiray: start · config · 6 ms · ok"),
  log("info", "umiray: start · spawn · 120 ms · ok"),
  "panic: runtime error: invalid memory address (raw output example)",
];

const tools: api.Tool[] = [
  {
    id: "dns",
    title: "Resolver race",
    group: "Network",
    hint: "which resolvers respond quickly and reliably",
    params: ["domain", "all"],
    network: true,
  },
  {
    id: "mtu",
    title: "Path MTU",
    group: "Network",
    hint: "largest packet without fragmentation",
    params: ["host", "timeout"],
    network: true,
  },
  {
    id: "clock",
    title: "Clock",
    group: "System",
    hint: "whether the system clock matches network time",
    params: [],
    network: true,
  },
];

/// Работающее ядро — одно (D-154): mihomo, если запущен, иначе qd, если поднят.
const status_ = (): api.Status => {
  const active = status.running ? "mihomo" : qdUp ? "qd" : null;
  const started = active === "mihomo" ? status.started : active === "qd" ? qdSince : null;
  return { ...status, active, started };
};

type Args = Record<string, unknown>;

let qdUp = false;
let qdSince: number | null = null;
let qdFlags = { egress: false, adblock: true };
const qdNodes = [
  { id: 1, name: "Entry A", role: "ingress", reachable: true, selected: true, latencyMs: 9 },
  { id: 2, name: "Entry B", role: "ingress", reachable: true, selected: false, latencyMs: 41 },
];
let qdRouting = {
  defaultRole: "direct",
  allowExit: true,
  rules: [
    {
      id: 1,
      process: "chrome.exe",
      path: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
      role: "tunnel",
    },
    { id: 2, process: "Telegram.exe", role: "egress" },
  ],
};
let qdSettings = { refreshMinutes: 480, refreshPinned: false, fixedRate: 0, ratePinned: false };
const qdState = () => ({
  imported: true,
  connected: qdUp,
  node: qdUp ? qdNodes[0] : null,
  nodes: { total: qdNodes.length, reachable: qdNodes.length },
  ...qdFlags,
  allowExit: true,
  subscription: {
    lastRefresh: Date.now() - 3_600_000,
    intervalMinutes: qdSettings.refreshMinutes,
    expiresAt: Date.now() + 30 * 86_400_000,
  },
  failed: false,
});
const qdCall = (method: string, path: string, body: Record<string, unknown> | null): unknown => {
  const at = path.replace("/client/api/", "");
  if (at === "state") return qdState();
  if (at === "connect" || at === "disconnect") {
    qdUp = at === "connect";
    qdSince = qdUp ? now() : null;
    return qdState();
  }
  if (at === "toggle") {
    qdFlags = { ...qdFlags, ...body };
    return qdState();
  }
  if (at === "nodes") return qdNodes;
  if (at.startsWith("history/"))
    return {
      window: 60,
      points: Array.from({ length: 60 }, (_, i) => ({
        t: i,
        down: qdUp ? 40_000 + ((i * 7919) % 90_000) : 0,
        up: qdUp ? 8_000 + ((i * 104729) % 20_000) : 0,
      })),
    };
  if (at === "about")
    return {
      tag: "demo",
      createdAt: Date.now() - 86_400_000,
      up: 180_000_000,
      down: 2_400_000_000,
      expiresAt: Date.now() + 30 * 86_400_000,
    };
  if (at === "settings") {
    if (method === "POST") qdSettings = { ...qdSettings, ...body };
    return qdSettings;
  }
  if (at === "routing") {
    if (method === "POST") qdRouting = { ...qdRouting, ...(body as typeof qdRouting) };
    return qdRouting;
  }
  if (at === "routing/processes")
    return [
      {
        name: "firefox.exe",
        path: "C:\\Program Files\\Mozilla Firefox\\firefox.exe",
        connections: 4,
      },
      ...Array.from({ length: 5 }, () => ({
        name: "chrome.exe",
        path: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
        connections: 3,
      })),
      {
        name: "Telegram.exe",
        path: "C:\\Users\\demo\\AppData\\Roaming\\Telegram Desktop\\Telegram.exe",
      },
      { name: "svchost.exe", path: "C:\\Windows\\System32\\svchost.exe" },
    ];
  if (at === "subscription/refresh") return { nodes: qdNodes.length };
  if (at === "routing/export") return { code: "qdr", name: "rules.qdr" };
  if (at === "routing/import") return { rules: 2 };
  return qdState();
};

const qdLogs = ["qd  embedded: api on 127.0.0.1:52100", "tunnel  up via Entry A, 9 ms"];

const HANDLERS: Record<string, (args: Args) => unknown> = {
  core_status: status_,
  core_start: () => {
    // Клиент гасит остальные ядра сам (D-154): поднимается выбранное в шапке.
    if (settings.engine === "qd") {
      status = { ...status, running: false, mode: null, systemProxy: false, started: null };
      qdCall("POST", "/client/api/connect", null);
      return status_();
    }
    qdCall("POST", "/client/api/disconnect", null);
    status = {
      ...status,
      running: true,
      mode: status.desiredMode === "tun" ? "tun" : "local",
      systemProxy: status.desiredMode === "system",
      started: now() - 5940,
    };
    return status_();
  },
  core_stop: () => {
    qdCall("POST", "/client/api/disconnect", null);
    status = { ...status, running: false, mode: null, systemProxy: false, started: null };
    return status_();
  },
  core_restart: () => HANDLERS.core_start({}),
  core_traffic: traffic,
  core_logs: ({ engine }) => (engine === "qd" ? qdLogs : logs),
  core_install: ({ engine }) => (engine === "qd" ? "v0.1.4-alpha" : "v1.19.0"),
  core_flush_fake_ip: () => null,
  mode_set: ({ mode }) => {
    status = { ...status, desiredMode: mode as api.Choice };
    status.restartReason =
      status.running && (mode === "tun") !== (status.mode === "tun")
        ? "Режим сменился — доедет перезапуском VPN."
        : null;
    return status_();
  },
  settings_get: () => settings,
  settings_update: ({ patch }) => {
    settings = { ...settings, ...(patch as api.SettingsPatch) };
    return settings;
  },
  config_list: sections,
  config_read: ({ id }) => {
    const key = String(id);
    if (key === "groups") return text(groups);
    if (key.startsWith("rules/")) return text(routing[key.slice(6)] ?? routing.default);
    return docs[key] ?? "";
  },
  config_write: ({ id, text: body }) => {
    docs[String(id)] = String(body);
    return status_();
  },
  config_reset: ({ id }) => docs[String(id)] ?? "",
  config_assembled: () =>
    `mixed-port: 2080\nproxies:\n${nodes.map((item) => `  - name: ${item.name}`).join("\n")}\nrules:\n  - MATCH,umiray\n`,
  groups_parse: ({ text: body }) => JSON.parse(String(body)),
  groups_render: ({ groups: list }) => text(list),
  rules_parse: ({ text: body }) => JSON.parse(String(body)),
  rules_render: ({ routing: value }) => text(value),
  rules_processes: () => [{ name: "chrome.exe" }, { name: "notepad.exe" }],
  sources_list: () => sources,
  sources_add: ({ input }) => {
    if (!String(input).includes("://"))
      throw {
        kind: "badInput",
        message: "Это не ссылка: нет схемы вроде https:// или vless://",
        details: [],
      };
    return { notices: [], source: sources[0] };
  },
  sources_refresh: () => ({ notices: ["Панель прислала 9 узлов"], source: sources[0] }),
  sources_refresh_all: () => [],
  sources_delete: () => null,
  sources_read: ({ id }) =>
    id === "links" ? "vless://uuid@host:443#vless-reality\n" : "dmxlc3M6Ly8...base64\n",
  sources_write: () => ({ notices: [], source: sources[1] }),
  sources_add_proxy: () => ({ notices: [], source: sources[1] }),
  sources_add_proxy_text: () => ({ notices: [], source: sources[1] }),
  sources_proxy_yaml: ({ entry }) => text(entry),
  sources_add_file: () => null,
  nodes_list: () => nodes,
  nodes_ping: () => {
    nodes = nodes.map((item) =>
      item.supported
        ? { ...item, delay: 60 + Math.round(Math.random() * 300), method: ping }
        : item,
    );
    return null;
  },
  // Как у живого ядра: выбранное и то, куда оно ведёт (D-145). В Auto и Rules группу
  // разворачивает только работающее ядро.
  connection_snapshot: () => ({
    nodes,
    direction,
    ping,
    route:
      direction === "direct"
        ? ["DIRECT"]
        : direction === "manual" || !status.running
          ? selected === null
            ? []
            : [selected]
          : [direction === "rules" ? "CUSTOM-FALLBACK" : "AUTO", nodes[1].name],
  }),
  nodes_code: ({ source }) =>
    source === "links"
      ? {
          text: "name: vless-reality\ntype: vless\nserver: host\nport: 443\n",
          editable: true,
          entry: { name: "vless-reality", type: "vless", server: "host", port: 443, uuid: "0000" },
          why: null,
        }
      : {
          text: "vless://uuid@host:443#node",
          editable: false,
          entry: null,
          why: "Узел пришёл ссылкой: её читает ядро, правится только параметром ссылки.",
        },
  nodes_entry_set: () => null,
  nodes_delete: () => null,
  nodes_code_set: () => null,
  nodes_reset: () => null,
  direction_set: ({ direction: next, node: name }) => {
    direction = next as api.Direction;
    if (name) selected = String(name);
    return status_();
  },
  presets_create: () => {
    const preset = {
      id: `p${presets.length}`,
      name: `Набор ${presets.length + 1}`,
      applied: false,
    };
    presets = [...presets, preset];
    routing[preset.id] = { rules: [], fallback: "umiray" };
    return { id: preset.id, name: preset.name, created: now() };
  },
  presets_select: ({ id }) => {
    presets = presets.map((preset) => ({ ...preset, applied: preset.id === id }));
    return null;
  },
  presets_rename: ({ id, name }) => {
    presets = presets.map((preset) =>
      preset.id === id ? { ...preset, name: String(name) } : preset,
    );
    return null;
  },
  presets_delete: ({ id }) => {
    if (presets.find((preset) => preset.id === id)?.applied)
      throw {
        kind: "refused",
        message: "Применённый набор удалить нельзя — сначала примените другой.",
        details: [],
      };
    presets = presets.filter((preset) => preset.id !== id);
    return null;
  },
  rulesets_list: () => rulesets,
  rulesets_set: ({ id, on }) => {
    rulesets = rulesets.map((set) => (set.id === id ? { ...set, on: Boolean(on) } : set));
    return status_();
  },
  rulesets_read: ({ id }) =>
    `title: ${rulesets.find((set) => set.id === id)?.title}\nrules:\n  - DOMAIN-SUFFIX,example.com\n`,
  rulesets_write: status_,
  rulesets_create: ({ title }) => {
    const id = `set${rulesets.length}`;
    rulesets = [...rulesets, { id, title: String(title), on: false, rules: [] }];
    return id;
  },
  rulesets_delete: ({ id }) => {
    rulesets = rulesets.filter((set) => set.id !== id);
    return status_();
  },
  udp_get: () => udp,
  udp_set: ({ on }) => {
    udp = { ...udp, on: Boolean(on) };
    return status_();
  },
  client_geo_get: () => geo,
  client_geo_set: ({ hours }) => {
    geo = Number(hours);
    return null;
  },
  client_mask_get: () => mask,
  client_mask_set: ({ mask: next }) => {
    mask = next as api.Mask;
    return mask;
  },
  client_ping_get: () => ping,
  client_ping_set: ({ method }) => {
    ping = method as api.PingMethod;
    return null;
  },
  client_health_get: () => health,
  client_health_set: ({ url }) => {
    health = String(url);
    return null;
  },
  advanced_get: () => advanced,
  advanced_set: ({ options }) => {
    advanced = options as api.Advanced;
    return advanced;
  },
  system_device: () => "5f3c9a2e-1b7d-4c8e-9a0f-6d2b1e4c7a90",
  system_language: () => (navigator.language.startsWith("ru") ? "ru" : "en"),
  system_reset: status_,
  system_autostart_set: ({ on }) => {
    status = { ...status, autostart: Boolean(on) };
    return status_();
  },
  system_always_admin_set: ({ on }) => {
    status = { ...status, alwaysAdmin: Boolean(on) };
    return status_();
  },
  system_kill_switch_set: ({ on }) => {
    settings = { ...settings, killSwitch: Boolean(on) };
    return status_();
  },
  system_relaunch_elevated: () => null,
  updates_check: () => ({ enabled: false, version: null, notes: null }),
  updates_install: () => null,
  qd_status: () => ({
    present: true,
    elevated: true,
    running: true,
    state: qdState(),
    problem: null,
  }),
  qd_call: ({ method, path, body }) =>
    qdCall(String(method), String(path), (body as Record<string, unknown>) ?? null),
  qd_rules_export: () => "C:\\Users\\demo\\rules.qdr",
  qd_rules_import: () => ({ rules: 2 }),
  diag_tools: () => tools,
  diag_run: ({ id }) => ({
    tool: id,
    verdict: "warn",
    headline: "5 из 9",
    ms: 840,
    columns: ["резолвер", "ответ", "время"],
    rows: [
      { cells: ["1.1.1.1", "93.184.216.34", "12 мс"], verdict: "ok", mark: true },
      { cells: ["8.8.8.8", "93.184.216.34", "31 мс"], verdict: "ok", mark: false },
      { cells: ["провайдер", "10.10.10.10", "4 мс"], verdict: "bad", mark: false },
    ],
    lines: [
      { tone: "info", text: `${id}: старт` },
      { tone: "ok", text: "1.1.1.1 — 12 мс" },
      { tone: "warn", text: "провайдер подменяет ответ" },
      { tone: "dim", text: "готово за 840 мс" },
    ],
  }),
  diag_apply: ({ id }) => HANDLERS.diag_run({ id }),
  diag_providers: () => ({ version: 1, providers: [] }),
};

document.documentElement.dataset.demo = "true";
(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
  invoke: async (cmd: string, args: Args = {}) => {
    await new Promise((resolve) => setTimeout(resolve, 60));
    const handler = HANDLERS[cmd];
    if (!handler)
      throw { kind: "unknown", message: `Заглушка не знает команду ${cmd}`, details: [] };
    return structuredClone(handler(args));
  },
  transformCallback: () => 0,
};
