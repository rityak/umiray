/**
 * Which fields make up a node when it is added by hand (D-120).
 *
 * Field names come **from the mihomo docs** (`wiki.metacubex.one/config/proxies`), not
 * from memory: the core silently accepts an invented key and just as silently ignores
 * it — exactly the trouble the node editor exists for (S-022).
 *
 * Only description and assembly live here. No requests, no state: the window draws from
 * this list, and assembly can be checked with a plain test.
 */

import { tk } from "../i18n";

export type Kind = "text" | "secret" | "number" | "bool" | "select" | "multi" | "numbers";

export type Field = {
  /// Key in the core config. A dot means nesting: `ws-opts.path` becomes `ws-opts: { path: … }`.
  key: string;
  label: string;
  kind: Kind;
  options?: string[];
  hint?: string;
  placeholder?: string;
  /// The node will not come up without it. Checked at assembly, not only highlighted.
  need?: boolean;
  /// When the field makes sense at all. vless with `reality` and with `tls` has different
  /// fields, and a ws path means nothing for grpc — showing everything at once invites junk.
  /// A hidden field is **not written** either: typed a path, changed your mind — it stays out.
  when?: (values: Values) => boolean;
  /// An option's parameter (D-131): the key of the checkbox it sits under. Visible only while
  /// that checkbox is on and visible itself — and drawn as a block under it, not as a peer.
  under?: string;
  /// A window field, not a core one: vless `security` is `tls` plus the presence of
  /// `reality-opts`. It never lands in the entry; assembly reads it.
  ui?: true;
};

export type Values = Record<string, string>;

export type Part = { title: string; fields: Field[] };
export type Protocol = { id: string; label: string; parts: Part[] };

const NAME: Field = { key: "name", label: tk("Name"), kind: "text", need: true };
const SERVER: Field = {
  key: "server",
  label: tk("Address"),
  kind: "text",
  need: true,
  placeholder: tk("example.com or 1.2.3.4"),
};
const PORT: Field = { key: "port", label: tk("Port"), kind: "number", need: true };
const UDP: Field = {
  key: "udp",
  label: "UDP",
  kind: "bool",
  hint: tk("without it games and calls bypass the tunnel"),
};

const basics = (...extra: Field[]): Part => ({
  title: tk("Basics"),
  fields: [NAME, SERVER, PORT, ...extra],
});

const FINGERPRINTS = [
  "chrome",
  "firefox",
  "safari",
  "ios",
  "android",
  "edge",
  "360",
  "qq",
  "random",
];

/// How the channel is protected is the window's question, not the core's: in mihomo it is
/// `tls: true` plus the presence of `reality-opts`. Asked as one list, because it is one
/// choice that changes several fields at once.
const SECURITY: Field = {
  key: "security",
  label: tk("Security"),
  kind: "select",
  options: ["none", "tls", "reality"],
  ui: true,
  hint: tk("reality is the same TLS, but with someone else's certificate and the server's key"),
};

const secured = (values: Values) => values.security === "tls" || values.security === "reality";
const isReality = (values: Values) => values.security === "reality";

/// TLS for vless and vmess: shown by the "Security" choice. trojan and hysteria2 always
/// have it and there is nothing to choose — hence the same set, without the list.
const tls = (pick: boolean, sni = "servername"): Part => ({
  title: tk("Security"),
  fields: [
    ...(pick ? [SECURITY] : []),
    {
      key: sni,
      label: "SNI",
      kind: "text",
      hint: tk("the name in the handshake; empty — the address is used"),
      when: pick ? secured : undefined,
    },
    {
      key: "alpn",
      label: "ALPN",
      kind: "multi",
      options: ["h2", "http/1.1", "h3"],
      when: pick ? secured : undefined,
    },
    {
      key: "client-fingerprint",
      label: tk("TLS fingerprint"),
      kind: "select",
      options: FINGERPRINTS,
      hint: tk("which browser the handshake pretends to be"),
      when: pick ? secured : undefined,
    },
    {
      key: "fingerprint",
      label: tk("Certificate pin"),
      kind: "text",
      hint: tk("SHA256, if the server requires it"),
      when: pick ? secured : undefined,
    },
    {
      key: "skip-cert-verify",
      label: tk("Skip certificate check"),
      kind: "bool",
      // reality's certificate is someone else's by design and cannot be checked — the field would lie there.
      when: pick ? (values) => values.security === "tls" : undefined,
    },
  ],
});

const REALITY: Part = {
  title: "REALITY",
  fields: [
    {
      key: "reality-opts.public-key",
      label: tk("Public key"),
      kind: "text",
      need: true,
      when: isReality,
    },
    { key: "reality-opts.short-id", label: "Short ID", kind: "text", when: isReality },
    {
      key: "reality-opts.support-x25519mlkem768",
      label: tk("Post-quantum exchange"),
      kind: "bool",
      hint: tk("x25519mlkem768: enabled on recent servers"),
      when: isReality,
    },
  ],
};

/// Transport: the network and the fields of the **chosen** network. A ws path means
/// nothing under gRPC, and showing it alongside invites typing what the core will not read.
const net = (name: string) => (values: Values) => (values.network ?? "tcp") === name;

const transport = (networks: string[]): Part => ({
  title: tk("Transport"),
  fields: [
    { key: "network", label: tk("Network"), kind: "select", options: networks },
    { key: "ws-opts.path", label: tk("Path"), kind: "text", placeholder: "/", when: net("ws") },
    { key: "ws-opts.headers.Host", label: "Host", kind: "text", when: net("ws") },
    {
      key: "ws-opts.max-early-data",
      label: tk("Early data, bytes"),
      kind: "number",
      when: net("ws"),
    },
    {
      key: "grpc-opts.grpc-service-name",
      label: tk("Service name"),
      kind: "text",
      when: net("grpc"),
    },
    { key: "h2-opts.path", label: tk("Path"), kind: "text", when: net("h2") },
    { key: "h2-opts.host", label: "Host", kind: "multi", options: [], when: net("h2") },
    { key: "http-opts.method", label: tk("Method"), kind: "text", when: net("http") },
    { key: "http-opts.path", label: tk("Path"), kind: "multi", options: [], when: net("http") },
    {
      key: "xhttp-opts.mode",
      label: tk("Mode"),
      kind: "select",
      options: ["auto", "packet-up", "stream-up", "stream-one"],
      when: net("xhttp"),
    },
    { key: "xhttp-opts.path", label: tk("Path"), kind: "text", when: net("xhttp") },
    { key: "xhttp-opts.host", label: "Host", kind: "text", when: net("xhttp") },
    {
      key: "xhttp-opts.x-padding-bytes",
      label: tk("Padding bytes"),
      kind: "text",
      placeholder: "100-1000",
      when: net("xhttp"),
    },
  ],
});

/// What the core understands on **any** node: in its sources this is `BasicOption`, embedded
/// in every outbound. So one part serves all protocols instead of eight identical copies.
/// `routing-mark` is left out: it is a Linux packet mark and marks nothing on Windows.
const CONNECTION: Part = {
  title: tk("Connection"),
  fields: [
    {
      key: "ip-version",
      label: tk("IP version"),
      kind: "select",
      options: ["dual", "ipv4", "ipv6", "ipv4-prefer", "ipv6-prefer"],
      hint: tk("which server address to use when the name has several"),
    },
    {
      key: "dialer-proxy",
      label: tk("Via node"),
      kind: "text",
      hint: tk("another node's name: this one goes through it — a chain"),
    },
    {
      key: "interface-name",
      label: tk("Network adapter"),
      kind: "text",
      hint: tk("go out exactly through it, not through the one the system picks"),
    },
    { key: "tfo", label: "TCP Fast Open", kind: "bool" },
    { key: "mptcp", label: "Multipath TCP", kind: "bool" },
  ],
};

/// Multiplexing (D-121). In the core it is **not a protocol property**: the wrapper goes on
/// top of any outbound after its fields are parsed (`adapter/parser.go`). But we offer it
/// only where someone implements it on the far side — over TCP; a stream multiplexer over
/// hysteria2 or wireguard would be an invitation to type junk.
const SMUX: Part = {
  title: tk("Multiplexing"),
  fields: [
    {
      key: "smux.enabled",
      label: tk("Enable smux"),
      kind: "bool",
      hint: tk(
        "all streams in one connection: saves handshakes, but a big download chokes the rest",
      ),
    },
    {
      key: "smux.protocol",
      label: tk("Kind"),
      kind: "select",
      options: ["smux", "yamux", "h2mux"],
      under: "smux.enabled",
    },
    {
      key: "smux.max-connections",
      label: tk("Connections, at most"),
      kind: "number",
      under: "smux.enabled",
    },
    {
      key: "smux.min-streams",
      label: tk("Streams before a new one"),
      kind: "number",
      under: "smux.enabled",
    },
    {
      key: "smux.max-streams",
      label: tk("Streams per connection"),
      kind: "number",
      under: "smux.enabled",
    },
    {
      key: "smux.padding",
      label: tk("Padding"),
      kind: "bool",
      hint: tk("hides frame sizes; the server must support it too"),
      under: "smux.enabled",
    },
    { key: "smux.only-tcp", label: tk("TCP only"), kind: "bool", under: "smux.enabled" },
    { key: "smux.statistic", label: tk("Count connections"), kind: "bool", under: "smux.enabled" },
    {
      key: "smux.brutal-opts.enabled",
      label: "TCP Brutal",
      kind: "bool",
      hint: tk("its own rate control; needs a server with the same support"),
      under: "smux.enabled",
    },
    {
      key: "smux.brutal-opts.up",
      label: tk("Brutal: upload"),
      kind: "text",
      placeholder: "50 Mbps",
      under: "smux.brutal-opts.enabled",
    },
    {
      key: "smux.brutal-opts.down",
      label: tk("Brutal: download"),
      kind: "text",
      placeholder: "200 Mbps",
      under: "smux.brutal-opts.enabled",
    },
  ],
};

const CIPHERS = [
  "aes-128-gcm",
  "aes-192-gcm",
  "aes-256-gcm",
  "chacha20-ietf-poly1305",
  "xchacha20-ietf-poly1305",
  "2022-blake3-aes-128-gcm",
  "2022-blake3-aes-256-gcm",
  "2022-blake3-chacha20-poly1305",
  "none",
];

const KINDS: Protocol[] = [
  {
    id: "vless",
    label: "VLESS",
    parts: [
      basics(UDP),
      {
        title: "VLESS",
        fields: [
          { key: "uuid", label: "UUID", kind: "secret", need: true },
          {
            key: "flow",
            label: "Flow",
            kind: "select",
            options: ["xtls-rprx-vision"],
            when: secured,
          },
          { key: "encryption", label: tk("Encryption"), kind: "text", placeholder: "none" },
          {
            key: "packet-encoding",
            label: tk("UDP packing"),
            kind: "select",
            options: ["packetaddr", "xudp"],
          },
        ],
      },
      tls(true),
      REALITY,
      transport(["tcp", "ws", "grpc", "h2", "http", "xhttp"]),
      SMUX,
    ],
  },
  {
    id: "vmess",
    label: "VMess",
    parts: [
      basics(UDP),
      {
        title: "VMess",
        fields: [
          { key: "uuid", label: "UUID", kind: "secret", need: true },
          {
            key: "alterId",
            label: "alterId",
            kind: "number",
            hint: tk("0 for anything newer than 2021"),
          },
          {
            key: "cipher",
            label: tk("Cipher"),
            kind: "select",
            options: ["auto", "none", "zero", "aes-128-gcm", "chacha20-poly1305"],
          },
          {
            key: "packet-encoding",
            label: tk("UDP packing"),
            kind: "select",
            options: ["packetaddr", "xudp"],
          },
          { key: "global-padding", label: tk("Global padding"), kind: "bool" },
          { key: "authenticated-length", label: tk("Authenticated length"), kind: "bool" },
        ],
      },
      tls(true),
      transport(["tcp", "ws", "grpc", "h2", "http"]),
      SMUX,
    ],
  },
  {
    id: "trojan",
    label: "Trojan",
    parts: [
      basics(UDP),
      {
        title: "Trojan",
        fields: [{ key: "password", label: tk("Password"), kind: "secret", need: true }],
      },
      // trojan always has TLS: there is no separate toggle, and SNI is called `sni`.
      tls(false, "sni"),
      REALITY,
      transport(["tcp", "ws", "grpc"]),
      SMUX,
    ],
  },
  {
    id: "hysteria2",
    label: "Hysteria2",
    parts: [
      basics(),
      {
        title: "Hysteria2",
        fields: [
          { key: "password", label: tk("Password"), kind: "secret", need: true },
          {
            key: "ports",
            label: tk("Port range"),
            kind: "text",
            placeholder: "443-8443",
            hint: tk("port hopping; the main port is still required"),
          },
          { key: "hop-interval", label: tk("Hop interval, s"), kind: "number" },
          { key: "up", label: tk("Upload"), kind: "text", placeholder: "50 Mbps" },
          { key: "down", label: tk("Download"), kind: "text", placeholder: "200 Mbps" },
          {
            key: "obfs",
            label: tk("Obfuscation"),
            kind: "select",
            options: ["salamander", "gecko"],
          },
          {
            key: "obfs-password",
            label: tk("Obfuscation password"),
            kind: "secret",
            need: true,
            when: (values) => Boolean(values.obfs),
          },
          {
            key: "obfs-min-packet-size",
            label: tk("Junk, min"),
            kind: "number",
            when: (values) => Boolean(values.obfs),
          },
          {
            key: "obfs-max-packet-size",
            label: tk("Junk, max"),
            kind: "number",
            when: (values) => Boolean(values.obfs),
          },
          { key: "udp-mtu", label: "MTU", kind: "number" },
          {
            key: "cwnd",
            label: tk("Congestion window"),
            kind: "number",
            hint: tk("packets in flight; touch only if the link keeps breaking"),
          },
        ],
      },
      tls(false, "sni"),
    ],
  },
  {
    id: "wireguard",
    label: "WireGuard",
    parts: [
      basics(),
      {
        title: tk("Keys"),
        fields: [
          { key: "private-key", label: tk("Own key"), kind: "secret", need: true },
          { key: "public-key", label: tk("Server key"), kind: "secret", need: true },
          { key: "pre-shared-key", label: tk("Pre-shared key (PSK)"), kind: "secret" },
        ],
      },
      {
        title: tk("Tunnel"),
        fields: [
          {
            key: "ip",
            label: tk("Tunnel address"),
            kind: "text",
            need: true,
            placeholder: "10.0.0.2",
          },
          { key: "ipv6", label: tk("IPv6 address"), kind: "text" },
          { key: "mtu", label: "MTU", kind: "number", placeholder: "1420" },
          {
            key: "persistent-keepalive",
            label: tk("Keepalive, s"),
            kind: "number",
            placeholder: "25",
          },
          { key: "remote-dns-resolve", label: tk("Resolve names on the far side"), kind: "bool" },
          {
            key: "dns",
            label: tk("DNS in the tunnel"),
            kind: "multi",
            options: ["1.1.1.1", "8.8.8.8"],
            hint: tk("who answers queries on the far side"),
          },
          {
            key: "reserved",
            label: "Reserved",
            kind: "numbers",
            placeholder: "0, 0, 0",
            hint: tk("exactly three numbers; servers like Cloudflare WARP require them"),
          },
        ],
      },
    ],
  },
  {
    id: "ss",
    label: "Shadowsocks",
    parts: [
      basics(UDP),
      {
        title: "Shadowsocks",
        fields: [
          { key: "cipher", label: tk("Cipher"), kind: "select", options: CIPHERS, need: true },
          { key: "password", label: tk("Password"), kind: "secret", need: true },
          { key: "udp-over-tcp", label: tk("UDP over TCP"), kind: "bool" },
          {
            key: "udp-over-tcp-version",
            label: tk("UoT version"),
            kind: "number",
            placeholder: "2",
            under: "udp-over-tcp",
          },
        ],
      },
      {
        title: tk("Plugin"),
        fields: [
          {
            key: "plugin",
            label: tk("Plugin"),
            kind: "select",
            options: ["obfs", "v2ray-plugin", "shadow-tls", "restls"],
          },
          {
            key: "plugin-opts.mode",
            label: tk("Mode"),
            kind: "text",
            placeholder: tk("tls or http"),
            when: (values) => Boolean(values.plugin),
          },
          {
            key: "plugin-opts.host",
            label: "Host",
            kind: "text",
            when: (values) => Boolean(values.plugin),
          },
          {
            key: "plugin-opts.password",
            label: tk("Plugin password"),
            kind: "secret",
            when: (values) => values.plugin === "shadow-tls" || values.plugin === "restls",
          },
        ],
      },
      SMUX,
    ],
  },
  {
    id: "socks5",
    label: "SOCKS5",
    parts: [
      basics(UDP),
      {
        title: tk("Access"),
        fields: [
          { key: "username", label: tk("Username"), kind: "text" },
          { key: "password", label: tk("Password"), kind: "secret" },
        ],
      },
      {
        title: "TLS",
        fields: [
          { key: "tls", label: tk("Enable TLS"), kind: "bool" },
          {
            key: "fingerprint",
            label: tk("Certificate pin"),
            kind: "text",
            under: "tls",
          },
          {
            key: "skip-cert-verify",
            label: tk("Skip certificate check"),
            kind: "bool",
            under: "tls",
          },
        ],
      },
    ],
  },
  {
    id: "http",
    label: "HTTP",
    parts: [
      basics(),
      {
        title: tk("Access"),
        fields: [
          { key: "username", label: tk("Username"), kind: "text" },
          { key: "password", label: tk("Password"), kind: "secret" },
        ],
      },
      {
        title: "TLS",
        fields: [
          { key: "tls", label: tk("Enable TLS"), kind: "bool" },
          { key: "sni", label: "SNI", kind: "text", under: "tls" },
          {
            key: "fingerprint",
            label: tk("Certificate pin"),
            kind: "text",
            under: "tls",
          },
          {
            key: "skip-cert-verify",
            label: tk("Skip certificate check"),
            kind: "bool",
            under: "tls",
          },
        ],
      },
    ],
  },
];

/// The core reads `BasicOption` on every node, so the "Connection" part is appended to all
/// of them at once — eight identical copies in the list above would drift on the first edit.
export const PROTOCOLS: Protocol[] = KINDS.map((kind) => ({
  ...kind,
  parts: [...kind.parts, CONNECTION],
}));

export const protocol = (id: string) => PROTOCOLS.find((item) => item.id === id) ?? PROTOCOLS[0];

/// All protocol fields in a row — the window and assembly need the same list.
export const fieldsOf = (id: string) => protocol(id).parts.flatMap((part) => part.fields);

/// A field value as the core writes it: a number as a number, a checkbox as a boolean,
/// a list as a list. Empty is not written at all — an unfilled field means "not set",
/// not "empty".
function written(field: Field, raw: string): unknown | undefined {
  const text = raw.trim();
  if (text === "") return undefined;
  if (field.kind === "bool") return text === "true";
  if (field.kind === "number") {
    const number = Number(text);
    return Number.isFinite(number) ? number : undefined;
  }
  if (field.kind === "numbers") {
    const list = text
      .split(",")
      .map((part) => Number(part.trim()))
      .filter((part) => Number.isFinite(part));
    return list.length > 0 ? list : undefined;
  }
  if (field.kind === "multi") {
    const list = text
      .split(",")
      .map((part) => part.trim())
      .filter(Boolean);
    return list.length > 0 ? list : undefined;
  }
  return text;
}

/// Put a value at a dotted path: `ws-opts.headers.Host`.
function put(entry: Record<string, unknown>, path: string, value: unknown) {
  const steps = path.split(".");
  let here = entry;
  for (const step of steps.slice(0, -1)) {
    if (typeof here[step] !== "object" || here[step] === null) here[step] = {};
    here = here[step] as Record<string, unknown>;
  }
  here[steps[steps.length - 1]] = value;
}

/// The fields visible now: one hidden by its condition is not shown and **not written**.
/// Typed a path for ws, switched to grpc — it stays out of the config.
///
/// A field under a checkbox (`under`) is visible while the checkbox is on **and visible
/// itself**: turn off smux and Brutal goes too, with its rates, even though Brutal's
/// checkbox stays in the values.
export const shown = (id: string, values: Values) => {
  const fields = fieldsOf(id);
  const byKey = new Map(fields.map((field) => [field.key, field]));
  const visible = (field: Field): boolean => {
    if (field.when !== undefined && !field.when(values)) return false;
    if (field.under === undefined) return true;
    const parent = byKey.get(field.under);
    return parent !== undefined && values[field.under] === "true" && visible(parent);
  };
  return fields.filter(visible);
};

/// A node entry as the core reads it.
export type Entry = Record<string, unknown>;

/// Fields assembly adds by itself — no point putting them in "the rest": they would come
/// back anyway and only inflate the count of what was not understood.
const SYNTHETIC: Record<string, string[]> = { wireguard: ["allowed-ips", "udp"] };

/// A dotted entry: `{ "ws-opts": { path: "/" } }` → `{ "ws-opts.path": "/" }`.
/// Only objects are unfolded: a list is a field value, not a branch.
function flatten(entry: Entry, prefix = "", out: Entry = {}): Entry {
  for (const [key, value] of Object.entries(entry)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (value !== null && typeof value === "object" && !Array.isArray(value)) {
      flatten(value as Entry, path, out);
    } else {
      out[path] = value;
    }
  }
  return out;
}

/// An entry value as the field shows it. `undefined` means "this value does not fit this
/// field" — then it goes to "the rest" and survives the edit intact.
function shownAs(field: Field, raw: unknown): string | undefined {
  if (field.kind === "bool") return raw === true ? "true" : undefined;
  if (field.kind === "multi" || field.kind === "numbers") {
    return Array.isArray(raw) ? raw.map(String).join(", ") : undefined;
  }
  if (raw === null || Array.isArray(raw) || typeof raw === "object") return undefined;
  return String(raw);
}

/**
 * Parse an entry back into form values (D-121) — the pair of `toEntry`.
 *
 * Everything the model does not know goes to `extra` and returns to the entry on assembly:
 * opening a node in the form and pressing "Save" must not quietly impoverish it. A known
 * field that is **not shown** under the current choice (a ws path under gRPC) goes there
 * too: it cannot be shown, and losing it is worse.
 */
export function fromEntry(entry: Entry): { kind: string; values: Values; extra: Entry } {
  const kind = String(entry.type ?? "");
  const fields = new Map(fieldsOf(kind).map((field) => [field.key, field]));
  const flat = flatten(entry);
  const values: Values = {};
  const extra: Entry = {};

  // "Security" is a window field (D-120): it is not in the entry, it is derived from `tls`
  // and the presence of `reality-opts`. And `tls` then stays out of "the rest" — otherwise
  // choosing "none" could not clear it.
  const picks = fields.has("security");
  if (picks) {
    values.security =
      entry["reality-opts"] !== undefined ? "reality" : flat.tls === true ? "tls" : "none";
  }

  const skip = new Set(["type", ...(SYNTHETIC[kind] ?? []), ...(picks ? ["tls"] : [])]);
  for (const [path, raw] of Object.entries(flat)) {
    if (skip.has(path)) continue;
    const field = fields.get(path);
    const text = field && shownAs(field, raw);
    if (text === undefined) extra[path] = raw;
    else values[path] = text;
  }

  // Second pass: a field hidden under the resulting values is not editable in the form —
  // so it must be kept as unknown, or assembly would drop it.
  const here = new Set(shown(kind, values).map((field) => field.key));
  for (const path of Object.keys(values)) {
    if (path === "security" || here.has(path)) continue;
    extra[path] = flat[path];
    delete values[path];
  }
  return { kind, values, extra };
}

/// Assemble a node entry. Empty fields are not written: the core reads `sni: ""` literally.
/// `extra` is what the form does not know: it goes first, fields are laid over it.
export function toEntry(id: string, values: Values, extra: Entry = {}): Entry {
  const entry: Entry = { name: values.name?.trim() || "Node", type: id };
  for (const [path, value] of Object.entries(extra)) put(entry, path, value);
  for (const field of shown(id, values)) {
    if (field.key === "name" || field.ui) continue;
    const value = written(field, values[field.key] ?? "");
    if (value !== undefined) put(entry, field.key, value);
  }
  // "Security" is a window field: for the core it is `tls` and the presence of `reality-opts` (D-120).
  if (values.security === "tls" || values.security === "reality") entry.tls = true;
  // Here the node is a whole exit to the outside, same as a link (D-063).
  if (id === "wireguard") {
    entry["allowed-ips"] = values.ipv6?.trim() ? ["0.0.0.0/0", "::/0"] : ["0.0.0.0/0"];
    entry.udp = true;
  }
  return entry;
}

/// What is missing for the node to come up at all. Counted over **visible** fields: a
/// hidden one cannot be required — it was not shown.
export function missing(id: string, values: Values): string[] {
  return shown(id, values)
    .filter((field) => field.need && !(values[field.key] ?? "").trim())
    .map((field) => field.label);
}
