import { Select } from "rootik";
import type * as api from "../api";
import { t, tk } from "../i18n";

type Props = {
  /// Absent is medium — the document does not spell out the default (D-158).
  value: api.Priority | undefined;
  label: string;
  onChange: (priority: api.Priority | undefined) => void;
};

const LEVELS: { value: api.Priority; label: string; hint: string }[] = [
  { value: "high", label: tk("High"), hint: tk("above everything but your rules") },
  { value: "medium", label: tk("Medium"), hint: tk("rule sets above ready-made sets") },
  { value: "low", label: tk("Low"), hint: tk("just above MATCH") },
];

/**
 * Where a rule set or a ready-made set stands in the route (D-158). Your own rules are
 * always above all three levels; within a level, rule sets go before ready-made sets.
 */
export default function PriorityPicker({ value, label, onChange }: Props) {
  return (
    <Select
      aria-label={label}
      value={value ?? "medium"}
      onChange={(next) => onChange(next === "medium" ? undefined : (next as api.Priority))}
      options={LEVELS.map((level) => ({
        value: level.value,
        label: t(level.label),
        hint: t(level.hint),
      }))}
    />
  );
}
