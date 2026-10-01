import { describe, expect, it } from "vitest";
import { fieldsOf, fromEntry, missing, PROTOCOLS, shown, toEntry, wrong } from "./proxy";

describe("a node assembled by hand", () => {
  it("a value the core would take but never connect with is called out", () => {
    const fine = { name: "N", server: "a.example", port: "443", uuid: "u" };
    expect(wrong("vless", fine)).toEqual([]);
    // The core accepts port 99999 and an address with spaces silently (live, 01.10.2026):
    // the node is there and never comes up.
    expect(wrong("vless", { ...fine, port: "99999" })).toEqual(["Port"]);
    expect(wrong("vless", { ...fine, port: "0" })).toEqual(["Port"]);
    expect(wrong("vless", { ...fine, port: "44.3" })).toEqual(["Port"]);
    expect(wrong("vless", { ...fine, server: "not a host" })).toEqual(["Address"]);
    // An empty field is `missing`'s business, not a wrong value.
    expect(wrong("vless", { ...fine, port: "" })).toEqual([]);
  });

  it("empty fields stay out of the entry", () => {
    const entry = toEntry("vless", { name: "Mine", server: "a.example", port: "443", uuid: "u" });
    expect(entry).toEqual({
      name: "Mine",
      type: "vless",
      server: "a.example",
      port: 443,
      uuid: "u",
    });
    expect(Object.keys(entry)).not.toContain("servername");
    expect(Object.keys(entry)).not.toContain("network");
  });

  it("numbers as numbers, checkboxes as booleans, lists as lists", () => {
    const entry = toEntry("vless", {
      name: "N",
      server: "a",
      port: "443",
      uuid: "u",
      udp: "true",
      security: "tls",
      alpn: "h2, http/1.1",
    });
    expect(entry.port).toBe(443);
    expect(entry.udp).toBe(true);
    expect(entry.tls).toBe(true);
    expect(entry.alpn).toEqual(["h2", "http/1.1"]);
    expect(Object.keys(entry)).not.toContain("security");
  });

  it("a dot in a field name is nesting, as in the core", () => {
    const entry = toEntry("vless", {
      name: "N",
      server: "a",
      port: "443",
      uuid: "u",
      security: "reality",
      network: "ws",
      "ws-opts.path": "/ray",
      "ws-opts.headers.Host": "a.example",
      "reality-opts.public-key": "pbk",
    });
    expect(entry["ws-opts"]).toEqual({ path: "/ray", headers: { Host: "a.example" } });
    expect(entry["reality-opts"]).toEqual({ "public-key": "pbk" });
  });

  /// This is why fields became conditional: typed one thing, changed your mind — it stays out.
  it("a hidden field is neither shown nor written", () => {
    const base = { name: "N", server: "a", port: "443", uuid: "u" };
    const keys = (values: Record<string, string>) =>
      shown("vless", values).map((field) => field.key);

    expect(keys({ ...base, security: "none" })).not.toContain("servername");
    expect(keys({ ...base, security: "tls" })).toContain("servername");
    expect(keys({ ...base, security: "tls" })).not.toContain("reality-opts.public-key");
    expect(keys({ ...base, security: "reality" })).toContain("reality-opts.public-key");
    // reality's certificate is someone else's by design — "skip check" would lie there.
    expect(keys({ ...base, security: "reality" })).not.toContain("skip-cert-verify");

    expect(keys({ ...base, network: "ws" })).toContain("ws-opts.path");
    expect(keys({ ...base, network: "grpc" })).not.toContain("ws-opts.path");
    expect(keys({ ...base, network: "xhttp" })).toContain("xhttp-opts.mode");

    const switched = toEntry("vless", { ...base, network: "grpc", "ws-opts.path": "/forgot" });
    expect(switched["ws-opts"]).toBeUndefined();
  });

  it("an option's parameter is visible and written only while its checkbox is on (D-131)", () => {
    const base = { name: "N", server: "a", port: "443", cipher: "aes-128-gcm", password: "p" };
    const keys = (values: Record<string, string>) => shown("ss", values).map((field) => field.key);

    expect(keys(base)).not.toContain("smux.protocol");
    expect(keys({ ...base, "smux.enabled": "true" })).toContain("smux.protocol");

    // Two levels: rates under Brutal, Brutal under smux. Clear the top one — both go.
    const brutal = {
      ...base,
      "smux.enabled": "true",
      "smux.brutal-opts.enabled": "true",
      "smux.brutal-opts.up": "50 Mbps",
    };
    expect(keys(brutal)).toContain("smux.brutal-opts.up");
    expect(keys({ ...brutal, "smux.enabled": "" })).not.toContain("smux.brutal-opts.up");
    expect(toEntry("ss", { ...brutal, "smux.enabled": "" }).smux).toBeUndefined();
  });

  it("what is shown becomes required: the obfuscation password only with obfuscation", () => {
    const base = { name: "N", server: "a", port: "443", password: "p" };
    expect(missing("hysteria2", base)).toEqual([]);
    expect(missing("hysteria2", { ...base, obfs: "salamander" })).toEqual(["Obfuscation password"]);
  });

  /// A WireGuard node is a whole exit to the outside, and the client writes that, not a person.
  it("wireguard writes its outward route by itself", () => {
    const v4 = toEntry("wireguard", {
      name: "W",
      server: "a",
      port: "51820",
      "private-key": "k",
      "public-key": "p",
      ip: "10.0.0.2",
    });
    expect(v4["allowed-ips"]).toEqual(["0.0.0.0/0"]);
    expect(v4.udp).toBe(true);
    const v6 = toEntry("wireguard", {
      name: "W",
      server: "a",
      port: "51820",
      "private-key": "k",
      "public-key": "p",
      ip: "10.0.0.2",
      ipv6: "fd00::2",
    });
    expect(v6["allowed-ips"]).toEqual(["0.0.0.0/0", "::/0"]);
  });

  it("required fields are named before sending, not by the core refusing", () => {
    expect(missing("vless", { name: "N" })).toEqual(["Address", "Port", "UUID"]);
    expect(missing("vless", { name: "N", server: "a", port: "443", uuid: "u" })).toEqual([]);
  });

  it("the node type lands in the entry and comes from the core's list", () => {
    for (const item of PROTOCOLS) {
      const entry = toEntry(item.id, { name: "N", server: "a", port: "1" });
      expect(entry.type).toBe(item.id);
    }
    expect(PROTOCOLS.map((item) => item.id)).toEqual([
      "vless",
      "vmess",
      "trojan",
      "hysteria2",
      "shadowquic",
      "tuic",
      "anytls",
      "trusttunnel",
      "snell",
      "sudoku",
      "ssh",
      "openvpn",
      "tailscale",
      "zerotier",
      "masque",
      "wireguard",
      "ss",
      "mieru",
      "ssr",
      "socks5",
      "http",
    ]);
  });

  /// The core reads `BasicOption` on every node — so the window offers it on every one.
  it("common connection fields exist on all eight protocols", () => {
    for (const item of PROTOCOLS) {
      const keys = fieldsOf(item.id).map((field) => field.key);
      for (const common of ["ip-version", "dialer-proxy", "interface-name", "tfo", "mptcp"]) {
        expect(keys, `${common} missing from ${item.id}`).toContain(common);
      }
    }
  });

  /// What the link converter writes for a protocol, the form reads back whole: a node from a
  /// subscription opens in the form and saves unchanged.
  it("a tuic entry opens and saves unchanged, v4 token included", () => {
    const entry = {
      name: "T",
      type: "tuic",
      server: "a.example",
      port: 443,
      uuid: "u",
      password: "p",
      "congestion-controller": "bbr",
      "udp-relay-mode": "native",
      sni: "b.example",
      alpn: ["h3"],
      "skip-cert-verify": true,
      "disable-sni": true,
    };
    const { kind, values, extra } = fromEntry(entry);
    expect(extra).toEqual({});
    expect(toEntry(kind, values, extra)).toEqual(entry);

    const v4 = { name: "T", type: "tuic", server: "a", port: 443, token: "t" };
    const back = fromEntry(v4);
    expect(back.extra).toEqual({ token: "t" });
    expect(toEntry(back.kind, back.values, back.extra)).toEqual(v4);
  });

  /// The core takes `port` or `port-range`, never both.
  it("mieru writes a port or a range, whichever is filled", () => {
    const base = {
      name: "M",
      server: "a",
      transport: "TCP",
      username: "u",
      password: "p",
    };
    expect(missing("mieru", base)).toEqual(["Port"]);
    expect(missing("mieru", { ...base, "port-range": "2090-2099" })).toEqual([]);
    const ranged = toEntry("mieru", { ...base, "port-range": "2090-2099" });
    expect(ranged["port-range"]).toBe("2090-2099");
    expect(ranged.port).toBeUndefined();

    const entry = {
      name: "M",
      type: "mieru",
      server: "a",
      "port-range": "2090-2099",
      udp: true,
      transport: "UDP",
      username: "u",
      password: "p",
      multiplexing: "MULTIPLEXING_LOW",
    };
    const { kind, values, extra } = fromEntry(entry);
    expect(extra).toEqual({});
    expect(toEntry(kind, values, extra)).toEqual(entry);
  });

  /// sudoku's pure downlink defaults to true in the core and must match the server: the form
  /// has to be able to write an explicit no.
  it("a yes-or-no field writes false, not nothing", () => {
    const base = { name: "S", server: "a", port: "443", key: "k" };
    expect(toEntry("sudoku", { ...base, "enable-pure-downlink": "false" })).toEqual({
      name: "S",
      type: "sudoku",
      server: "a",
      port: 443,
      key: "k",
      "enable-pure-downlink": false,
    });
    const entry = {
      name: "S",
      type: "sudoku",
      server: "a",
      port: 443,
      key: "k",
      "padding-min": 1,
      "enable-pure-downlink": false,
      httpmask: { mode: "stream", tls: true, host: "cdn.example" },
    };
    const { kind, values, extra } = fromEntry(entry);
    expect(extra).toEqual({});
    expect(toEntry(kind, values, extra)).toEqual(entry);
  });

  it("snell asks for the obfuscation password only where the mode has one", () => {
    const base = { name: "N", server: "a", port: "1", psk: "k" };
    expect(missing("snell", { ...base, "obfs-opts.mode": "http" })).toEqual([]);
    expect(missing("snell", { ...base, "obfs-opts.mode": "shadow-tls" })).toEqual([
      "Obfuscation password",
    ]);
    const entry = toEntry("snell", { ...base, version: "4", "obfs-opts.mode": "tls" });
    expect(entry).toEqual({
      name: "N",
      type: "snell",
      server: "a",
      port: 1,
      psk: "k",
      version: 4,
      "obfs-opts": { mode: "tls" },
    });
  });

  it("a pinned ssh key brings its algorithm, or the server picks another type", () => {
    const base = { name: "S", server: "a", port: "22", username: "u" };
    const pinned = toEntry("ssh", {
      ...base,
      "host-key": "ssh-ed25519 AAAA, ssh-rsa BBBB",
    });
    expect(pinned["host-key-algorithms"]).toEqual(["ssh-ed25519", "rsa-sha2-512", "rsa-sha2-256"]);
    const chosen = toEntry("ssh", {
      ...base,
      "host-key": "ssh-ed25519 AAAA",
      "host-key-algorithms": "ssh-ed25519",
    });
    expect(chosen["host-key-algorithms"]).toEqual(["ssh-ed25519"]);
    expect(toEntry("ssh", base)["host-key-algorithms"]).toBeUndefined();
  });

  /// An .ovpn file lands as an entry with PEM blocks; the form must open and save it whole.
  it("an openvpn entry keeps its certificates line by line", () => {
    const pem = "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----";
    const entry = {
      name: "O",
      type: "openvpn",
      server: "a.example",
      port: 1194,
      udp: true,
      proto: "udp",
      cipher: "AES-256-GCM",
      "data-ciphers": ["AES-256-GCM", "AES-128-GCM"],
      ca: pem,
      cert: pem,
      key: pem,
      "tls-auth": pem,
      "key-direction": "1",
    };
    const { kind, values, extra } = fromEntry(entry);
    expect(extra).toEqual({});
    expect(values.ca).toContain("\n");
    expect(toEntry(kind, values, extra)).toEqual(entry);
    expect(missing("openvpn", { name: "O", server: "a", port: "1" })).toEqual(["Server CA"]);
  });

  it("tailscale has no address, and each node gets its own state folder", () => {
    expect(missing("tailscale", { name: "T" })).toEqual([]);
    const entry = toEntry("tailscale", { name: "Дом / exit", "exit-node": "100.64.0.1" });
    expect(entry).toEqual({
      name: "Дом / exit",
      type: "tailscale",
      "exit-node": "100.64.0.1",
      "state-dir": "tailscale/Дом-exit",
    });
    const { kind, values, extra } = fromEntry(entry);
    expect(extra).toEqual({});
    expect(toEntry(kind, values, extra)).toEqual(entry);
  });

  it("zerotier needs only a network", () => {
    expect(missing("zerotier", { name: "Z" })).toEqual(["Network ID"]);
    const entry = { name: "Z", type: "zerotier", udp: true, network: "ca7d185721bcb634" };
    const { kind, values, extra } = fromEntry(entry);
    expect(extra).toEqual({});
    expect(toEntry(kind, values, extra)).toEqual(entry);
  });

  it("a masque entry from usque opens and saves unchanged", () => {
    const entry = {
      name: "W",
      type: "masque",
      server: "162.159.198.1",
      port: 443,
      udp: true,
      "private-key": "PRIV",
      "public-key": "MFkwEwYH",
      ip: "172.16.0.2/32",
      ipv6: "2606:4700:110:8::2/128",
    };
    const { kind, values, extra } = fromEntry(entry);
    expect(extra).toEqual({});
    expect(toEntry(kind, values, extra)).toEqual(entry);
    expect(missing("masque", { name: "W", server: "a", port: "443" })).toEqual([
      "Own key",
      "Server key",
    ]);
  });

  it("an ssr entry opens and saves unchanged", () => {
    const entry = {
      name: "R",
      type: "ssr",
      server: "a.example",
      port: 8388,
      udp: true,
      cipher: "aes-256-cfb",
      password: "p",
      protocol: "auth_aes128_sha1",
      "protocol-param": "1:key",
      obfs: "tls1.2_ticket_auth",
      "obfs-param": "cdn.example",
    };
    const { kind, values, extra } = fromEntry(entry);
    expect(extra).toEqual({});
    expect(toEntry(kind, values, extra)).toEqual(entry);
    expect(missing("ssr", { name: "R", server: "a", port: "1" })).toEqual([
      "Cipher",
      "Password",
      "Protocol",
      "Obfuscation",
    ]);
  });

  it("an anytls entry opens and saves unchanged", () => {
    const entry = {
      name: "A",
      type: "anytls",
      server: "a.example",
      port: 443,
      udp: true,
      password: "p",
      sni: "b.example",
      "client-fingerprint": "chrome",
      "skip-cert-verify": true,
      "idle-session-timeout": 30,
    };
    const { kind, values, extra } = fromEntry(entry);
    expect(extra).toEqual({ "idle-session-timeout": 30 });
    expect(toEntry(kind, values, extra)).toEqual(entry);
  });

  /// Over QUIC there is no browser to pretend to be: the core has no such field there.
  it("QUIC protocols do not offer a TLS fingerprint", () => {
    for (const id of ["hysteria2", "tuic"]) {
      expect(fieldsOf(id).map((field) => field.key)).not.toContain("client-fingerprint");
    }
    expect(fieldsOf("trojan").map((field) => field.key)).toContain("client-fingerprint");
  });

  /// wireguard `reserved` is exactly three **numbers**: the core will not take it as a string.
  it("a list of numbers is written as numbers", () => {
    const entry = toEntry("wireguard", {
      name: "W",
      server: "a",
      port: "51820",
      "private-key": "k",
      "public-key": "p",
      ip: "10.0.0.2",
      reserved: "8, 0, 133",
      tfo: "true",
    });
    expect(entry.reserved).toEqual([8, 0, 133]);
    expect(entry.tfo).toBe(true);
  });

  /// Parsing exists for this: opening a node in the form and saving must not lose what the
  /// form does not know (D-121).
  it("an entry parses back and assembles the same", () => {
    const entry = {
      name: "Node",
      type: "vless",
      server: "a.example",
      port: 443,
      uuid: "u",
      tls: true,
      servername: "b.example",
      alpn: ["h2", "http/1.1"],
      network: "ws",
      "ws-opts": { path: "/ray", headers: { Host: "b.example" } },
      "reality-opts": { "public-key": "pbk", "short-id": "sid" },
      smux: { enabled: true, protocol: "smux", "brutal-opts": { enabled: true, up: "50 Mbps" } },
      "ech-opts": { enable: true, config: "here" },
    };
    const { kind, values, extra } = fromEntry(entry);
    expect(kind).toBe("vless");
    expect(values.security).toBe("reality");
    expect(values["ws-opts.path"]).toBe("/ray");
    expect(values.alpn).toBe("h2, http/1.1");
    expect(values["smux.enabled"]).toBe("true");
    expect(values["smux.brutal-opts.up"]).toBe("50 Mbps");
    // A field unknown to the model is not shown in the form and **not lost**.
    expect(extra).toEqual({ "ech-opts.enable": true, "ech-opts.config": "here" });
    expect(toEntry(kind, values, extra)).toEqual(entry);
  });

  /// A field hidden by the current choice is not editable in the form — so it must be kept
  /// as unknown, or assembly would drop it.
  it("a field hidden by a choice survives the edit", () => {
    const entry = {
      name: "N",
      type: "vless",
      server: "a",
      port: 443,
      uuid: "u",
      network: "grpc",
      "grpc-opts": { "grpc-service-name": "gun" },
      "ws-opts": { path: "/ray" },
    };
    const { kind, values, extra } = fromEntry(entry);
    expect(values["ws-opts.path"]).toBeUndefined();
    expect(extra["ws-opts.path"]).toBe("/ray");
    expect(toEntry(kind, values, extra)).toEqual(entry);
  });

  /// `false` in the entry and a missing field are different things, and a checkbox cannot tell them apart.
  it("an explicit no does not turn into not set", () => {
    const { kind, values, extra } = fromEntry({
      name: "N",
      type: "socks5",
      server: "a",
      port: 1080,
      udp: false,
    });
    expect(values.udp).toBeUndefined();
    expect(extra.udp).toBe(false);
    expect(toEntry(kind, values, extra).udp).toBe(false);
    // And once the checkbox is on, the form's truth is laid over it.
    expect(toEntry(kind, { ...values, udp: "true" }, extra).udp).toBe(true);
  });

  /// Two fields with one key would mean one silently overwrites the other.
  it("field keys within a protocol do not repeat", () => {
    for (const item of PROTOCOLS) {
      const keys = fieldsOf(item.id).map((field) => field.key);
      expect(new Set(keys).size, `repeat in ${item.id}: ${keys}`).toBe(keys.length);
    }
  });
});
