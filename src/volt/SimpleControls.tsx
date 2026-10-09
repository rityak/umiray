import { FileText } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button, Callout, Field, Select, Slider } from "rootik";
import { asAppError } from "../api/call";
import * as api from "../api/volt";
import { t } from "../i18n";
import { candidateName } from "./AutoTune";
import { readSimple, setDictionary, setPackets, setRepeats } from "./simple";

const TLS_PRESETS = ["tls-split", "tls-disorder", "tls-fake", "tls-fake-split", "tls-auto"];

/**
 * A strategy seen as a template (D-187): `select` — the method, recognized from the YAML
 * itself; `knobs` — the template's quick settings for Fine-tuning. Two parts of one
 * reading, so the page shows each strategy field once.
 */
export default function SimpleControls({
  part,
  yaml,
  disabled,
  relay,
  pools,
  vpnTcp,
  vpnNoise,
  label,
  hint,
  onChange,
  onPending,
  onCode,
  onParsed,
}: {
  part: "select" | "knobs";
  yaml: string;
  disabled: boolean;
  relay: boolean;
  pools: api.VoltSnapshot["domainPools"];
  vpnTcp: string;
  vpnNoise: string;
  label?: string;
  hint?: string;
  onChange: (yaml: string) => void;
  onPending: (pending: boolean) => void;
  onCode: () => void;
  onParsed?: (strategy: api.VoltStrategy | null) => void;
}) {
  const [strategy, setStrategy] = useState<api.VoltStrategy | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const current = useRef<api.VoltStrategy | null>(null);
  const revision = useRef(0);
  const ours = useRef<string | null>(null);
  useEffect(() => {
    if (yaml === ours.current) return;
    const version = ++revision.current;
    setLoading(true);
    onPending(true);
    api
      .voltStrategyParse(yaml)
      .then(
        (parsed) => {
          if (version !== revision.current) return;
          current.current = parsed;
          setStrategy(parsed);
          onParsed?.(parsed);
          setError(null);
        },
        (failure) => {
          if (version !== revision.current) return;
          current.current = null;
          setStrategy(null);
          onParsed?.(null);
          setError(asAppError(failure).message);
        },
      )
      .finally(() => {
        if (version === revision.current) {
          setLoading(false);
          onPending(false);
        }
      });
    return () => {
      revision.current++;
      onPending(false);
    };
  }, [yaml, onPending, onParsed]);

  const commit = (next: api.VoltStrategy) => {
    const previous = current.current;
    const version = ++revision.current;
    current.current = next;
    setStrategy(next);
    onParsed?.(next);
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
          if (version !== revision.current) return;
          current.current = previous;
          setStrategy(previous);
          setError(asAppError(failure).message);
        },
      )
      .finally(() => {
        if (version === revision.current) onPending(false);
      });
  };
  const edit = (update: (value: api.VoltStrategy) => api.VoltStrategy) => {
    if (!current.current) return;
    try {
      commit(update(current.current));
    } catch (failure) {
      setError(asAppError(failure).message);
    }
  };
  const replace = async (id: string) => {
    const version = ++revision.current;
    setLoading(true);
    onPending(true);
    try {
      const rendered =
        id === "tcp-split"
          ? vpnTcp
          : id === "vpn-noise"
            ? vpnNoise
            : await api.voltStrategyPreset(yaml, id);
      const parsed = await api.voltStrategyParse(rendered);
      if (version !== revision.current) return;
      current.current = parsed;
      setStrategy(parsed);
      ours.current = rendered;
      setError(null);
      onChange(rendered);
    } catch (failure) {
      if (version === revision.current) setError(asAppError(failure).message);
    } finally {
      if (version === revision.current) {
        setLoading(false);
        onPending(false);
      }
    }
  };
  const pickFile = async () => {
    const version = revision.current;
    try {
      const file = await api.voltDictionaryPick();
      if (file && version === revision.current) edit((value) => setDictionary(value, "file", file));
    } catch (failure) {
      setError(asAppError(failure).message);
    }
  };
  const simple = strategy ? readSimple(strategy) : null;
  const locked = disabled || loading;
  const quic = strategy?.profiles.some(
    (profile) =>
      profile.match.network === "udp" &&
      profile.match.payloads?.includes("quic") &&
      (profile.stages ?? [profile.transform]).some((stage) => stage?.fake?.kind === "quic"),
  );
  const preset =
    !relay && simple?.preset === "tls-auto" && quic ? "vpn-noise" : (simple?.preset ?? "custom");
  const source = simple?.source ?? "custom";
  if (part === "select")
    return (
      <Field label={label ?? t("Traffic preset")} hint={hint}>
        <Select
          value={preset}
          disabled={locked}
          options={[
            ...TLS_PRESETS.map((id) => ({ value: id, label: candidateName(id, id) })),
            ...(!relay
              ? [
                  { value: "tcp-split", label: t("TCP fragmentation") },
                  { value: "vpn-noise", label: t("TLS + QUIC noise (experimental)") },
                ]
              : []),
            { value: "custom", label: t("Custom strategy · Code") },
          ]}
          onChange={(id) => {
            if (id === "custom") onCode();
            else void replace(id);
          }}
        />
        {error && <Callout tone="danger">{error}</Callout>}
      </Field>
    );
  return (
    <div className="flex min-w-0 flex-col gap-3">
      {simple?.preset === "custom" && (
        <Button
          size="sm"
          variant="ghost"
          className="self-start"
          icon={<FileText />}
          onClick={onCode}
        >
          {t("Edit custom strategy in Code")}
        </Button>
      )}
      {simple?.packets !== null && simple?.packets !== undefined && (
        <Slider
          label={t("Process the start of a connection")}
          min={1}
          max={8}
          step={1}
          value={simple.packets}
          disabled={locked}
          showValue={(value) => t("First {n} packets", { n: value })}
          onChange={(value) => edit((parsed) => setPackets(parsed, value))}
        />
      )}
      {simple?.repeats !== null && simple?.repeats !== undefined && (
        <>
          <Slider
            label={t("Noise amount")}
            min={1}
            max={16}
            step={1}
            value={simple.repeats}
            disabled={locked}
            showValue={(value) => t("{n} decoys per packet", { n: value * 2 })}
            onChange={(value) => edit((parsed) => setRepeats(parsed, value))}
          />
          <Field label={t("Noise dictionary")}>
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <div className="min-w-0 flex-1">
                <Select
                  value={source}
                  disabled={locked}
                  options={[
                    ...pools.map((pool) => ({
                      value: pool.id,
                      label: `${pool.id === "noise-compact" ? t("Compact") : pool.id === "noise-extended" ? t("Extended") : pool.label} · ${pool.count}`,
                    })),
                    { value: "file", label: t("My dictionary file") },
                    ...(!pools.some((pool) => pool.id === source) && source !== "file"
                      ? [{ value: source, label: t("Dictionary from Code") }]
                      : []),
                  ]}
                  onChange={(value) => {
                    if (value === "file") void pickFile();
                    else edit((parsed) => setDictionary(parsed, value));
                  }}
                />
              </div>
              <Button
                size="sm"
                variant="secondary"
                icon={<FileText />}
                disabled={locked}
                onClick={() => void pickFile()}
              >
                {t("Choose .txt")}
              </Button>
            </div>
            {simple.file && (
              <span className="truncate text-xs text-[var(--rk-muted)]" title={simple.file}>
                {simple.file.split(/[\\/]/).at(-1)}
              </span>
            )}
          </Field>
        </>
      )}
    </div>
  );
}
