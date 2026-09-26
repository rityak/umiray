import { StatusDot, type Tone } from "rootik";
import type * as api from "../api";
import { t, tk } from "../i18n";

const TONE: Record<api.Verdict, Tone> = {
  ok: "success",
  warn: "warn",
  bad: "danger",
  idle: "neutral",
};

const WORD: Record<api.Verdict, string> = {
  ok: tk("ok"),
  warn: tk("caveat"),
  bad: tk("failed"),
  idle: tk("not run"),
};

/// A probe's verdict: a dot and a word. `label` replaces the word where the utility has
/// something more precise to say ("5 of 9"); `hideLabel` keeps the word for screen readers
/// only — for dense lists.
export default function Verdict({
  value,
  label,
  hideLabel,
}: {
  value: api.Verdict;
  label?: string;
  hideLabel?: boolean;
}) {
  return (
    <StatusDot
      tone={TONE[value]}
      label={label && label.length > 0 ? label : t(WORD[value])}
      hideLabel={hideLabel}
    />
  );
}
