import { useCallback, useState } from "react";
import { toast } from "rootik";
import * as api from "../api";
import { t } from "../i18n";
import * as qd from "../qd/api";
import { failure, type Message, notice } from "../shell/Banner";
import type { AddKind } from "../sources/AddMenu";

type Deps = {
  report: (message: Message | null) => void;
  /// Что-то добавилось: источники, статус (qd мог появиться), состояние qd — перечитать.
  onAdded: () => Promise<void>;
};

/// Ссылка `qd://` уходит в qd, остальное — источник mihomo (D-161). qd скачивается, если
/// его нет, — секунды, о которых говорит тост: иначе нажатие выглядело бы зависшим.
async function addLink(input: string): Promise<Message> {
  if (!qd.isLink(input)) {
    const result = await api.sourcesAdd(input);
    return notice(api.importSummary(result), result.notices);
  }
  const id = toast({ title: t("Adding the qd link…"), loading: true });
  try {
    const got = await qd.adopt(input);
    toast.success(
      got.downloaded
        ? t("{engine} {version} downloaded", { engine: "qd", version: got.downloaded })
        : t("qd link added"),
      { id },
    );
    return got.pending
      ? {
          tone: "info",
          text: t("qd takes the link once the client runs as administrator."),
          details: [],
          kind: "needsElevation",
        }
      : notice(t("qd link added"));
  } catch (e) {
    toast.dismiss(id);
    throw e;
  }
}

/// Добавление источника как действие человека (D-160): одно меню на все кнопки «+», окна
/// добавления живут у `App`, и путь один, откуда бы ни нажали.
export function useAdding({ report, onAdded }: Deps) {
  /// Какое окно открыто. У файла окна нет — его спрашивает система.
  const [open, setOpen] = useState<"link" | "node" | "warp" | null>(null);

  /// Ссылка из любого поля: окна добавления и мастера. Отказ бросается — его показывает
  /// поле, у которого он случился.
  const link = useCallback(
    async (input: string) => {
      const message = await addLink(input);
      await onAdded();
      return message;
    },
    [onAdded],
  );

  /// Файл спрашивает система. `null` — окно выбора закрыли: это не отказ.
  const file = useCallback(async () => {
    const result = await api.sourcesAddFile();
    if (result === null) return null;
    await onAdded();
    return notice(api.importSummary(result), result.notices);
  }, [onAdded]);

  const pick = useCallback(
    (kind: AddKind) => {
      if (kind !== "file") {
        setOpen(kind);
        return;
      }
      file().then(
        (message) => message && report(message),
        (e) => report(failure(e)),
      );
    },
    [file, report],
  );

  /// Окно ссылки: удача закрывает окно и говорит итог баннером.
  const submit = useCallback(
    async (input: string) => {
      const message = await link(input);
      setOpen(null);
      report(message);
    },
    [link, report],
  );

  /// Узел собран формой (D-120) или выпущен WARP (D-165).
  const done = useCallback(
    async (result: api.Import) => {
      setOpen(null);
      await onAdded();
      report(notice(api.importSummary(result), result.notices));
    },
    [onAdded, report],
  );

  const close = useCallback(() => setOpen(null), []);

  return { open, pick, link, file, submit, done, close };
}
