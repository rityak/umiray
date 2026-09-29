import { z } from "zod";
import { type Node as AppNode, call } from "../api";

export const Node = z.object({
  id: z.number(),
  name: z.string(),
  role: z.string(),
  reachable: z.boolean(),
  selected: z.boolean(),
  latencyMs: z.number().optional(),
});
export type Node = z.infer<typeof Node>;

export const asNode = (node: Node): AppNode => ({
  name: node.name,
  kind: "qd",
  source: "qd",
  supported: true,
  delay: node.latencyMs ?? null,
  method: node.latencyMs === undefined ? null : "proxy",
  fallback: false,
  address: null,
  country: null,
  edited: false,
});

export const State = z.object({
  imported: z.boolean(),
  connected: z.boolean(),
  node: Node.nullable(),
  nodes: z.object({ total: z.number(), reachable: z.number() }),
  egress: z.boolean(),
  adblock: z.boolean(),
  allowExit: z.boolean(),
  subscription: z.object({
    lastRefresh: z.number(),
    intervalMinutes: z.number(),
    expiresAt: z.number(),
  }),
  failed: z.boolean().optional(),
});
export type State = z.infer<typeof State>;

export const Status = z.object({
  present: z.boolean(),
  elevated: z.boolean(),
  running: z.boolean(),
  state: State.nullable(),
  problem: z.string().nullable(),
});
export type Status = z.infer<typeof Status>;

export const About = z.object({
  tag: z.string(),
  createdAt: z.number(),
  up: z.number(),
  down: z.number(),
  expiresAt: z.number(),
});
export type About = z.infer<typeof About>;

export const Sample = z.object({ t: z.number(), up: z.number(), down: z.number() });
export const History = z.object({ window: z.number(), points: z.array(Sample) });
export type History = z.infer<typeof History>;

export const Settings = z.object({
  refreshMinutes: z.number(),
  refreshPinned: z.boolean(),
  fixedRate: z.number(),
  ratePinned: z.boolean(),
});
export type Settings = z.infer<typeof Settings>;

export const ROLES = ["direct", "tunnel", "egress", "noEgress"] as const;
export type Role = (typeof ROLES)[number];
const Role = z.enum(ROLES).catch("tunnel");

export const Rule = z.object({
  id: z.number(),
  process: z.string(),
  path: z.string().optional(),
  role: Role,
  matched: z.number().optional(),
  running: z.boolean().optional(),
  icon: z.string().optional(),
});
export type Rule = z.infer<typeof Rule>;

export const Routing = z.object({
  defaultRole: Role,
  allowExit: z.boolean().catch(false),
  rules: z
    .array(Rule)
    .nullable()
    .transform((rules) => rules ?? []),
});
export type Routing = z.infer<typeof Routing>;

export const Process = z.object({
  name: z.string(),
  path: z.string().optional(),
  icon: z.string().optional(),
  connections: z.number().optional(),
});
export type Process = z.infer<typeof Process>;

const ask = <T extends z.ZodType>(
  schema: T,
  method: "GET" | "POST",
  path: string,
  body?: unknown,
) => call(schema, "qd_call", { method, path, body: body ?? null });

export const status = () => call(Status, "qd_status");

export const nodes = () => ask(z.array(Node), "GET", "/client/api/nodes");
export const toggle = (patch: { egress?: boolean; adblock?: boolean }) =>
  ask(State, "POST", "/client/api/toggle", patch);
export const importLink = (uri: string) => ask(State, "POST", "/client/api/import", { uri });
export const unlink = () => ask(State, "POST", "/client/api/reset", { subscription: true });
export const refresh = () =>
  ask(z.object({ nodes: z.number() }), "POST", "/client/api/subscription/refresh");
export const about = () => ask(About, "GET", "/client/api/about");
export const history = (window: 1 | 5 | 15 | 60) =>
  ask(History, "GET", `/client/api/history/${window}`);
export const settings = () => ask(Settings, "GET", "/client/api/settings");
export const saveSettings = (patch: Partial<Pick<Settings, "refreshMinutes" | "fixedRate">>) =>
  ask(Settings, "POST", "/client/api/settings", patch);
export const routing = () => ask(Routing, "GET", "/client/api/routing");
export const saveRouting = (defaultRole: Role, rules: Rule[]) =>
  ask(Routing, "POST", "/client/api/routing", {
    defaultRole,
    rules: rules.map(({ process, path, role }) => ({ process, path, role })),
  });
export const processes = () =>
  ask(
    z
      .array(Process)
      .nullable()
      .transform((list) => list ?? []),
    "GET",
    "/client/api/routing/processes",
  );
export const exportRules = () => call(z.string().nullable(), "qd_rules_export");
export const importRules = () =>
  call(z.object({ rules: z.number() }).nullable(), "qd_rules_import");
