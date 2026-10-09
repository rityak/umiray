import { ArrowDown, ArrowUp, Plus, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  Button,
  Callout,
  CheckboxGroup,
  Field,
  IconButton,
  Input,
  NumberInput,
  Select,
  Slider,
  Switch,
  Textarea,
} from "rootik";
import { asAppError } from "../api/call";
import * as api from "../api/volt";
import { t } from "../i18n";
import {
  actionOf,
  defaultFake,
  fakeSource,
  lines,
  networkOf,
  PAYLOADS,
  stepsOf,
  withFakeSource,
  withPayloads,
  withSteps,
} from "./strategy";

type Pools = api.VoltSnapshot["domainPools"];

export function ListInput({
  value,
  onChange,
  disabled,
  rows = 2,
}: {
  value: string[];
  onChange: (value: string[]) => void;
  disabled: boolean;
  rows?: number;
}) {
  const [text, setText] = useState(value.join("\n"));
  const normalized = JSON.stringify(value);
  useEffect(() => {
    if (JSON.stringify(lines(text)) !== normalized) setText(value.join("\n"));
  }, [normalized, text, value]);
  return (
    <Textarea
      rows={rows}
      value={text}
      disabled={disabled}
      onChange={(event) => {
        setText(event.target.value);
        onChange(lines(event.target.value));
      }}
    />
  );
}

function NumberField({
  label,
  hint,
  value,
  min,
  max,
  unit,
  onChange,
  disabled,
}: {
  label: string;
  hint?: string;
  value: number;
  min: number;
  max: number;
  unit?: string;
  onChange: (value: number) => void;
  disabled: boolean;
}) {
  return (
    <Field label={label} hint={hint}>
      <NumberInput
        value={value}
        min={min}
        max={max}
        unit={unit}
        disabled={disabled}
        onChange={(next) => {
          if (next !== null) onChange(next);
        }}
      />
    </Field>
  );
}

function StageEditor({
  stage,
  profile,
  index,
  count,
  pools,
  disabled,
  onChange,
  onMove,
  onRemove,
}: {
  stage: api.VoltTransform;
  profile: api.VoltProfile;
  index: number;
  count: number;
  pools: Pools;
  disabled: boolean;
  onChange: (stage: api.VoltTransform) => void;
  onMove: (offset: number) => void;
  onRemove: () => void;
}) {
  const tcp = profile.match.network === "tcp";
  const patch = (next: Partial<api.VoltTransform>) => onChange({ ...stage, ...next });
  const fake = stage.fake;
  const [fileError, setFileError] = useState<string | null>(null);
  const source = fake ? fakeSource(fake) : "fixed";
  const pool = pools.find((item) => item.id === source);
  const sourceName = (id: string, label: string) =>
    id === "noise-extended"
      ? t("Extended noise dictionary")
      : id === "noise-compact"
        ? t("Compact noise dictionary")
        : label;
  const lastSplit =
    stepsOf(profile).reduce(
      (last, item, at) => (item.action === "split" || item.action === "disorder" ? at : last),
      -1,
    ) === index;
  const setFake = (next: NonNullable<api.VoltTransform["fake"]>) => {
    const updated = { ...stage, fake: next };
    if (next.kind === "tls-auto") {
      delete updated.sequence_overlap;
      if (profile.match.payloads?.length !== 1 || profile.match.payloads[0] !== "tls")
        updated.payloads = ["tls"];
    }
    onChange(updated);
  };
  return (
    <details open={index === 0} className="min-w-0 rounded-md border border-[var(--rk-line)] p-3">
      <summary className="cursor-pointer text-sm font-medium">
        {t("Step {n}", { n: index + 1 })} ·{" "}
        {stage.action === "pass"
          ? t("Pass unchanged")
          : stage.action === "split"
            ? t("Split into smaller packets")
            : stage.action === "disorder"
              ? t("Split and send fragments out of order")
              : t("Add decoy packets")}
      </summary>
      <div className="mt-3 flex min-w-0 flex-col gap-4">
        <div className="flex justify-end gap-1">
          <IconButton
            icon={<ArrowUp />}
            label={t("Move step earlier")}
            size="sm"
            variant="ghost"
            disabled={disabled || index === 0 || stage.action === "pass"}
            onClick={() => onMove(-1)}
          />
          <IconButton
            icon={<ArrowDown />}
            label={t("Move step later")}
            size="sm"
            variant="ghost"
            disabled={
              disabled || index === count - 1 || stepsOf(profile)[index + 1]?.action === "pass"
            }
            onClick={() => onMove(1)}
          />
          <IconButton
            icon={<Trash2 />}
            label={t("Remove step")}
            size="sm"
            variant="ghost"
            disabled={disabled || count === 1}
            onClick={onRemove}
          />
        </div>
        <Field label={t("What to do with traffic")}>
          <Select
            value={stage.action}
            disabled={disabled}
            options={[
              { value: "pass", label: t("Pass unchanged"), disabled: index !== count - 1 },
              { value: "split", label: t("Split into smaller packets"), disabled: !tcp },
              {
                value: "disorder",
                label: t("Split and send fragments out of order"),
                disabled: !tcp,
              },
              { value: "fake", label: t("Add decoy packets") },
            ]}
            onChange={(action) => onChange(actionOf(stage, action, profile))}
          />
        </Field>
        {(stage.action === "split" || stage.action === "disorder") && (
          <>
            <Field
              label={t("Where to split")}
              hint={t(
                "Choose a readable layout or enter up to seven byte positions. The real stream is restored by TCP.",
              )}
            >
              <Select
                value={
                  JSON.stringify(stage.positions) === JSON.stringify(["1", "midsld"])
                    ? "tls"
                    : JSON.stringify(stage.positions) === JSON.stringify(["1", "host+1"])
                      ? "http"
                      : JSON.stringify(stage.positions) === JSON.stringify(["1", "32", "64"])
                        ? "bytes"
                        : "custom"
                }
                disabled={disabled}
                options={[
                  { value: "tls", label: t("At the start and inside the TLS hostname") },
                  { value: "http", label: t("At the start and inside the HTTP Host header") },
                  { value: "bytes", label: t("At bytes 1, 32 and 64") },
                  { value: "custom", label: t("Custom split positions") },
                ]}
                onChange={(value) => {
                  if (value !== "custom")
                    patch({
                      positions:
                        value === "tls"
                          ? ["1", "midsld"]
                          : value === "http"
                            ? ["1", "host+1"]
                            : ["1", "32", "64"],
                    });
                }}
              />
            </Field>
            <Field
              label={t("Split positions")}
              hint={t(
                "Bytes or markers: 1, midsld, sni+1, host+1. Missing markers leave the packet unchanged.",
              )}
            >
              <ListInput
                value={stage.positions ?? []}
                disabled={disabled}
                onChange={(positions) => patch({ positions })}
              />
            </Field>
          </>
        )}
        {stage.action !== "pass" && (
          <>
            <div className="grid gap-4 sm:grid-cols-2">
              <NumberField
                label={t("Only the first packets")}
                hint={t("Stop changing this connection after this many packets per step.")}
                value={stage.packet_limit || 4}
                min={1}
                max={128}
                disabled={disabled}
                onChange={(packet_limit) => patch({ packet_limit })}
              />
              <NumberField
                label={t("Only the first bytes")}
                hint={t("Limit how much of the connection this step may change.")}
                value={stage.byte_limit || 65536}
                min={1}
                max={1048576}
                unit={t("bytes")}
                disabled={disabled}
                onChange={(byte_limit) => patch({ byte_limit })}
              />
            </div>
            {tcp &&
              lastSplit &&
              fake?.kind !== "tls-auto" &&
              (stage.action === "split" || stage.action === "disorder") && (
                <NumberField
                  label={t("Overlap adjacent fragments")}
                  hint={t("Repeat these bytes at the fragment boundary. Zero disables overlap.")}
                  value={stage.sequence_overlap ?? 0}
                  min={0}
                  max={1200}
                  unit={t("bytes")}
                  disabled={disabled}
                  onChange={(sequence_overlap) => patch({ sequence_overlap })}
                />
              )}
            {stage.action !== "fake" && (
              <Switch
                label={t("Add decoys before real fragments")}
                checked={Boolean(fake)}
                disabled={disabled}
                onChange={(event) => {
                  if (event.target.checked)
                    onChange(
                      actionOf({ ...stage, fake: defaultFake(profile) }, stage.action, profile),
                    );
                  else {
                    const next = { ...stage };
                    delete next.fake;
                    onChange(next);
                  }
                }}
              />
            )}
          </>
        )}
        {fake && (
          <>
            <Field label={t("Decoy packet appearance")}>
              <Select
                value={fake.kind}
                disabled={disabled}
                options={(tcp
                  ? [
                      { value: "tls-auto", label: t("TLS handshake based on the real connection") },
                      { value: "tls", label: t("Independent TLS handshake") },
                      { value: "http", label: t("HTTP request") },
                    ]
                  : [{ value: "quic", label: t("QUIC handshake") }]
                ).concat([
                  { value: "zero", label: t("Empty binary payload") },
                  { value: "custom", label: t("My own payload file") },
                ])}
                onChange={(kind) => {
                  const next = { ...fake, kind };
                  if (kind !== "custom") {
                    delete next.payload_file;
                    delete next.hex;
                  }
                  if (kind === "zero" || kind === "custom") {
                    delete next.server_name;
                    delete next.server_name_source;
                    delete next.server_names;
                    delete next.server_name_file;
                  } else if (
                    !next.server_name &&
                    !next.server_name_source &&
                    !next.server_names &&
                    !next.server_name_file
                  )
                    next.server_name_source = "noise-extended";
                  setFake(next);
                }}
              />
            </Field>
            <div>
              <Slider
                label={t("Decoy repetitions")}
                showValue
                value={fake.repeats || 1}
                min={1}
                max={32}
                step={1}
                marks={[1, 8, 16, 32]}
                disabled={disabled}
                onChange={(repeats) => setFake({ ...fake, repeats })}
              />
              <p className="mt-1 text-xs text-[var(--rk-muted)]">
                {t(
                  "Per changed packet and step. TLS AUTO sends two decoys per repetition; more noise can increase delays.",
                )}
              </p>
            </div>
            {!["zero", "custom"].includes(fake.kind) && (
              <>
                <Field
                  label={t("Noise dictionary")}
                  hint={t(
                    "A random hostname is selected for each new connection. Destination, real TLS hostname and certificates stay unchanged.",
                  )}
                >
                  <Select
                    value={source}
                    disabled={disabled}
                    options={[
                      ...pools.map((item) => ({
                        value: item.id,
                        label: `${sourceName(item.id, item.label)} · ${item.count}`,
                      })),
                      { value: "fixed", label: t("One hostname") },
                      { value: "inline", label: t("My hostname list") },
                      { value: "file", label: t("My dictionary file") },
                    ]}
                    onChange={(value) => setFake(withFakeSource(fake, value))}
                  />
                </Field>
                {pool && (
                  <p className="text-xs text-[var(--rk-muted)]">
                    <a href={pool.source} target="_blank" rel="noreferrer" className="underline">
                      {t("Dictionary source")}
                    </a>{" "}
                    · {pool.revision.slice(0, 10)} ·{" "}
                    {t("Bundled snapshot; availability varies by provider and region.")}
                  </p>
                )}
                {source === "fixed" && (
                  <Field label={t("Decoy hostname")}>
                    <Input
                      value={fake.server_name ?? ""}
                      disabled={disabled}
                      onChange={(event) => setFake({ ...fake, server_name: event.target.value })}
                    />
                  </Field>
                )}
                {source === "inline" && (
                  <Field label={t("Hostnames, one per line")}>
                    <ListInput
                      value={fake.server_names ?? []}
                      disabled={disabled}
                      onChange={(server_names) => setFake({ ...fake, server_names })}
                      rows={4}
                    />
                  </Field>
                )}
                {source === "file" && (
                  <Field
                    label={t("Dictionary file")}
                    hint={t(
                      "Choose a .txt file with one hostname per line. Comments start with #.",
                    )}
                  >
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={disabled}
                      onClick={async () => {
                        setFileError(null);
                        try {
                          const file = await api.voltDictionaryPick();
                          if (file)
                            setFake({ ...withFakeSource(fake, "file"), server_name_file: file });
                        } catch (failure) {
                          setFileError(asAppError(failure).message);
                        }
                      }}
                    >
                      {t("Choose .txt")}
                    </Button>
                    <Input
                      value={fake.server_name_file ?? ""}
                      disabled={disabled}
                      onChange={(event) =>
                        setFake({ ...fake, server_name_file: event.target.value })
                      }
                    />
                    {fileError && <Callout tone="danger">{fileError}</Callout>}
                  </Field>
                )}
              </>
            )}
            {fake.kind === "custom" && (
              <Field
                label={t("Payload file")}
                hint={t(
                  "Absolute path or relative to VOLT runtime data. Existing hexadecimal payloads remain available in Code.",
                )}
              >
                <Input
                  value={typeof fake.payload_file === "string" ? fake.payload_file : ""}
                  disabled={disabled}
                  onChange={(event) => {
                    const next = { ...fake, payload_file: event.target.value };
                    delete next.hex;
                    setFake(next);
                  }}
                />
              </Field>
            )}
            {(fake.ttl ?? 0) > 0 && (
              <Callout tone="warn">
                {t(
                  "Decoy hop limit is {ttl}. A low value can discard decoys before they reach the network filter. If all strategies fail, set Decoy lifetime to 0 in Advanced decoy settings to keep the original packet's value.",
                  { ttl: fake.ttl ?? 0 },
                )}
              </Callout>
            )}
            <details className="min-w-0">
              <summary className="cursor-pointer text-sm font-medium">
                {t("Advanced decoy settings")}
              </summary>
              <div className="mt-3">
                <NumberField
                  label={t("Decoy lifetime")}
                  hint={t(
                    "IP hop limit for decoys. Zero keeps the original value; real packets keep their own lifetime.",
                  )}
                  value={fake.ttl ?? 0}
                  min={0}
                  max={255}
                  disabled={disabled}
                  onChange={(ttl) => setFake({ ...fake, ttl })}
                />
              </div>
            </details>
          </>
        )}
        {stage.payloads?.length ? (
          <p className="text-xs text-[var(--rk-muted)]">
            {t("This step only changes these payloads: {kinds}", {
              kinds: stage.payloads.join(", "),
            })}
          </p>
        ) : null}
      </div>
    </details>
  );
}

export default function StrategyForm({
  yaml,
  onChange,
  onPending,
  disabled,
  directAuto,
  relay,
  pools,
}: {
  yaml: string;
  onChange: (yaml: string) => void;
  onPending: (pending: boolean) => void;
  disabled: boolean;
  directAuto: boolean;
  relay: boolean;
  pools: Pools;
}) {
  const [strategy, setStrategy] = useState<api.VoltStrategy | null>(null);
  const [selected, setSelected] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const current = useRef<api.VoltStrategy | null>(null);
  const revision = useRef(0);
  const ours = useRef<string | null>(null);
  const stepIds = useRef(new WeakMap<api.VoltTransform, string>());
  const stepKey = (stage: api.VoltTransform) => {
    let id = stepIds.current.get(stage);
    if (!id) {
      id = crypto.randomUUID();
      stepIds.current.set(stage, id);
    }
    return id;
  };
  useEffect(() => {
    if (yaml === ours.current) return;
    const version = ++revision.current;
    onPending(true);
    api
      .voltStrategyParse(yaml)
      .then(
        (parsed) => {
          if (version !== revision.current) return;
          current.current = parsed;
          setStrategy(parsed);
          setError(null);
        },
        (failure) => {
          if (version !== revision.current) return;
          current.current = null;
          setStrategy(null);
          setError(asAppError(failure).message);
        },
      )
      .finally(() => {
        if (version === revision.current) onPending(false);
      });
    return () => {
      revision.current++;
      onPending(false);
    };
  }, [yaml, onPending]);
  const commit = (next: api.VoltStrategy) => {
    const version = ++revision.current;
    current.current = next;
    setStrategy(next);
    onPending(true);
    api
      .voltStrategyRender(next)
      .then(
        (rendered) => {
          if (version !== revision.current) return;
          ours.current = rendered;
          setError(null);
          onChange(rendered);
        },
        (failure) => {
          if (version === revision.current) setError(asAppError(failure).message);
        },
      )
      .finally(() => {
        if (version === revision.current) onPending(false);
      });
  };
  const profileIndex = Math.min(selected, Math.max(0, (strategy?.profiles.length ?? 1) - 1));
  const profile = strategy?.profiles[profileIndex];
  const changeProfile = (next: api.VoltProfile) => {
    const value = current.current;
    if (value)
      commit({
        ...value,
        profiles: value.profiles.map((item, index) => (index === profileIndex ? next : item)),
      });
  };
  const moveProfile = (offset: number) => {
    if (!strategy) return;
    const profiles = [...strategy.profiles];
    [profiles[profileIndex], profiles[profileIndex + offset]] = [
      profiles[profileIndex + offset],
      profiles[profileIndex],
    ];
    setSelected(profileIndex + offset);
    commit({ ...strategy, profiles });
  };
  return (
    <div className="flex min-w-0 flex-col gap-4">
      {error && (
        <Callout tone="danger">
          {t("The visual editor cannot read this strategy. Open Code to fix it.")}
          <div>{error}</div>
        </Callout>
      )}
      {strategy && (
        <>
          <Field
            label={t("Traffic profile")}
            hint={t(
              "The first matching profile wins. Profiles below it do not run; steps inside one profile run in order.",
            )}
          >
            <Select
              value={String(profileIndex)}
              disabled={disabled}
              options={strategy.profiles.map((item, index) => ({
                value: String(index),
                label: `${index + 1}. ${item.name}`,
              }))}
              onChange={(value) => setSelected(Number(value))}
              placeholder={t("No traffic profiles")}
            />
          </Field>
          <div className="flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              variant="ghost"
              icon={<Plus />}
              disabled={disabled || strategy.profiles.length >= 64}
              onClick={() => {
                const names = new Set(strategy.profiles.map((item) => item.name));
                let id = 1;
                while (names.has(`profile-${id}`)) id++;
                setSelected(strategy.profiles.length);
                commit({
                  ...strategy,
                  profiles: [
                    ...strategy.profiles,
                    {
                      name: `profile-${id}`,
                      match: { network: "tcp", payloads: ["tls"] },
                      stages: [
                        {
                          action: "split",
                          positions: ["1", "midsld"],
                          packet_limit: 2,
                          byte_limit: 16384,
                        },
                      ],
                    },
                  ],
                });
              }}
            >
              {t("Add traffic profile")}
            </Button>
            <IconButton
              icon={<ArrowUp />}
              label={t("Move profile earlier")}
              size="sm"
              variant="ghost"
              disabled={disabled || !profile || profileIndex === 0}
              onClick={() => moveProfile(-1)}
            />
            <IconButton
              icon={<ArrowDown />}
              label={t("Move profile later")}
              size="sm"
              variant="ghost"
              disabled={disabled || !profile || profileIndex === strategy.profiles.length - 1}
              onClick={() => moveProfile(1)}
            />
            <IconButton
              icon={<Trash2 />}
              label={t("Remove traffic profile")}
              size="sm"
              variant="ghost"
              disabled={disabled || !profile}
              onClick={() =>
                commit({
                  ...strategy,
                  profiles: strategy.profiles.filter((_, index) => index !== profileIndex),
                })
              }
            />
          </div>
          {profile && (
            <>
              <div className="grid gap-4 sm:grid-cols-2">
                <Field
                  label={t("Profile name")}
                  hint={t("Letters, digits, hyphens and underscores; up to 64 characters.")}
                >
                  <Input
                    value={profile.name}
                    disabled={disabled}
                    maxLength={64}
                    onChange={(event) => changeProfile({ ...profile, name: event.target.value })}
                  />
                </Field>
                <Field label={t("Transport")}>
                  <Select
                    value={profile.match.network}
                    disabled={disabled}
                    options={[
                      { value: "tcp", label: "TCP" },
                      { value: "udp", label: "UDP" },
                    ]}
                    onChange={(value) => changeProfile(networkOf(profile, value))}
                  />
                </Field>
              </div>
              <Field
                label={t("Recognizable traffic")}
                hint={t(
                  "Nothing selected means any payload. Unknown encrypted UDP remains intact; arbitrary UDP fragmentation is unsupported.",
                )}
              >
                <CheckboxGroup
                  disabled={disabled}
                  value={profile.match.payloads ?? []}
                  options={PAYLOADS[profile.match.network].map((value) => ({
                    value,
                    label:
                      value === "unknown"
                        ? t("Unrecognized payload")
                        : value === "discord"
                          ? t("Discord voice")
                          : value.toUpperCase(),
                  }))}
                  onChange={(payloads) => changeProfile(withPayloads(profile, payloads))}
                />
              </Field>
              <details className="rounded-md border border-[var(--rk-line)] p-3">
                <summary className="cursor-pointer text-sm font-medium">
                  {t("Limit this profile to destinations")}
                </summary>
                <div className="mt-3 flex flex-col gap-4">
                  <Field
                    label={t("Destination hostnames")}
                    hint={t(
                      "Empty means any destination. Hostname suffixes are matched; these are real destinations, separate from decoy dictionaries.",
                    )}
                  >
                    <ListInput
                      value={profile.match.hosts ?? []}
                      disabled={disabled}
                      onChange={(hosts) =>
                        changeProfile({ ...profile, match: { ...profile.match, hosts } })
                      }
                    />
                  </Field>
                  <Field label={t("Leave these hostnames unchanged")}>
                    <ListInput
                      value={profile.match.exclude_hosts ?? []}
                      disabled={disabled}
                      onChange={(exclude_hosts) =>
                        changeProfile({ ...profile, match: { ...profile.match, exclude_hosts } })
                      }
                    />
                  </Field>
                  <Field
                    label={t("Destination ports")}
                    hint={t(
                      "Empty means any port. One port or range per line, for example 443 or 5000-5100.",
                    )}
                  >
                    <ListInput
                      value={profile.match.ports ?? []}
                      disabled={disabled}
                      onChange={(ports) =>
                        changeProfile({ ...profile, match: { ...profile.match, ports } })
                      }
                    />
                  </Field>
                  {[
                    "all",
                    "any",
                    "not",
                    "signatures",
                    "host_files",
                    "exclude_host_files",
                    "ip_ranges",
                    "exclude_ip_ranges",
                  ].some((key) => profile.match[key] !== undefined) && (
                    <Callout tone="info">
                      {t(
                        "This profile also has advanced matching. Those conditions are preserved; edit them in Code.",
                      )}
                    </Callout>
                  )}
                </div>
              </details>
              {stepsOf(profile).map((stage, index, steps) => (
                <StageEditor
                  key={stepKey(stage)}
                  stage={stage}
                  profile={profile}
                  index={index}
                  count={steps.length}
                  pools={pools}
                  disabled={disabled}
                  onChange={(next) => {
                    stepIds.current.set(next, stepKey(stage));
                    changeProfile(
                      withSteps(
                        profile,
                        steps.map((item, at) => (at === index ? next : item)),
                      ),
                    );
                  }}
                  onMove={(offset) => {
                    const next = [...steps];
                    [next[index], next[index + offset]] = [next[index + offset], next[index]];
                    changeProfile(withSteps(profile, next));
                  }}
                  onRemove={() =>
                    changeProfile(
                      withSteps(
                        profile,
                        steps.filter((_, at) => at !== index),
                      ),
                    )
                  }
                />
              ))}
              <Button
                size="sm"
                variant="ghost"
                icon={<Plus />}
                className="self-start"
                disabled={disabled || stepsOf(profile).length >= 8}
                onClick={() => {
                  const steps = [...stepsOf(profile)];
                  const at = steps.at(-1)?.action === "pass" ? steps.length - 1 : steps.length;
                  steps.splice(at, 0, {
                    action: profile.match.network === "tcp" ? "split" : "fake",
                    ...(profile.match.network === "tcp"
                      ? { positions: ["1"] }
                      : { fake: defaultFake(profile) }),
                    packet_limit: 2,
                    byte_limit: 16384,
                  });
                  changeProfile(withSteps(profile, steps));
                }}
              >
                {t("Add transformation step")}
              </Button>
            </>
          )}
          <details className="rounded-md border border-[var(--rk-line)] p-3">
            <summary className="cursor-pointer text-sm font-medium">
              {relay ? t("UDP connection limits") : t("UDP processing")}
            </summary>
            <div className="mt-3 flex flex-col gap-4">
              <Switch
                label={relay ? t("Allow UDP through Relay") : t("Modify UDP traffic")}
                checked={strategy.udp?.enabled ?? false}
                disabled={disabled}
                onChange={(event) =>
                  commit({ ...strategy, udp: { ...strategy.udp, enabled: event.target.checked } })
                }
              />
              {relay && (
                <NumberField
                  label={t("UDP destinations per connection")}
                  value={strategy.udp?.max_destinations || 32}
                  min={1}
                  max={256}
                  disabled={disabled}
                  onChange={(max_destinations) =>
                    commit({ ...strategy, udp: { ...strategy.udp, max_destinations } })
                  }
                />
              )}
              {relay && (
                <NumberField
                  label={t("Close idle UDP connections after")}
                  value={strategy.udp?.idle_timeout_seconds || 60}
                  min={1}
                  max={3600}
                  unit={t("s")}
                  disabled={disabled}
                  onChange={(idle_timeout_seconds) =>
                    commit({ ...strategy, udp: { ...strategy.udp, idle_timeout_seconds } })
                  }
                />
              )}
            </div>
          </details>
          {directAuto && (
            <details className="rounded-md border border-[var(--rk-line)] p-3">
              <summary className="cursor-pointer text-sm font-medium">
                {t("DIRECT-AUTO timing and memory")}
              </summary>
              <div className="mt-3 flex flex-col gap-4">
                <NumberField
                  label={t("Wait before trying VOLT in parallel")}
                  hint={t(
                    "Direct TLS starts immediately. VOLT starts after this delay if direct has not succeeded.",
                  )}
                  value={strategy.auto?.fallback_delay_ms || 250}
                  min={100}
                  max={30000}
                  unit={t("ms")}
                  disabled={disabled}
                  onChange={(fallback_delay_ms) =>
                    commit({ ...strategy, auto: { ...strategy.auto, fallback_delay_ms } })
                  }
                />
                <NumberField
                  label={t("Direct handshake timeout")}
                  value={strategy.auto?.direct_timeout_ms || 1500}
                  min={100}
                  max={30000}
                  unit={t("ms")}
                  disabled={disabled}
                  onChange={(direct_timeout_ms) =>
                    commit({ ...strategy, auto: { ...strategy.auto, direct_timeout_ms } })
                  }
                />
                <NumberField
                  label={t("VOLT handshake timeout")}
                  value={strategy.auto?.fallback_timeout_ms || 5000}
                  min={100}
                  max={30000}
                  unit={t("ms")}
                  disabled={disabled}
                  onChange={(fallback_timeout_ms) =>
                    commit({ ...strategy, auto: { ...strategy.auto, fallback_timeout_ms } })
                  }
                />
                <Switch
                  label={t("Remember which route worked")}
                  checked={strategy.auto?.route_cache?.enabled ?? true}
                  disabled={disabled}
                  onChange={(event) =>
                    commit({
                      ...strategy,
                      auto: {
                        ...strategy.auto,
                        route_cache: {
                          ...strategy.auto?.route_cache,
                          enabled: event.target.checked,
                        },
                      },
                    })
                  }
                />
                <NumberField
                  label={t("Remember a route for")}
                  value={strategy.auto?.route_cache?.ttl_seconds || 60}
                  min={1}
                  max={3600}
                  unit={t("s")}
                  disabled={disabled}
                  onChange={(ttl_seconds) =>
                    commit({
                      ...strategy,
                      auto: {
                        ...strategy.auto,
                        route_cache: { ...strategy.auto?.route_cache, ttl_seconds },
                      },
                    })
                  }
                />
                <NumberField
                  label={t("Remembered destinations")}
                  value={strategy.auto?.route_cache?.max_entries || 256}
                  min={1}
                  max={4096}
                  disabled={disabled}
                  onChange={(max_entries) =>
                    commit({
                      ...strategy,
                      auto: {
                        ...strategy.auto,
                        route_cache: { ...strategy.auto?.route_cache, max_entries },
                      },
                    })
                  }
                />
              </div>
            </details>
          )}
        </>
      )}
    </div>
  );
}
