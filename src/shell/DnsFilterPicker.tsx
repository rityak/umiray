import { SegmentedControl, Text } from "rootik";
import type * as api from "../api";
import { t, tk } from "../i18n";

/// Из каких DNS выбирает подбор (S-033). Порядок — от осторожного к широкому.
const FILTERS: { value: api.DnsFilter; label: string; hint: string }[] = [
  {
    value: "clean",
    label: tk("No filtering"),
    hint: tk("Servers that answer as is: nothing is blocked or replaced."),
  },
  {
    value: "ads",
    label: tk("Block ads"),
    hint: tk("Ad and tracker addresses don't resolve, so ads don't load in any app."),
  },
  {
    value: "any",
    label: tk("Any"),
    hint: tk("The fastest of all, including ones that block ads or dangerous sites."),
  },
];

type Props = {
  value: api.DnsFilter;
  onChange: (value: api.DnsFilter) => void;
  disabled?: boolean;
};

/// Выбор категории DNS для подбора: в мастере и в «Настройках mihomo».
export default function DnsFilterPicker({ value, onChange, disabled }: Props) {
  const chosen = FILTERS.find((item) => item.value === value) ?? FILTERS[0];
  return (
    <div className="flex flex-col gap-1">
      <SegmentedControl<api.DnsFilter>
        aria-label={t("DNS filtering")}
        fill
        disabled={disabled}
        options={FILTERS.map((item) => ({ value: item.value, label: t(item.label) }))}
        value={value}
        onChange={onChange}
      />
      <Text size="xs" tone="muted">
        {t(chosen.hint)}
      </Text>
    </div>
  );
}
