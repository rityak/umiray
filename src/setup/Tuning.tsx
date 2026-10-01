import { FileCog, Ruler, Server } from "lucide-react";
import type { ReactNode } from "react";
import { Item, ItemGroup, Switch, Text } from "rootik";
import type * as api from "../api";
import { t, tk } from "../i18n";

type Tune = {
  id: api.Tuning;
  /// Термин как есть (COPY): его ищут этим словом.
  title: string;
  what: string;
  /// Подпись кнопки, пока идёт замер.
  doing: string;
  icon: ReactNode;
  /// Итог зависит от блокировки рекламы: её смена перемеряет только такие подборы.
  filtered?: true;
};

/// Что делает «Рекомендованная» (D-105, D-162, D-169): запись плюс замеры в «Настройки
/// mihomo». Новый подбор — запись здесь: его и прогонит мастер, и увидит человек до «Готово».
export const TUNING: Tune[] = [
  // Первым: замеры ниже кладут резолверы и MTU уже поверх него (D-169).
  {
    id: "recommended",
    title: "mihomo",
    what: tk("Sets up the core the same way for Proxy, System and TUN."),
    doing: tk("Writing the core config…"),
    icon: <FileCog />,
  },
  {
    id: "dns-race",
    title: "DNS",
    what: tk(
      "Checks public DNS servers and keeps the four fastest. The core asks all of them at once and takes the first answer.",
    ),
    doing: tk("Measuring resolvers…"),
    icon: <Server />,
    filtered: true,
  },
  {
    id: "pmtu",
    title: "MTU",
    what: tk(
      "Finds the largest packet that gets through whole and leaves room for the tunnel header.",
    ),
    doing: tk("Measuring MTU…"),
    icon: <Ruler />,
  },
];

type Props = {
  /// Блокировать рекламу: DNS только из блокирующих и готовый набор правил (D-169).
  ads: boolean;
  onAds: (on: boolean) => void;
};

/// Список под карточкой «Рекомендованная»: что будет записано и замерено.
export default function Tuning({ ads, onAds }: Props) {
  return (
    <div className="flex flex-col gap-3">
      <Text size="sm" tone="muted">
        {t("What Recommended does")}
      </Text>
      <ItemGroup>
        {TUNING.map((tune) => (
          <Item key={tune.id} icon={tune.icon} title={tune.title} description={t(tune.what)} />
        ))}
      </ItemGroup>
      <Switch
        label={t("Block ads")}
        description={t(
          "Ads and trackers won't load in the browser or in apps. DNS is picked only from servers that block ads, and the ad-blocking rules are turned on.",
        )}
        checked={ads}
        onChange={(event) => onAds(event.target.checked)}
      />
      <Text size="xs" tone="muted">
        {t("Everything is saved to Mihomo Settings, where you can change it.")}
      </Text>
    </div>
  );
}
