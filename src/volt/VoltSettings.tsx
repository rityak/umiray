import { useEffect, useMemo, useState } from "react";
import { Button, Callout, CheckboxCards, ChoiceCards, Field, Switch, Textarea } from "rootik";
import type { VoltOptions, VoltStrategy } from "../api";
import Editor from "../config/Editor";
import type { Part } from "../config/Page";
import { locale, t } from "../i18n";
import AutoTune from "./AutoTune";
import SimpleControls from "./SimpleControls";
import SiteCheck from "./SiteCheck";
import type { VoltController } from "./useVolt";
import { activity } from "./VoltRow";

export function voltPart(
  volt: VoltController,
  disabled: boolean,
  onSaved: () => void,
  onDirty: (dirty: boolean) => void,
  onElevate: () => void,
): Part {
  return {
    id: "antidpi-volt",
    label: "VOLT",
    settings: [
      {
        id: "volt-form",
        control: (
          <VoltSettings
            volt={volt}
            disabled={disabled}
            onSaved={onSaved}
            onDirty={onDirty}
            onElevate={onElevate}
          />
        ),
      },
    ],
  };
}

const lines = (text: string) => text.split("\n");

/** What a service sends through the bypass (D-191): a few names, the rest on hover. */
function composition(rules: string[]) {
  const hosts = rules.flatMap((rule) => {
    const [kind, value] = rule.split(",");
    return kind === "DOMAIN-SUFFIX" || kind === "DOMAIN" ? [value] : [];
  });
  const shown = hosts.slice(0, 2).join(", ");
  const more = hosts.length > 2 ? ` +${hosts.length - 2}` : "";
  const voice = rules.some((rule) => rule.includes("NETWORK,UDP"))
    ? ` · ${t("voice over UDP")}`
    : "";
  return <span title={hosts.join("\n")}>{`${shown}${more}${voice}`}</span>;
}
const filled = (list: string[]) => list.map((line) => line.trim()).filter(Boolean);

/** Parts that parse or render the Relay strategy; Apply waits for all of them. */
type Pending = "select" | "knobs";

/**
 * VOLT in two halves (D-182, D-186, D-187): the bypass for direct connections — its method,
 * noise and Code (D-193) — and VOLT for proxy traffic, only its YAML (D-192).
 */
function VoltSettings({
  volt,
  disabled,
  onSaved,
  onDirty,
  onElevate,
}: {
  volt: VoltController;
  disabled: boolean;
  onSaved: () => void;
  onDirty: (dirty: boolean) => void;
  onElevate: () => void;
}) {
  const snapshot = volt.snapshot;
  const [draft, setDraft] = useState<VoltOptions | null>(snapshot?.options ?? null);
  const [dirty, setDirty] = useState(false);
  const [code, setCode] = useState(false);
  const [pending, setPending] = useState<Partial<Record<Pending, boolean>>>({});
  const [tuning, setTuning] = useState(false);
  const [relayStrategy, setRelayStrategy] = useState<VoltStrategy | null>(null);
  // Stable per part: the strategy editors re-parse when their callback changes.
  const pend = useMemo(() => {
    const make = (id: Pending) => (value: boolean) =>
      setPending((current) => (current[id] === value ? current : { ...current, [id]: value }));
    return { select: make("select"), knobs: make("knobs") };
  }, []);
  useEffect(() => {
    if (!dirty && snapshot) setDraft(snapshot.options);
  }, [snapshot, dirty]);
  useEffect(() => {
    onDirty(dirty);
    return () => onDirty(false);
  }, [dirty, onDirty]);
  if (!snapshot || !draft) return null;
  const change = (patch: Partial<VoltOptions>) => {
    setDraft((current) => (current ? { ...current, ...patch } : current));
    setDirty(true);
  };
  const open = () => setCode(true);
  const busy = Object.values(pending).some(Boolean);
  const locked = disabled || volt.busy || tuning;
  const english = !locale().startsWith("ru");
  const status = state(snapshot, draft);
  // Discord voice is UDP: with UDP off in the strategy it would stop silently (D-187).
  const voice =
    draft.scope === "services" &&
    snapshot.services.some(
      (service) =>
        draft.services.includes(service.id) &&
        service.rules.some((rule) => rule.includes("NETWORK,UDP")),
    );
  const relay = {
    yaml: draft.relayYaml,
    disabled: locked,
    pools: snapshot.domainPools,
    onChange: (relayYaml: string) => change({ relayYaml }),
  };
  return (
    <div className="flex min-w-0 flex-col gap-4">
      {status && (
        <Callout tone={status.tone}>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <span>{status.text}</span>
            {status.elevate && (
              <Button size="sm" variant="secondary" onClick={onElevate}>
                {t("Restart as admin")}
              </Button>
            )}
          </div>
        </Callout>
      )}
      <Switch
        label={t("Bypass")}
        description={t("Disguises direct connections from the provider's filter")}
        checked={draft.directEnabled}
        disabled={locked}
        onChange={(event) => change({ directEnabled: event.target.checked })}
      />
      {draft.directEnabled && (
        <>
          <Field
            label={t("What goes through the bypass")}
            hint={t(
              "The core gets the DIRECT-VOLT and DIRECT-AUTO exits: Routing can send rules to them.",
            )}
          >
            <ChoiceCards
              aria-label={t("What goes through the bypass")}
              value={draft.scope}
              disabled={locked}
              onChange={(scope) => change({ scope })}
              options={[
                {
                  value: "services",
                  label: t("Selected services"),
                  description: t(
                    "Only these and your domains, always bypassed — with any exit, proxy too.",
                  ),
                },
                {
                  value: "direct",
                  label: t("All DIRECT traffic"),
                  description: t(
                    "Everything through the DIRECT exit when it is chosen. Routing rules to DIRECT stay as they are.",
                  ),
                },
              ]}
            />
          </Field>
          {draft.scope === "services" ? (
            <>
              <Field label={t("Services")}>
                <CheckboxCards
                  aria-label={t("Services")}
                  minWidth={200}
                  value={draft.services}
                  disabled={locked}
                  onChange={(services) => change({ services })}
                  options={snapshot.services.map((service) => ({
                    value: service.id,
                    label: (english && service.title_en) || service.title,
                    description: composition(service.rules),
                  }))}
                />
              </Field>
              {voice && relayStrategy && !relayStrategy.udp?.enabled && (
                <Callout tone="warn">
                  {t(
                    "Discord voice goes over UDP, and UDP through Relay is off — the switch is below the method.",
                  )}
                </Callout>
              )}
              <Field label={t("Your domains")} hint={t("One per line, subdomains included")}>
                <Textarea
                  mono
                  rows={2}
                  value={draft.domains.join("\n")}
                  disabled={locked}
                  placeholder="rutracker.org"
                  onChange={(event) => change({ domains: lines(event.target.value) })}
                />
              </Field>
            </>
          ) : (
            <Field
              label={t("How")}
              hint={t("“Directly first” misses a site that connects and then slows down.")}
            >
              <ChoiceCards
                aria-label={t("How")}
                value={draft.mode}
                disabled={locked}
                onChange={(mode) => change({ mode })}
                options={[
                  {
                    value: "auto",
                    label: t("Directly first"),
                    description: t(
                      "Bypass only when the site does not answer directly. Exit DIRECT-AUTO.",
                    ),
                  },
                  {
                    value: "relay",
                    label: t("Always bypass"),
                    description: t("Every connection goes through the bypass. Exit DIRECT-VOLT."),
                  },
                ]}
              />
            </Field>
          )}
          <SimpleControls
            part="select"
            {...relay}
            template={snapshot.relayDefault}
            label={t("Method")}
            hint={t("How packets change. The check picks the one that works on your network.")}
            onPending={pend.select}
            onParsed={setRelayStrategy}
            onCode={open}
          />
          <SimpleControls
            part="knobs"
            {...relay}
            template={snapshot.relayDefault}
            onPending={pend.knobs}
            onCode={open}
          />
          <AutoTune
            options={draft}
            report={snapshot.tuning}
            targets={snapshot.probeTargets}
            disabled={locked || busy}
            dirty={dirty}
            code={code}
            onChange={change}
            onBusy={setTuning}
            onTune={volt.tune}
          />
          <SiteCheck
            disabled={locked || dirty}
            canAdd={(domain) => draft.scope === "services" && !draft.domains.includes(domain)}
            onAdd={(domain) => change({ domains: [...filled(draft.domains), domain] })}
          />
          <Switch
            label={t("Code")}
            description={t("Everything else in the strategy: profiles, AUTO timings, UDP limits")}
            checked={code}
            onChange={(event) => setCode(event.target.checked)}
          />
          {code && (
            <>
              <Field
                label={t("Relay strategy YAML")}
                hint={t(
                  "Listener addresses are managed by the client. This YAML controls traffic transformations.",
                )}
              >
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={locked}
                  onClick={() => change({ relayYaml: snapshot.relayDefault })}
                >
                  {t("Restore preset")}
                </Button>
                <div className="h-72 min-w-0 overflow-hidden rounded-md border border-[var(--rk-line)]">
                  <Editor
                    value={draft.relayYaml}
                    readOnly={locked}
                    onChange={(relayYaml) => change({ relayYaml })}
                  />
                </div>
              </Field>
              <Field
                label={t("Cloudflare bootstrap IPs")}
                hint={t(
                  "Only if issuing a WARP key fails: real addresses of api.cloudflareclient.com, one per line. Empty asks system DNS.",
                )}
              >
                <Textarea
                  mono
                  rows={2}
                  value={draft.bootstrapIps.join("\n")}
                  disabled={locked}
                  onChange={(event) => change({ bootstrapIps: lines(event.target.value) })}
                />
              </Field>
            </>
          )}
        </>
      )}
      <Switch
        label={t("VOLT for proxy traffic")}
        description={t("When the connection to the proxy itself does not get through")}
        checked={draft.vpnEnabled}
        disabled={locked}
        onChange={(event) => change({ vpnEnabled: event.target.checked })}
      />
      {draft.vpnEnabled && (
        <>
          <Field
            label={t("Proxy strategy YAML")}
            hint={t("How VOLT changes connections to the proxy servers")}
          >
            <Button
              size="sm"
              variant="ghost"
              className="self-start"
              disabled={locked || draft.vpnYaml === snapshot.vpnDefault}
              onClick={() => change({ vpnYaml: snapshot.vpnDefault })}
            >
              {t("Restore preset")}
            </Button>
            <div className="h-64 min-w-0 overflow-hidden rounded-md border border-[var(--rk-line)]">
              <Editor
                value={draft.vpnYaml}
                readOnly={locked}
                onChange={(vpnYaml) => change({ vpnYaml })}
              />
            </div>
          </Field>
          <Field
            label={t("Proxy endpoints")}
            hint={t(
              "One real IP:port per line. Empty resolves configured server addresses before connecting; maximum 64.",
            )}
          >
            <Textarea
              mono
              rows={3}
              value={draft.endpoints.join("\n")}
              disabled={locked}
              onChange={(event) => change({ endpoints: lines(event.target.value) })}
              placeholder="45.86.245.83:443"
            />
          </Field>
          <Switch
            label={t("Auto-TTL for decoys")}
            description={t(
              "Measure the hops to the proxy servers so decoys expire in the network, not on your server",
            )}
            checked={draft.autoTtl}
            disabled={locked}
            onChange={(event) => change({ autoTtl: event.target.checked })}
          />
          {snapshot.vpnRunning && snapshot.endpoints.length > 0 && (
            <details className="min-w-0 text-sm">
              <summary className="cursor-pointer">
                {t("Proxy servers captured: {n}", { n: snapshot.endpoints.length })}
              </summary>
              <div className="mt-2 flex flex-col gap-0.5 pl-3 font-mono text-xs text-[var(--rk-muted)]">
                {snapshot.endpoints.map((endpoint) => (
                  <span key={endpoint}>{endpoint}</span>
                ))}
              </div>
            </details>
          )}
        </>
      )}
      <div className="flex flex-wrap gap-2">
        <Button
          disabled={locked || !dirty || busy}
          onClick={async () => {
            const options = {
              ...draft,
              domains: filled(draft.domains),
              endpoints: filled(draft.endpoints),
              bootstrapIps: filled(draft.bootstrapIps),
              probeUrls: filled(draft.probeUrls),
            };
            if (await volt.save(options)) {
              setDirty(false);
              onSaved();
            }
          }}
        >
          {t("Apply VOLT settings")}
        </Button>
        <Button
          variant="ghost"
          disabled={locked || !dirty}
          onClick={() => {
            setDraft(snapshot.options);
            setDirty(false);
          }}
        >
          {t("Discard")}
        </Button>
      </div>
    </div>
  );
}

/** What to say first (COPY rule 7): a state that needs the person, then what is running. */
function state(
  snapshot: NonNullable<VoltController["snapshot"]>,
  draft: VoltOptions,
): { tone: "info" | "warn" | "danger" | "success"; text: string; elevate?: boolean } | null {
  const wanted = draft.directEnabled || draft.vpnEnabled;
  if (!snapshot.available)
    return { tone: "info", text: t("VOLT is not downloaded yet. Turning it on downloads it.") };
  if (wanted && !snapshot.elevated)
    return {
      tone: "warn",
      text: t("Restart the client as administrator to run VOLT."),
      elevate: true,
    };
  if (draft.directEnabled && snapshot.relayError)
    return {
      tone: "danger",
      text: t("Relay did not start: {why}", { why: snapshot.relayError }),
    };
  if (snapshot.relayRunning || snapshot.vpnRunning) {
    const stats = snapshot.relayStats;
    const done = activity(snapshot);
    const error = stats?.lastError ? t("last error: {why}", { why: stats.lastError }) : null;
    return {
      tone: stats && stats.failures > 0 ? "warn" : "success",
      text: [t("Running"), done, error].filter(Boolean).join(" · "),
    };
  }
  if (wanted) return { tone: "info", text: t("Starts with the connection") };
  return null;
}
