import { describe, expect, it } from "vitest";
import { VoltStrategy } from "../api/volt";
import {
  actionOf,
  fakeSource,
  networkOf,
  stepsOf,
  withFakeSource,
  withPayloads,
  withSteps,
} from "./strategy";

describe("VOLT visual strategy edits", () => {
  const parsed = () =>
    VoltStrategy.parse({
      version: 1,
      custom: { preserve: true },
      profiles: [
        {
          name: "custom-match",
          match: {
            network: "tcp",
            payloads: ["tls"],
            any: [{ signatures: [{ offset: 0, hex: "1603" }] }],
            host_files: ["hosts.txt"],
          },
          transform: {
            action: "split",
            positions: ["1"],
            packet_limit: 2,
            byte_limit: 16384,
            custom_step: "keep",
          },
        },
      ],
    });

  it("preserves advanced matching and unknown values when adding steps", () => {
    const strategy = parsed();
    const profile = strategy.profiles[0];
    const next = withSteps(profile, [
      stepsOf(profile)[0],
      { action: "fake", fake: { kind: "tls", repeats: 1 } },
    ]);
    expect(next.transform).toBeUndefined();
    expect(next.match).toEqual(profile.match);
    expect(next.stages?.[0].custom_step).toBe("keep");
    expect(VoltStrategy.parse({ ...strategy, profiles: [next] }).custom).toEqual({
      preserve: true,
    });
  });

  it("clears only fields that conflict with the explicitly chosen action", () => {
    const profile = parsed().profiles[0];
    const pass = actionOf(
      {
        ...stepsOf(profile)[0],
        sequence_overlap: 30,
        fake: { kind: "tls", repeats: 3 },
        custom_step: "keep",
      },
      "pass",
      profile,
    );
    expect(pass).toEqual({
      action: "pass",
      packet_limit: 2,
      byte_limit: 16384,
      custom_step: "keep",
    });
  });

  it("selects a dictionary without changing payload details or repetitions", () => {
    const fake = { kind: "tls-auto", repeats: 4, ttl: 7, server_name: "old.test", custom: true };
    const next = withFakeSource(fake, "noise-compact");
    expect(next).toEqual({
      kind: "tls-auto",
      repeats: 4,
      ttl: 7,
      custom: true,
      server_name_source: "noise-compact",
    });
    expect(fakeSource(next)).toBe("noise-compact");
    const inline = withFakeSource(next, "inline");
    expect(inline.server_name_source).toBeUndefined();
    expect(inline.server_names).toEqual([]);
    expect(fakeSource(inline)).toBe("inline");
    expect(fakeSource({ kind: "tls", server_names: [] })).toBe("inline");
    expect(fakeSource({ kind: "tls", server_name_file: "" })).toBe("file");
  });

  it("keeps TLS AUTO valid when a profile also accepts HTTP", () => {
    const profile = parsed().profiles[0];
    profile.transform = {
      action: "fake",
      fake: { kind: "tls-auto", server_name_source: "noise-extended" },
    };
    const changed = withPayloads(profile, ["tls", "http"]);
    expect(changed.match.payloads).toEqual(["tls", "http"]);
    expect(changed.match.any).toEqual(profile.match.any);
    expect(changed.stages?.[0].payloads).toEqual(["tls"]);
    expect(changed.stages?.[0].fake).toEqual(profile.transform.fake);
  });

  it("keeps advanced match nodes while making a network change valid for UDP", () => {
    const profile = parsed().profiles[0];
    const next = networkOf(
      {
        ...profile,
        transform: {
          ...stepsOf(profile)[0],
          fake: { kind: "tls-auto", server_name_source: "noise-extended" },
        },
      },
      "udp",
    );
    expect(next.match.any).toEqual(profile.match.any);
    expect(next.match.payloads).toBeUndefined();
    expect(next.stages?.[0]).toMatchObject({
      action: "fake",
      fake: { kind: "quic" },
      payloads: [],
    });
    expect(next.stages?.[0].positions).toBeUndefined();
  });

  it("keeps overlap only on the final compatible split step", () => {
    const profile = parsed().profiles[0];
    const next = withSteps(profile, [
      { action: "split", positions: ["1"], sequence_overlap: 32 },
      { action: "disorder", positions: ["32"], sequence_overlap: 16 },
    ]);
    expect(next.stages?.[0].sequence_overlap).toBeUndefined();
    expect(next.stages?.[1].sequence_overlap).toBe(16);
  });
});
