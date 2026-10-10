import type { VoltProfile, VoltStrategy, VoltTransform } from "../api/volt";

export type SimplePreset =
  | "custom"
  | "tls-split"
  | "tls-disorder"
  | "tls-fake"
  | "tls-fake-split"
  | "tls-auto";

/// What the page shows of a strategy (D-193): the method, its packet window, and the noise
/// when it has one.
export type SimpleSettings = {
  preset: SimplePreset;
  packets: number | null;
  repeats: number | null;
  source: string | null;
  file: string | null;
};

type Fake = NonNullable<VoltTransform["fake"]>;
type Target = { profile: number; stage: number; value: VoltTransform };
const custom: SimpleSettings = {
  preset: "custom",
  packets: null,
  repeats: null,
  source: null,
  file: null,
};

const stepsOf = (profile: VoltProfile): VoltTransform[] =>
  profile.stages ?? (profile.transform ? [profile.transform] : []);

function fakeSource(fake: Fake): string {
  if (fake.server_name_source !== undefined) return fake.server_name_source;
  if (fake.server_name_file !== undefined) return "file";
  if (fake.server_names !== undefined) return "inline";
  return "fixed";
}

/// The four dictionary fields exclude each other in the core: one goes in, the rest go.
function withFakeSource(fake: Fake, source: string): Fake {
  const next = { ...fake };
  delete next.server_name;
  delete next.server_name_source;
  delete next.server_names;
  delete next.server_name_file;
  if (source === "file") next.server_name_file = fake.server_name_file ?? "domains.txt";
  else next.server_name_source = source;
  return next;
}

function onlyTls(value: unknown): boolean {
  return Array.isArray(value) && value.length > 0 && value.every((item) => item === "tls");
}

function tlsMatch(value: unknown): boolean {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const match = value as Record<string, unknown>;
  return (
    onlyTls(match.payloads) ||
    (Array.isArray(match.all) && match.all.some(tlsMatch)) ||
    (Array.isArray(match.any) && match.any.length > 0 && match.any.every(tlsMatch))
  );
}

function bounded(value: number, min: number, max: number): boolean {
  return Number.isInteger(value) && value >= min && value <= max;
}

function method(stage: VoltTransform): SimplePreset {
  if (
    !bounded(stage.packet_limit || 4, 1, 8) ||
    stage.byte_limit !== 16384 ||
    (stage.sequence_overlap ?? 0) !== 0
  )
    return "custom";
  if (stage.fake) {
    if (
      stage.fake.kind !== "tls-auto" ||
      (stage.fake.ttl ?? 0) !== 0 ||
      stage.fake.hex !== undefined ||
      stage.fake.payload_file !== undefined ||
      !bounded(stage.fake.repeats || 1, 1, 16)
    )
      return "custom";
    if (stage.action === "fake" && !stage.positions?.length) return "tls-fake";
  }
  if (JSON.stringify(stage.positions) !== JSON.stringify(["1", "midsld"])) return "custom";
  if (stage.action === "split") return stage.fake ? "tls-fake-split" : "tls-split";
  if (stage.action === "disorder") return stage.fake ? "tls-auto" : "tls-disorder";
  return "custom";
}

function inspect(strategy: VoltStrategy): { preset: SimplePreset; targets: Target[] } {
  const targets: Target[] = [];
  let preset: SimplePreset | null = null;
  for (const [profileIndex, profile] of strategy.profiles.entries()) {
    if (profile.match.network !== "tcp") continue;
    if (profile.transform && profile.stages) return { preset: "custom", targets: [] };
    const stages = stepsOf(profile);
    const matched = tlsMatch(profile.match);
    const relevant = stages
      .map((value, stage) => ({ profile: profileIndex, stage, value }))
      .filter(({ value }) => onlyTls(value.payloads) || (matched && !value.payloads?.length));
    if (!relevant.length) {
      if (
        profile.match.payloads?.includes("tls") ||
        (!profile.match.payloads?.length && stages.some((stage) => !stage.payloads?.length))
      )
        return { preset: "custom", targets: [] };
      continue;
    }
    if (
      relevant.length !== 1 ||
      stages.some((stage) => stage.payloads?.includes("tls") && !onlyTls(stage.payloads)) ||
      (!matched && stages.some((stage) => !stage.payloads?.length))
    )
      return { preset: "custom", targets: [] };
    const next = method(relevant[0].value);
    if (next === "custom" || (preset !== null && preset !== next))
      return { preset: "custom", targets: [] };
    preset = next;
    targets.push(relevant[0]);
  }
  return { preset: preset ?? "custom", targets };
}

function same<T>(values: T[]): T | null {
  return values.length && values.every((value) => value === values[0]) ? values[0] : null;
}

export function readSimple(strategy: VoltStrategy): SimpleSettings {
  const { preset, targets } = inspect(strategy);
  if (preset === "custom") return { ...custom };
  const fakes = targets.map(({ value }) => value.fake).filter((fake) => fake !== undefined);
  const repeats = same(fakes.map((fake) => fake.repeats || 1));
  return {
    preset,
    packets: same(targets.map(({ value }) => value.packet_limit || 4)),
    repeats,
    source: same(fakes.map(fakeSource)),
    file: same(fakes.map((fake) => fake.server_name_file ?? null)),
  };
}

function update(
  strategy: VoltStrategy,
  change: (stage: VoltTransform) => VoltTransform,
  noise = false,
): VoltStrategy {
  const { preset, targets } = inspect(strategy);
  if (preset === "custom" || (noise && targets.some(({ value }) => !value.fake)))
    throw new Error("Open Code to edit this custom strategy");
  return {
    ...strategy,
    profiles: strategy.profiles.map((profile, profileIndex) => {
      const target = targets.find((item) => item.profile === profileIndex);
      if (!target) return profile;
      if (profile.stages)
        return {
          ...profile,
          stages: profile.stages.map((stage, index) =>
            index === target.stage ? change(stage) : stage,
          ),
        };
      return { ...profile, transform: change(target.value) };
    }),
  };
}

export function setPackets(strategy: VoltStrategy, packets: number): VoltStrategy {
  if (!bounded(packets, 1, 8)) throw new Error("Packet window must be 1..8");
  return update(strategy, (stage) => ({ ...stage, packet_limit: packets }));
}

/// UDP through Relay belongs to the whole strategy, not to a template: any strategy has it.
export function setUdp(strategy: VoltStrategy, enabled: boolean): VoltStrategy {
  return { ...strategy, udp: { ...strategy.udp, enabled } };
}

export function setRepeats(strategy: VoltStrategy, repeats: number): VoltStrategy {
  if (!bounded(repeats, 1, 16)) throw new Error("Noise repetitions must be 1..16");
  // `update(…, true)` refuses targets without noise, so `fake` is there.
  return update(
    strategy,
    (stage) => (stage.fake ? { ...stage, fake: { ...stage.fake, repeats } } : stage),
    true,
  );
}

export function setDictionary(strategy: VoltStrategy, source: string, file?: string): VoltStrategy {
  if (
    source === "fixed" ||
    source === "inline" ||
    !/^[a-z][a-z0-9_-]{0,63}$/.test(source) ||
    (source === "file" && !file?.trim())
  )
    throw new Error("Choose a noise dictionary or a .txt file");
  return update(
    strategy,
    (stage) =>
      stage.fake
        ? {
            ...stage,
            fake: {
              ...withFakeSource(stage.fake, source),
              ...(source === "file" && file ? { server_name_file: file.trim() } : {}),
            },
          }
        : stage,
    true,
  );
}
