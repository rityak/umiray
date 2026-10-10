import { z } from "zod";
import { call } from "./call";

export const VoltOptions = z.object({
  directEnabled: z.boolean(),
  scope: z.enum(["services", "direct"]).default("services"),
  services: z.array(z.string()).default([]),
  domains: z.array(z.string()).default([]),
  mode: z.enum(["relay", "auto"]),
  relayPort: z.number().int().min(1).max(65535),
  autoPort: z.number().int().min(1).max(65535),
  relayYaml: z.string(),
  vpnEnabled: z.boolean(),
  vpnYaml: z.string(),
  endpoints: z.array(z.string()),
  bootstrapIps: z.array(z.string()),
  autoSelect: z.boolean().default(false),
  probeUrls: z.array(z.string()).default([]),
  autoTtl: z.boolean().default(false),
});
export type VoltOptions = z.infer<typeof VoltOptions>;

export const VoltTuneReport = z.object({
  checkedAt: z.string(),
  scope: z.literal("direct-https"),
  urls: z.array(z.string()),
  selected: z.string().nullable(),
  unblocked: z.boolean().default(false),
  candidates: z.array(
    z.object({
      id: z.string(),
      label: z.string(),
      successes: z.number().int().min(0),
      total: z.number().int().min(0),
      latencyMs: z.number().min(0).nullable(),
      checks: z.array(
        z.object({
          url: z.string(),
          attempt: z.number().int().min(1),
          ok: z.boolean(),
          latencyMs: z.number().min(0),
          error: z.string().nullable(),
        }),
      ),
    }),
  ),
});
export type VoltTuneReport = z.infer<typeof VoltTuneReport>;

export const VoltSnapshot = z.object({
  options: VoltOptions,
  available: z.boolean(),
  elevated: z.boolean(),
  relayRunning: z.boolean(),
  vpnRunning: z.boolean(),
  endpoints: z.array(z.string()),
  log: z.array(z.string()),
  relayDefault: z.string(),
  vpnDefault: z.string(),
  domainPools: z
    .array(
      z.object({
        id: z.string(),
        label: z.string(),
        count: z.number().int().min(0),
        source: z.string(),
        revision: z.string(),
        file: z.string(),
      }),
    )
    .default([]),
  tuning: VoltTuneReport.nullable().default(null),
  relayError: z.string().nullable().default(null),
  probeTargets: z.array(z.string()).default([]),
  relayStats: z
    .object({
      connections: z.number().int().min(0),
      active: z.number().int().min(0),
      modified: z.number().int().min(0),
      faked: z.number().int().min(0),
      failures: z.number().int().min(0),
      lastError: z.string().nullable(),
      autoDirect: z.number().int().min(0),
      autoBypassed: z.number().int().min(0),
      rotations: z.number().int().min(0).default(0),
      detectedFailures: z.number().int().min(0).default(0),
    })
    .nullable()
    .default(null),
  services: z
    .array(
      z.object({
        id: z.string(),
        title: z.string(),
        title_en: z.string().optional(),
        rules: z.array(z.string()),
      }),
    )
    .default([]),
});
export type VoltSnapshot = z.infer<typeof VoltSnapshot>;
export const voltGet = () => call(VoltSnapshot, "volt_get");
export const voltSet = (options: VoltOptions) => call(VoltSnapshot, "volt_set", { options });
export const voltTune = () => call(VoltTuneReport, "volt_tune");

const VoltAttempt = z.object({
  ok: z.boolean(),
  latencyMs: z.number().int().min(0).nullable(),
  error: z.string().nullable(),
});
export const VoltSiteCheck = z.object({
  url: z.string(),
  domain: z.string().nullable(),
  direct: VoltAttempt,
  bypass: VoltAttempt,
});
export type VoltSiteCheck = z.infer<typeof VoltSiteCheck>;
export const voltCheckSite = (url: string) => call(VoltSiteCheck, "volt_check_site", { url });
export const voltStrategyPreset = (yaml: string, id: string) =>
  call(z.string(), "volt_strategy_preset", { yaml, id });
export const voltDictionaryPick = () => call(z.string().nullable(), "volt_dictionary_pick");

const VoltFake = z
  .object({
    kind: z.string(),
    server_name: z.string().optional(),
    server_name_source: z.string().optional(),
    server_names: z.array(z.string()).optional(),
    server_name_file: z.string().optional(),
    payload_file: z.string().optional(),
    hex: z.string().optional(),
    repeats: z.number().int().optional(),
    ttl: z.number().int().optional(),
  })
  .passthrough();

export const VoltTransform = z
  .object({
    action: z.enum(["pass", "split", "disorder", "fake"]),
    positions: z.array(z.string()).optional(),
    payloads: z.array(z.string()).optional(),
    packet_limit: z.number().int().optional(),
    byte_limit: z.number().int().optional(),
    sequence_overlap: z.number().int().optional(),
    fake: VoltFake.optional(),
  })
  .passthrough();
export type VoltTransform = z.infer<typeof VoltTransform>;

export const VoltProfile = z
  .object({
    name: z.string(),
    match: z
      .object({
        network: z.enum(["tcp", "udp"]),
        payloads: z.array(z.string()).optional(),
        hosts: z.array(z.string()).optional(),
        exclude_hosts: z.array(z.string()).optional(),
        ports: z.array(z.string()).optional(),
      })
      .passthrough(),
    transform: VoltTransform.optional(),
    stages: z.array(VoltTransform).max(8).optional(),
  })
  .passthrough();
export type VoltProfile = z.infer<typeof VoltProfile>;

export const VoltStrategy = z
  .object({
    version: z.number().int(),
    profiles: z.array(VoltProfile).max(64).default([]),
    udp: z
      .object({
        enabled: z.boolean().optional(),
        max_destinations: z.number().int().optional(),
        idle_timeout_seconds: z.number().int().optional(),
      })
      .passthrough()
      .optional(),
    auto: z
      .object({
        direct_timeout_ms: z.number().int().optional(),
        fallback_delay_ms: z.number().int().optional(),
        hello_timeout_ms: z.number().int().optional(),
        fallback_timeout_ms: z.number().int().optional(),
        route_cache: z
          .object({
            enabled: z.boolean().optional(),
            ttl_seconds: z.number().int().optional(),
            max_entries: z.number().int().optional(),
          })
          .passthrough()
          .optional(),
      })
      .passthrough()
      .optional(),
  })
  .passthrough();
export type VoltStrategy = z.infer<typeof VoltStrategy>;

export const voltStrategyParse = (yaml: string) =>
  call(VoltStrategy, "volt_strategy_parse", { yaml });
export const voltStrategyRender = (strategy: VoltStrategy) =>
  call(z.string(), "volt_strategy_render", { strategy });
