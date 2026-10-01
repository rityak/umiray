import { useState } from "react";
import { Button, NumberInput, Select } from "rootik";
import * as api from "../api";
import { t } from "../i18n";

type Props = {
  schedule: api.Refresh;
  onSchedule: (schedule: api.Refresh) => void;
};

/// Когда обновлять подписки: готовые варианты и «своё» — число и единица прямо в строке.
/// Сохранённое «своё» — в тех же числе и единице, какими его вводили; иначе поле при
/// следующем показе стояло на 30 минутах, и «Применить» без правки их и записывало.
function asTyped(schedule: api.Refresh): { amount: number; unit: "minutes" | "hours" } {
  const minutes = schedule.everyMinutes;
  if (minutes <= 0) return { amount: 30, unit: "minutes" };
  return minutes % 60 === 0
    ? { amount: minutes / 60, unit: "hours" }
    : { amount: minutes, unit: "minutes" };
}

export default function RefreshSchedule({ schedule, onSchedule }: Props) {
  const [amount, setAmount] = useState<number | null>(() => asTyped(schedule).amount);
  const [unit, setUnit] = useState<"minutes" | "hours">(() => asTyped(schedule).unit);
  /// Выбрали «Своё» — поля видны до «Применить», даже пока расписание совпадает с вариантом.
  const [custom, setCustom] = useState(false);
  const preset = api.refreshPreset(schedule);

  const own = (): api.Refresh => {
    const every = Math.max(1, amount ?? 1);
    return { onStart: true, everyMinutes: unit === "hours" ? every * 60 : every };
  };

  return (
    <>
      <Select
        size="sm"
        className="w-44"
        aria-label={t("Subscription refresh schedule")}
        value={String(custom ? -1 : preset)}
        onChange={(value) => {
          const index = Number(value);
          setCustom(index < 0);
          if (index >= 0) onSchedule(api.REFRESH_PRESETS[index].value);
        }}
        options={[
          ...api.REFRESH_PRESETS.map((item, index) => ({
            value: String(index),
            label: t(item.label),
          })),
          { value: "-1", label: t("Custom…") },
        ]}
      />
      {(custom || preset < 0) && (
        <>
          <NumberInput
            size="sm"
            className="w-20"
            aria-label={t("Refresh interval")}
            min={1}
            value={amount}
            onChange={setAmount}
          />
          <Select
            size="sm"
            className="w-28"
            aria-label={t("Interval unit")}
            value={unit}
            onChange={setUnit}
            options={[
              { value: "minutes", label: t("minutes") },
              { value: "hours", label: t("hours") },
            ]}
          />
          <Button
            size="sm"
            onClick={() => {
              onSchedule(own());
              setCustom(false);
            }}
          >
            {t("Apply")}
          </Button>
        </>
      )}
    </>
  );
}
