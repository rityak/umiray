import type { ReactNode } from "react";
import * as api from "./api";
import { t } from "./i18n";
import type * as qd from "./qd/api";
import Uptime from "./shell/Uptime";
import type { AddKind } from "./sources/AddMenu";

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
  /// Вторая строка шапки, пока это ядро держит трафик: чем оно его держит.
  detail: (seen: Seen) => ReactNode;
  /// Разделы конфига, какими их показывает это ядро: чего у него нет — нет и в dock.
  sections: (all: api.ConfigSection[]) => api.ConfigSection[];
  /// Настройки клиента (`id` групп, частей и полей формы), которых у этого ядра нет.
  hiddenClientSettings: ReadonlySet<string>;
  /// Что предлагает «+» на виде этого ядра (D-160). Одно — меню нет, «+» открывает его сразу.
  adds: AddKind[];
};

/**
 * Реестр ядер окна (D-154). Новое ядро — запись здесь, папка его разделов и ветка
 * в `App`, где разделы выбираются. Общие компоненты про ядра не знают: им передают
 * возможности (`measure`, `onHidden`, `editable`), а не имя ядра.
 */
export const ENGINES: Record<api.Engine, Engine> = {
  mihomo: {
    label: "mihomo",
    detail: ({ status }) => {
      const mode = api.runningMode(status);
      return (
        <>
          {mode && `${api.MODE_LABEL[mode]} · `}
          <Uptime started={status.started} fallback={t("just now")} />
        </>
      );
    },
    sections: (all) => all,
    hiddenClientSettings: new Set(),
    adds: ["link", "file", "node", "warp"],
    // Без явного списка `AddMenu` предлагает то же самое — виды mihomo его не передают.
  },
  qd: {
    label: "qd",
    detail: ({ qd: seen, status }) =>
      seen?.state?.node?.name ?? <Uptime started={status.started} fallback={t("just now")} />,
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
    // qd берёт только свою ссылку `qd://` (D-161): файл, форма и WARP — узлы mihomo.
    adds: ["link"],
  },
};

/**
 * Шапка — о клиенте, а не о виде (D-154): включён ли VPN и каким ядром, в любом разделе
 * и при любом положении переключателя. Беды одного ядра говорит его кнопка питания.
 */
export function headline(seen: Seen): Headline {
  const { status, powering } = seen;
  const on = status.active && ENGINES[status.active];
  const detail = on ? on.detail(seen) : t("core stopped");
  if (powering) return { tone: "connecting", label: t("Starting…"), detail };
  if (!on) return { tone: "off", label: t("Disconnected"), detail };
  return { tone: "on", label: `${t("Connected")} · ${on.label}`, detail };
}
