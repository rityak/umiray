import { describe, expect, it } from "vitest";
import { fieldsOf, fromEntry, missing, PROTOCOLS, shown, toEntry } from "./proxy";

describe("a node assembled by hand", () => {
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
      "wireguard",
      "ss",
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
