import type { VoltProfile, VoltTransform } from "../api/volt";

export const PAYLOADS = {
  tcp: ["tls", "http", "unknown"],
  udp: ["quic", "stun", "discord", "unknown"],
};
export const stepsOf = (profile: VoltProfile): VoltTransform[] =>
  profile.stages ?? (profile.transform ? [profile.transform] : []);

export function withSteps(profile: VoltProfile, stages: VoltTransform[]): VoltProfile {
  const lastSplit = stages.reduce(
    (last, stage, index) =>
      stage.action === "split" || stage.action === "disorder" ? index : last,
    -1,
  );
  const next = {
    ...profile,
    stages: stages.map((stage, index) => {
      if (!stage.sequence_overlap || (index === lastSplit && stage.fake?.kind !== "tls-auto"))
        return stage;
      const normalized = { ...stage };
      delete normalized.sequence_overlap;
      return normalized;
    }),
  };
  delete next.transform;
  return next;
}

export function defaultFake(profile: VoltProfile) {
  const kind =
    profile.match.network === "udp"
      ? profile.match.payloads?.includes("quic")
        ? "quic"
        : "zero"
      : profile.match.payloads?.length === 1 && profile.match.payloads[0] === "tls"
        ? "tls-auto"
        : profile.match.payloads?.includes("http")
          ? "http"
          : "tls";
  return {
    kind,
    repeats: 1,
    ...(kind === "zero" ? {} : { server_name_source: "noise-extended" }),
  };
}

export function actionOf(
  stage: VoltTransform,
  action: VoltTransform["action"],
  profile: VoltProfile,
): VoltTransform {
  const next = { ...stage, action };
  if (action === "split" || action === "disorder") {
    next.positions ??= profile.match.payloads?.includes("tls") ? ["1", "midsld"] : ["1", "32"];
  } else {
    delete next.positions;
    delete next.sequence_overlap;
  }
  if (action === "pass") delete next.fake;
  if (action === "fake") next.fake ??= defaultFake(profile);
  if (next.fake?.kind === "tls-auto") {
    delete next.sequence_overlap;
    if (profile.match.payloads?.length !== 1 || profile.match.payloads[0] !== "tls")
      next.payloads = ["tls"];
  }
  return next;
}

export function networkOf(profile: VoltProfile, network: "tcp" | "udp"): VoltProfile {
  const next = { ...profile, match: { ...profile.match, network } };
  const payloads = profile.match.payloads?.filter((value) => PAYLOADS[network].includes(value));
  if (payloads?.length) next.match.payloads = payloads;
  else delete next.match.payloads;
  return withSteps(
    next,
    stepsOf(profile)
      .map((stage) => {
        let step = { ...stage };
        if (stage.payloads)
          step.payloads = stage.payloads.filter((value) => PAYLOADS[network].includes(value));
        if (network === "udp" && (stage.action === "split" || stage.action === "disorder")) {
          step = actionOf(step, stage.fake ? "fake" : "pass", next);
        }
        if (step.fake) {
          const allowed =
            network === "tcp"
              ? ["tls-auto", "tls", "http", "zero", "custom"]
              : ["quic", "zero", "custom"];
          if (!allowed.includes(step.fake.kind))
            step.fake = { ...step.fake, kind: network === "tcp" ? "tls" : "quic" };
        }
        if (step.payloads)
          step.payloads = step.payloads.filter((value) => PAYLOADS[network].includes(value));
        return step;
      })
      .filter((stage, index, stages) => stage.action !== "pass" || index === stages.length - 1),
  );
}

export function fakeSource(fake: NonNullable<VoltTransform["fake"]>): string {
  if (fake.server_name_source !== undefined) return fake.server_name_source;
  if (fake.server_name_file !== undefined) return "file";
  if (fake.server_names !== undefined) return "inline";
  return "fixed";
}

export function withPayloads(profile: VoltProfile, payloads: string[]): VoltProfile {
  const next = { ...profile, match: { ...profile.match, payloads } };
  return withSteps(
    next,
    stepsOf(profile).map((stage) =>
      stage.fake?.kind === "tls-auto" && (payloads.length !== 1 || payloads[0] !== "tls")
        ? { ...stage, payloads: ["tls"] }
        : stage,
    ),
  );
}

export function withFakeSource(
  fake: NonNullable<VoltTransform["fake"]>,
  source: string,
): NonNullable<VoltTransform["fake"]> {
  const next = { ...fake };
  delete next.server_name;
  delete next.server_name_source;
  delete next.server_names;
  delete next.server_name_file;
  if (source === "fixed") next.server_name = fake.server_name ?? "www.google.com";
  else if (source === "inline") next.server_names = fake.server_names ?? [];
  else if (source === "file") next.server_name_file = fake.server_name_file ?? "domains.txt";
  else next.server_name_source = source;
  return next;
}

export function lines(text: string): string[] {
  return [
    ...new Set(
      text
        .split(/[\n,]/)
        .map((value) => value.trim())
        .filter(Boolean),
    ),
  ];
}
