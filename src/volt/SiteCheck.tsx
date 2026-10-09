import { useState } from "react";
import { Button, Callout, Field, Input } from "rootik";
import { asAppError } from "../api/call";
import * as api from "../api/volt";
import { t } from "../i18n";

/**
 * "Check a site" (D-190): one address, directly and with the strategy Relay runs on this
 * network. Blocked directly but open through the bypass — offer it to your domains.
 */
export default function SiteCheck({
  disabled,
  canAdd,
  onAdd,
}: {
  disabled: boolean;
  /** The domain can join "Your domains" right now (lists mode, not there yet). */
  canAdd: (domain: string) => boolean;
  onAdd: (domain: string) => void;
}) {
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<api.VoltSiteCheck | null>(null);
  const [error, setError] = useState<string | null>(null);
  const check = async () => {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(await api.voltCheckSite(url));
    } catch (failure) {
      setError(asAppError(failure).message);
    } finally {
      setBusy(false);
    }
  };
  const line = (attempt: api.VoltSiteCheck["direct"]) =>
    attempt.ok
      ? t("opens, {ms} ms", { ms: attempt.latencyMs ?? 0 })
      : t("does not open{why}", { why: attempt.error ? ` — ${attempt.error}` : "" });
  const helps = result && !result.direct.ok && result.bypass.ok;
  return (
    <Field
      label={t("Check a site")}
      hint={t("Directly and with the bypass, like the strategy check.")}
    >
      <form
        className="flex min-w-0 flex-wrap gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (url.trim()) void check();
        }}
      >
        <div className="min-w-0 flex-1">
          <Input
            mono
            value={url}
            placeholder="rutracker.org"
            disabled={disabled || busy}
            onChange={(event) => setUrl(event.target.value)}
          />
        </div>
        <Button
          type="submit"
          size="sm"
          variant="secondary"
          disabled={disabled || busy || !url.trim()}
        >
          {busy ? t("Checking…") : t("Check")}
        </Button>
      </form>
      {error && <Callout tone="danger">{error}</Callout>}
      {result && (
        <Callout tone={result.direct.ok ? "info" : helps ? "success" : "warn"}>
          <div className="flex flex-col gap-1">
            <span>
              {t("Directly")}: {line(result.direct)}
            </span>
            <span>
              {t("With the bypass")}: {line(result.bypass)}
            </span>
            {helps && result.domain && canAdd(result.domain) && (
              <Button
                size="sm"
                variant="secondary"
                className="self-start"
                onClick={() => result.domain && onAdd(result.domain)}
              >
                {t("Add {domain} to your domains", { domain: result.domain })}
              </Button>
            )}
          </div>
        </Callout>
      )}
    </Field>
  );
}
