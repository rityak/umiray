import { useState } from "react";
import { Button, Callout, Field, Switch, Textarea } from "rootik";
import type * as api from "../api/volt";
import { locale, t } from "../i18n";

export function candidateName(id: string, fallback: string): string {
  if (id === "current") return t("Saved strategy");
  if (id === "direct") return t("Directly, without VOLT");
  if (id === "tls-split") return t("Fragmentation");
  if (id === "tls-disorder") return t("Packet reordering");
  if (id === "tls-fake") return t("Noise only");
  if (id === "tls-fake-split") return t("Noise + fragmentation");
  if (id === "tls-auto") return t("Noise + reordering");
  return fallback;
}

const host = (url: string) => {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
};

/**
 * Strategy check (D-185, D-189): what it checks, when, what it chose and that the choice
 * runs on this network while the saved method stays — all without Code.
 */
export default function AutoTune({
  options,
  report,
  targets,
  disabled,
  dirty,
  code,
  onChange,
  onBusy,
  onTune,
}: {
  options: api.VoltOptions;
  report: api.VoltTuneReport | null;
  /** What the check will open: the person's own URLs or what goes through the bypass. */
  targets: string[];
  disabled: boolean;
  dirty: boolean;
  code: boolean;
  onChange: (patch: Partial<api.VoltOptions>) => void;
  onBusy: (busy: boolean) => void;
  onTune: () => Promise<api.VoltTuneReport | null>;
}) {
  const [busy, setBusy] = useState(false);
  const result = report;
  return (
    <div className="flex min-w-0 flex-col gap-2 border-t border-[var(--rk-line)] pt-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Switch
          label={t("Choose a preset at startup")}
          checked={options.autoSelect}
          disabled={disabled || busy}
          onChange={(event) => onChange({ autoSelect: event.target.checked })}
        />
        <Button
          size="sm"
          variant="secondary"
          disabled={disabled || busy || dirty || !options.directEnabled}
          onClick={async () => {
            setBusy(true);
            onBusy(true);
            try {
              await onTune();
            } finally {
              setBusy(false);
              onBusy(false);
            }
          }}
        >
          {busy ? t("Checking strategies…") : t("Check presets")}
        </Button>
      </div>
      {targets.length > 0 && (
        <p className="text-xs text-[var(--rk-muted)]">
          {t("Checks: {list}", { list: [...new Set(targets.map(host))].join(", ") })}
        </p>
      )}
      {code && (
        <Field
          label={t("HTTPS addresses for strategy checks")}
          hint={t(
            "Empty checks what goes through the bypass. Your own: at most four HTTPS URLs, one per line.",
          )}
        >
          <Textarea
            value={options.probeUrls.join("\n")}
            placeholder={targets.join("\n")}
            disabled={disabled || busy}
            onChange={(event) => onChange({ probeUrls: event.target.value.split("\n") })}
            rows={3}
          />
        </Field>
      )}
      {(dirty || !options.directEnabled) && (
        <p className="text-xs text-[var(--rk-muted)]">
          {dirty
            ? t("Apply changes before checking.")
            : t("Enable direct connections to check presets.")}
        </p>
      )}
      {result && (
        <>
          <Callout tone={result.selected || result.unblocked ? "success" : "warn"}>
            {result.unblocked
              ? t("Everything opened directly — no bypass needed here. The strategy was kept.")
              : result.selected
                ? t("Chosen strategy: {name}", {
                    name: candidateName(result.selected, result.selected),
                  })
                : t("No strategy passed every check. The saved strategy was kept.")}
            {result.selected && !result.unblocked && (
              <div className="text-xs">
                {t("Relay runs with it on this network; your saved method is unchanged.")}
              </div>
            )}
            <div className="text-xs">
              {t("Last check")}: {new Date(result.checkedAt).toLocaleString(locale())}
            </div>
          </Callout>
          {result.candidates.map((candidate) => (
            <details key={candidate.id} className="min-w-0 text-sm">
              <summary className="cursor-pointer">
                {candidateName(candidate.id, candidate.label)} · {candidate.successes}/
                {candidate.total}
                {candidate.latencyMs !== null
                  ? ` · ${Math.round(candidate.latencyMs)} ${t("ms")}`
                  : ""}
              </summary>
              <div className="mt-2 flex flex-col gap-2 pl-3 text-xs text-[var(--rk-muted)]">
                {candidate.checks.map((check) => (
                  <div key={`${check.url}-${check.attempt}`} className="break-words">
                    {check.ok ? t("Passed") : t("Failed")} · {check.url} ·{" "}
                    {t("Attempt {n}", { n: check.attempt })} · {Math.round(check.latencyMs)}{" "}
                    {t("ms")}
                    {check.error && <div>{check.error}</div>}
                  </div>
                ))}
              </div>
            </details>
          ))}
        </>
      )}
    </div>
  );
}
