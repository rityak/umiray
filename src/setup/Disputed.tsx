import { Switch, Text } from "rootik";
import { t, tk } from "../i18n";

/// Что решает сам человек (D-169): у каждой опции есть цена, и правильного ответа на все
/// машины нет. Новая спорная опция — запись здесь и поле в «Настройках mihomo».
export type Choices = { sniffer: boolean; preferH3: boolean; openNat: boolean };

/// С чего начинает первый запуск: так, как советует рекомендованный конфиг.
export const ADVISED: Choices = { sniffer: true, preferH3: true, openNat: true };

const OPTIONS: { key: keyof Choices; label: string; about: string }[] = [
  {
    key: "sniffer",
    label: tk("Recognize sites by connection"),
    about: tk(
      "Site rules also work for apps that connect by address, not by name. Rarely, an app stops working with it — then turn it off.",
    ),
  },
  {
    key: "preferH3",
    label: tk("DNS over HTTP/3"),
    about: tk(
      "Names resolve faster if your provider lets QUIC through. If it blocks QUIC, the first lookup waits a moment and then goes the usual way.",
    ),
  },
  {
    key: "openNat",
    label: tk("Open NAT"),
    about: tk(
      "Games, calls and torrents connect to other people more easily. Works in TUN mode and adds a little load.",
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
          "Each of these usually helps, but not on every computer. All of them stay in Mihomo Settings.",
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
