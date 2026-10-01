import { Switch, Text } from "rootik";
import { t, tk } from "../i18n";

/// Что решает сам человек (D-169): у каждой опции есть цена, и правильного ответа на все
/// машины нет. Новая спорная опция — запись здесь и поле в «Настройках mihomo».
/// `prefer-h3` здесь нет: HTTP/3 к DoH на сетях в РФ не проходит, и пользы от него ноль (S-034).
export type Choices = { sniffer: boolean; openNat: boolean };

/// С чего начинает первый запуск: так, как советует рекомендованный конфиг.
export const ADVISED: Choices = { sniffer: true, openNat: true };

/// Подпись — термин, как его ищут (COPY, правило 2); подсказка — что будет и чем заплатишь.
const OPTIONS: { key: keyof Choices; label: string; about: string }[] = [
  {
    key: "sniffer",
    label: tk("Sniffer"),
    about: tk(
      "Site rules also catch apps that connect by IP. If an app stops working, turn it off.",
    ),
  },
  {
    key: "openNat",
    label: tk("Open NAT"),
    about: tk(
      "Games, calls and torrents connect to other people more easily. TUN only, adds a little CPU load.",
    ),
  },
];

type Props = {
  value: Choices;
  onChange: (value: Choices) => void;
};

/// Шаг мастера «Тонкости»: опции, которые могут как помочь, так и помешать.
export default function Disputed({ value, onChange }: Props) {
  return (
    <div className="flex flex-col gap-4">
      <Text size="sm" tone="muted">
        {t(
          "These usually help but sometimes get in the way. Change them later in Mihomo Settings.",
        )}
      </Text>
      {OPTIONS.map((option) => (
        <Switch
          key={option.key}
          label={t(option.label)}
          description={t(option.about)}
          checked={value[option.key]}
          onChange={(event) => onChange({ ...value, [option.key]: event.target.checked })}
        />
      ))}
    </div>
  );
}
