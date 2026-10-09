// biome-ignore-all lint/style/noNonNullAssertion: fixtures have a known shape; a missing step fails the test anyway
import { describe, expect, it } from "vitest";
import mock from "../../collections/volt/strategy-mock.json";
import { VoltStrategy } from "../api/volt";
import { readSimple, setDictionary, setPackets, setRepeats } from "./simple";

const relay = () => VoltStrategy.parse(mock.relay);

describe("VOLT simple settings", () => {
  it("reads the actual default and reports emitted packets instead of repetitions", () => {
    expect(readSimple(relay())).toEqual({
      preset: "tls-auto",
      packets: 2,
      repeats: 11,
      noisePackets: 22,
      source: "noise-extended",
      file: null,
    });
    expect(readSimple(VoltStrategy.parse(mock.vpn)).preset).toBe("tls-split");
    expect(readSimple(VoltStrategy.parse(mock["vpn-tcp"]))).toMatchObject({
      preset: "tcp-split",
      packets: 8,
      noisePackets: 0,
    });
  });

  it.each([
    ["split", false, "tls-split"],
    ["disorder", false, "tls-disorder"],
    ["fake", true, "tls-fake"],
    ["split", true, "tls-fake-split"],
    ["disorder", true, "tls-auto"],
  ] as const)("recognizes the %s preset with noise %s", (action, noise, preset) => {
    const strategy = relay();
    const stage = strategy.profiles[0].stages![0];
    stage.action = action;
    if (!noise) delete stage.fake;
    if (action === "fake") delete stage.positions;
    expect(readSimple(strategy).preset).toBe(preset);
  });

  it("keeps every field outside the explicitly changed TLS parameter", () => {
    const strategy = relay();
    strategy.custom = { preserve: true };
    const profile = strategy.profiles[0];
    profile.match.all = [{ signatures: [{ hex: "1603" }] }];
    profile.match.hosts = ["private.test"];
    profile.stages![0].custom_step = "keep";
    const before = structuredClone(strategy);
    const changed = setPackets(setRepeats(strategy, 3), 5);
    expect(readSimple(changed)).toMatchObject({ packets: 5, repeats: 3, noisePackets: 6 });
    expect(strategy).toEqual(before);
    expect(changed.custom).toEqual(before.custom);
    expect(changed.auto).toEqual(before.auto);
    expect(changed.udp).toEqual(before.udp);
    expect(changed.profiles.slice(1)).toEqual(before.profiles.slice(1));
    expect(changed.profiles[0].match).toEqual(before.profiles[0].match);
    expect(changed.profiles[0].stages![0]).toEqual({
      ...before.profiles[0].stages![0],
      packet_limit: 5,
      fake: { ...before.profiles[0].stages![0].fake, repeats: 3 },
    });
  });

  it("keeps the transform representation and switches mutually exclusive dictionary fields", () => {
    const strategy = relay();
    const profile = strategy.profiles[0];
    profile.transform = profile.stages![0];
    delete profile.stages;
    profile.transform.fake!.server_name_source = undefined;
    profile.transform.fake!.server_names = ["one.test", "two.test"];
    const file = setDictionary(strategy, "file", " D:\\Noise\\domains.txt ");
    expect(file.profiles[0].stages).toBeUndefined();
    expect(readSimple(file)).toMatchObject({ source: "file", file: "D:\\Noise\\domains.txt" });
    expect(file.profiles[0].transform!.fake).toEqual({
      kind: "tls-auto",
      repeats: 11,
      server_name_file: "D:\\Noise\\domains.txt",
    });
    const builtin = setDictionary(file, "noise-compact");
    expect(builtin.profiles[0].transform!.fake).toEqual({
      kind: "tls-auto",
      repeats: 11,
      server_name_source: "noise-compact",
    });
    expect(strategy.profiles[0].transform!.fake!.server_names).toEqual(["one.test", "two.test"]);
  });

  it("uses TLS restrictions in nested all and any without changing non-TLS steps", () => {
    const strategy = relay();
    const profile = strategy.profiles[0];
    delete profile.match.payloads;
    delete profile.stages![0].payloads;
    profile.match.all = [
      { any: [{ payloads: ["tls"] }, { all: [{ payloads: ["tls"] }] }] },
      { not: { hosts: ["private.test"] } },
    ];
    expect(readSimple(strategy).preset).toBe("tls-auto");
    const changed = setPackets(strategy, 4);
    expect(changed.profiles[0].match).toEqual(profile.match);
    expect(changed.profiles.slice(1)).toEqual(strategy.profiles.slice(1));
    profile.match.all = [{ any: [{ payloads: ["tls"] }, { payloads: ["http"] }] }];
    expect(readSimple(strategy).preset).toBe("custom");
  });

  it("does not mistake mixed stages or unsafe custom parameters for presets", () => {
    const unsafe = [
      { sequence_overlap: 256 },
      { positions: ["1", "sni+1"] },
      { byte_limit: 65536 },
      { packet_limit: 64 },
      { payloads: ["tls", "http"] },
      { fake: { kind: "tls-auto", ttl: 3, repeats: 3 } },
      { fake: { kind: "tls-auto", payload_file: "payload.bin", repeats: 3 } },
      { fake: { kind: "custom", hex: "0000", repeats: 3 } },
    ];
    for (const patch of unsafe) {
      const strategy = relay();
      Object.assign(strategy.profiles[0].stages![0], patch);
      expect(readSimple(strategy).preset).toBe("custom");
      expect(() => setPackets(strategy, 2)).toThrow("Open Code");
    }
    const strategy = relay();
    strategy.profiles[0].stages!.push({ action: "split", positions: ["1"], byte_limit: 16384 });
    expect(readSimple(strategy).preset).toBe("custom");
    expect(readSimple({ ...relay(), profiles: [] }).preset).toBe("custom");
  });

  it("leaves unfiltered custom TCP and conflicting TLS profiles in Code", () => {
    const strategy = relay();
    const duplicate = structuredClone(strategy.profiles[0]);
    delete duplicate.stages![0].fake;
    strategy.profiles.push(duplicate);
    expect(readSimple(strategy).preset).toBe("custom");
    const broad = relay();
    broad.profiles.unshift({
      name: "raw-custom",
      match: { network: "tcp" },
      transform: { action: "split", positions: ["1"], packet_limit: 2, byte_limit: 16384 },
    });
    expect(readSimple(broad).preset).toBe("custom");
  });

  it("updates an explicitly TLS-filtered step without changing the HTTP sibling", () => {
    const strategy = relay();
    const profile = strategy.profiles[0];
    profile.match.payloads = ["tls", "http"];
    const http = { action: "split" as const, payloads: ["http"], positions: ["host+1"] };
    profile.stages!.push(http);
    expect(readSimple(strategy).preset).toBe("tls-auto");
    const changed = setRepeats(strategy, 4);
    expect(changed.profiles[0].stages![1]).toBe(http);
    expect(changed.profiles[0].match).toEqual(profile.match);
  });

  it("bounds controls and refuses to create fake configurations from a quiet preset", () => {
    const strategy = relay();
    for (const value of [0, 9, 2.5, Number.NaN])
      expect(() => setPackets(strategy, value)).toThrow();
    for (const value of [0, 17, 1.5, Number.NaN])
      expect(() => setRepeats(strategy, value)).toThrow();
    expect(() => setDictionary(strategy, "file", " ")).toThrow();
    expect(() => setDictionary(strategy, "inline")).toThrow();
    expect(() => setDictionary(strategy, "../pool")).toThrow();
    expect(() => setRepeats(VoltStrategy.parse(mock.vpn), 3)).toThrow("Open Code");
    const raw = VoltStrategy.parse(mock["vpn-tcp"]);
    expect(readSimple(setPackets(raw, 4))).toMatchObject({ preset: "tcp-split", packets: 4 });
  });
});
