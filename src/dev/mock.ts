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

/// `?fresh` — первый запуск: ни источников, ни qd, мастер не пройден (D-162).
const fresh = new URLSearchParams(location.search).has("fresh");

let status: api.Status = {
  active: null,
  running: false,
  mode: null,
  desiredMode: "local",
  restartReason: null,
  trouble: null,
  port: 2080,
  corePresent: true,
  qdPresent: !fresh,
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
  setup: !fresh,
  routing: true,
};

const DEMO_SOURCES: api.Source[] = [
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
let sources: api.Source[] = fresh ? [] : DEMO_SOURCES;

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

const DEMO_NODES: api.Node[] = [
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
let nodes: api.Node[] = fresh ? [] : DEMO_NODES;

let selected: string | null = fresh ? null : "Poland 1";
let direction: api.Direction = fresh ? "direct" : "auto";
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
  openNat: false,
  dnsEnable: true,
  enhancedMode: "fake-ip",
  nameserver: ["https://1.1.1.1/dns-query", "tls://8.8.8.8"],
  preferH3: false,
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
    ruleSets: [
      { id: "itdog-inside", target: "umiray" },
      { id: "antizapret", target: "AUTO" },
    ],
    ready: [{ id: "ai" }],
  },
  work: { rules: [], fallback: "DIRECT", ruleSets: [], ready: [] },
};

let rulesets: api.Ruleset[] = [
  {
    id: "ai",
    title: "AI services",
    target: "umiray",
    rules: ["DOMAIN-SUFFIX,openai.com,umiray", "DOMAIN-SUFFIX,chatgpt.com,umiray"],
  },
  {
    id: "streaming",
    title: "Streaming",
    target: "DIRECT",
    rules: ["DOMAIN-SUFFIX,netflix.com,DIRECT"],
  },
];

/// Rule sets (D-157): one fresh, one stale — so the age badge has something to show.
const DAY = 86_400;
const listOffers: Omit<api.ListOffer, "added">[] = [
  {
    id: "itdog-inside",
    title: "Russia inside — itdoginfo",
    group: "blocked",
    note: "заблокированное и закрытое для России",
    noteEn: "blocked in Russia and geo-blocked for it",
    urls: ["https://raw.githubusercontent.com/itdoginfo/allow-domains/main/Russia/inside-raw.lst"],
  },
  {
    id: "antizapret",
    title: "antizapret",
    group: "blocked",
    note: "реестр целиком",
    noteEn: "the whole registry",
    urls: [
      "https://github.com/savely-krasovsky/antizapret-sing-box/releases/latest/download/antizapret-ruleset.json",
    ],
  },
  {
    id: "telegram",
    title: "Telegram",
    group: "services",
    note: "домены и подсети",
    noteEn: "domains and subnets",
    urls: ["https://raw.githubusercontent.com/itdoginfo/allow-domains/main/Services/telegram.lst"],
  },
  {
    id: "geoip-ru",
    title: "Российские адреса",
    titleEn: "Russian addresses",
    group: "russia",
    note: "geoip:ru",
    urls: ["https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/meta/geo/geoip/ru.list"],
  },
];
let ruleLists: api.RuleList[] = [
  {
    id: "itdog-inside",
    title: "Russia inside — itdoginfo",
    urls: listOffers[0].urls,
    updated: now() - 3600,
    published: null,
    domains: 1183,
    cidrs: 0,
    skipped: 0,
  },
  {
    id: "antizapret",
    title: "antizapret",
    urls: listOffers[1].urls,
    updated: now() - 7200,
    published: now() - 188 * DAY,
    domains: 982094,
    cidrs: 337343,
    skipped: 6,
  },
];
let geoFiles: api.GeoFile[] = [
  { name: "geoip.metadb", modified: now() - 2 * DAY },
  { name: "GeoSite.dat", modified: now() - 2 * DAY },
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
        hint: "your node groups, shared by all routes; the client builds AUTO and umiray itself",
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

const TUNED: Record<string, api.Report> = {
  recommended: {
    tool: "recommended",
    verdict: "ok",
    headline:
      "Конфиг ядра: рекомендованный, DNS и сниффер — во всех режимах. Рискованное: dns.prefer-h3, tun.endpoint-independent-nat",
  },
  "dns-race": {
    tool: "dns-race",
    verdict: "ok",
    headline: "DNS: Quad9 DNS (DoH), Control D (DoH), Cloudflare DNS (DoH)",
  },
  pmtu: { tool: "pmtu", verdict: "ok", headline: "MTU: 1440" },
};

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
let qdImported = !fresh;
const qdState = () => ({
  imported: qdImported,
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
  core_install: ({ engine }) => {
    if (engine !== "qd") return "v1.19.0";
    status = { ...status, qdPresent: true };
    return "v0.1.5";
  },
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
    const key = String(id);
    // Набор живёт разобранным: его читают и форма, и снимок «Соединения» (MATCH, D-166).
    if (key.startsWith("rules/")) routing[key.slice(6)] = JSON.parse(String(body));
    else docs[key] = String(body);
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
    if (sources.length === 0) {
      sources = DEMO_SOURCES;
      nodes = DEMO_NODES;
      // Как бэкенд (D-056): первый источник переводит DIRECT в автовыбор.
      if (direction === "direct") direction = "auto";
    }
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
  sources_add_warp: () => ({ notices: [], source: sources[1] }),
  nodes_list: () => nodes,
  nodes_ping: () => {
    nodes = nodes.map((item) =>
      item.supported
        ? { ...item, delay: 60 + Math.round(Math.random() * 300), method: ping }
        : item,
    );
    return null;
  },
  // Как у живого ядра: выбранное и то, куда оно ведёт (D-145). В Auto группу
  // разворачивает только работающее ядро; MATCH мимо псевдонима называет набор (D-166).
  connection_snapshot: () => {
    const active = presets.find((preset) => preset.applied)?.id;
    const target = active === undefined ? undefined : routing[active]?.fallback;
    const fallback = settings.routing && target && target !== "umiray" ? target : null;
    return {
      nodes,
      direction,
      node: direction === "manual" ? selected : null,
      fallback,
      ping,
      route:
        fallback !== null
          ? [fallback]
          : direction === "direct"
            ? ["DIRECT"]
            : direction === "manual" || !status.running
              ? selected === null
                ? []
                : [selected]
              : ["AUTO", nodes[1].name],
    };
  },
  // Как бэкенд с D-122: у всякого узла есть запись — ссылка разбирается в неё же.
  nodes_code: ({ node: name }) => ({
    text: `name: ${name}\ntype: vless\nserver: host\nport: 443\nuuid: "0000"\n`,
    editable: true,
    entry: { name, type: "vless", server: "host", port: 443, uuid: "0000" },
    why: null,
  }),
  nodes_entry_set: () => null,
  nodes_delete: () => null,
  nodes_code_set: () => null,
  nodes_reset: () => null,
  direction_set: ({ direction: next, node: name }) => {
    direction = next as api.Direction;
    if (name) selected = String(name);
    return status_();
  },
  routing_set: ({ on }) => {
    settings = { ...settings, routing: Boolean(on) };
    return status_();
  },
  // Как бэкенд: готовый набор в применённом наборе, включённый — с маршрутизацией.
  routing_ads_set: ({ on }) => {
    const id = presets.find((preset) => preset.applied)?.id ?? presets[0].id;
    const ready = (routing[id].ready ?? []).filter((use) => use.id !== "block-ads");
    routing[id] = { ...routing[id], ready: on ? [...ready, { id: "block-ads" }] : ready };
    if (on) settings = { ...settings, routing: true };
    return status_();
  },
  // Как бэкенд: наборы и применённый — тот, что решает маршрут.
  presets_list: () => ({
    presets: presets.map(({ id, name }) => ({ id, name, created: now() })),
    active: presets.find((preset) => preset.applied)?.id ?? presets[0]?.id ?? null,
  }),
  presets_create: () => {
    const preset = {
      id: `p${presets.length}`,
      name: `Набор ${presets.length + 1}`,
      applied: false,
    };
    presets = [...presets, preset];
    routing[preset.id] = { rules: [], fallback: "umiray", ruleSets: [], ready: [] };
    return { id: preset.id, name: preset.name, created: now() };
  },
  presets_select: ({ id }) => {
    presets = presets.map((preset) => ({ ...preset, applied: preset.id === id }));
    settings = { ...settings, routing: true };
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
  lists_list: () => ruleLists,
  lists_catalog: () => listOffers,
  lists_fetch: ({ id }) => {
    const cached = ruleLists.find((list) => list.id === id);
    if (cached) return cached;
    const offer = listOffers.find((item) => item.id === id);
    if (!offer) throw { kind: "invalid", message: `В каталоге нет списка ${id}`, details: [] };
    const list: api.RuleList = {
      id: offer.id,
      title: offer.title,
      titleEn: offer.titleEn,
      urls: offer.urls,
      updated: now(),
      published: now() - DAY,
      domains: 20,
      cidrs: 14,
      skipped: 0,
    };
    ruleLists = [...ruleLists, list];
    return list;
  },
  lists_add_url: ({ url, title }) => {
    const name = String(title) || String(url).split("/").pop()?.split(".")[0] || "list";
    const list: api.RuleList = {
      id: name.toLowerCase().replace(/[^a-z0-9а-я]+/g, "-"),
      title: name,
      urls: [String(url)],
      updated: now(),
      domains: 42,
      cidrs: 0,
      skipped: 0,
    };
    ruleLists = [...ruleLists, list];
    return list;
  },
  lists_refresh: ({ id }) => {
    ruleLists = ruleLists.map((list) =>
      id === null || list.id === id ? { ...list, updated: now(), error: null } : list,
    );
    return null;
  },
  lists_ensure: () => null,
  geo_files: () => geoFiles,
  geo_update: () => {
    if (!status.running) throw { kind: "invalid", message: "Ядро не запущено", details: [] };
    geoFiles = geoFiles.map((file) => ({ ...file, modified: now() }));
    return geoFiles;
  },
  rulesets_list: () => rulesets,
  rulesets_read: ({ id }) =>
    `title: ${rulesets.find((set) => set.id === id)?.title}\nrules:\n  - DOMAIN-SUFFIX,example.com\n`,
  rulesets_write: status_,
  rulesets_create: ({ title }) => {
    const id = `set${rulesets.length}`;
    rulesets = [
      ...rulesets,
      { id, title: String(title), target: "DIRECT", rules: ["DOMAIN-SUFFIX,example.com,DIRECT"] },
    ];
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
    present: status.qdPresent,
    elevated: true,
    running: status.qdPresent,
    state: status.qdPresent ? qdState() : null,
    problem: null,
  }),
  qd_adopt: async ({ link }) => {
    if (!String(link).toLowerCase().startsWith("qd://"))
      throw { kind: "invalid", message: "Это не ссылка qd", details: [] };
    const downloaded = status.qdPresent ? null : "v0.1.5";
    // Загрузка идёт секунды — ровно то, что показывает тост.
    if (downloaded) await new Promise((resolve) => setTimeout(resolve, 1500));
    status = { ...status, qdPresent: true };
    qdImported = true;
    return { downloaded, pending: false };
  },
  qd_remove: () => {
    if (qdUp) throw { kind: "invalid", message: "Сначала отключите qd", details: [] };
    status = { ...status, qdPresent: false };
    settings = { ...settings, engine: "mihomo" };
    return null;
  },
  system_export: () => "C:\\Users\\demo\\Documents\\umiray-settings.db",
  system_open_github: ({ url }) => {
    window.open(String(url), "_blank", "noopener");
    return null;
  },
  qd_call: ({ method, path, body }) =>
    qdCall(String(method), String(path), (body as Record<string, unknown>) ?? null),
  qd_rules_export: () => "C:\\Users\\demo\\rules.qdr",
  qd_rules_import: () => ({ rules: 2 }),
  // Те же строки, что пишет бэкенд (`diag/smart.rs`): мастер показывает их как есть.
  diag_apply: ({ id }) => TUNED[String(id)] ?? { tool: id, verdict: "idle", headline: "—" },
};

document.documentElement.dataset.demo = "true";
(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
  invoke: async (cmd: string, args: Args = {}) => {
    await new Promise((resolve) => setTimeout(resolve, 60));
    const handler = HANDLERS[cmd];
    if (!handler)
      throw { kind: "unknown", message: `Заглушка не знает команду ${cmd}`, details: [] };
    return structuredClone(await handler(args));
  },
  transformCallback: () => 0,
};
