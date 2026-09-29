import type { ReactNode } from "react";
import * as api from "./api";
import { t } from "./i18n";
import type * as qd from "./qd/api";
import Uptime from "./shell/Uptime";

/// Документ «Настройки qd»: своя форма (`QdSettings`), файла за ним нет.
export const QD_SETTINGS = "qd";

/// Что окно знает о каждом ядре, чтобы показать его состояние (D-154).
type Seen = {
  status: api.Status;
  qd: qd.Status | null;
  powering: boolean;
};

export type Headline = { tone: api.Tone; label: string; detail: ReactNode };

type Engine = {
  label: string;
  /// Строка состояния в шапке. Шапка говорит о **работающем** ядре; ничего не работает —
  /// о выбранном.
  headline: (seen: Seen) => Headline;
  /// Разделы конфига, какими их показывает это ядро: чего у него нет — нет и в dock.
  sections: (all: api.ConfigSection[]) => api.ConfigSection[];
  /// Настройки клиента (`id` групп, частей и полей формы), которых у этого ядра нет.
  hiddenClientSettings: ReadonlySet<string>;
};

/**
 * Реестр ядер окна (D-154). Новое ядро — запись здесь, папка его разделов и ветка
 * в `App`, где разделы выбираются. Общие компоненты про ядра не знают: им передают
 * возможности (`measure`, `onHidden`, `editable`), а не имя ядра.
 */
export const ENGINES: Record<api.Engine, Engine> = {
  mihomo: {
    label: "mihomo",
    headline: ({ status, powering }) => ({
      ...api.statusView(status, powering),
      detail: status.running ? (
        <Uptime started={status.started} fallback={t("just now")} />
      ) : (
        t("core stopped")
      ),
    }),
    sections: (all) => all,
    hiddenClientSettings: new Set(),
  },
  qd: {
    label: "qd",
    headline: ({ qd: seen, status, powering }) => {
      const state = seen?.state ?? null;
      const on = status.active === "qd";
      const view: Pick<Headline, "tone" | "label"> = powering
        ? { tone: "connecting", label: t("Starting…") }
        : state?.failed
          ? { tone: "error", label: t("qd connection failed") }
          : on
            ? { tone: "on", label: `${t("Connected")} · qd` }
            : { tone: "off", label: t("Disconnected") };
      return { ...view, detail: (on && state?.node?.name) || t("qd stopped") };
    },
    // Узлы qd выбирает сам — групп нет; маршруты — свои правила по приложениям,
    // без наборов; вместо конфига mihomo — свои настройки.
    sections: (all) =>
      all
        .filter((section) => section.id !== "groups")
        .map((section) => {
          if (section.id === "rules") return { ...section, docs: [] };
          if (section.id !== "advanced") return section;
          return {
            ...section,
            docs: [
              ...section.docs.filter((doc) => doc.id === "client"),
              {
                id: QD_SETTINGS,
                label: t("qd Settings"),
                hint: t("qd client settings"),
                core: false,
                applied: false,
              },
            ],
          };
        }),
    hiddenClientSettings: new Set([
      "guard",
      "antidpi",
      "nodes",
      "service-rules",
      "service-core",
      "device",
      "flush",
    ]),
  },
};

export const ENGINE_IDS = Object.keys(ENGINES) as api.Engine[];
