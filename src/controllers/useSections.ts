import { useCallback, useState } from "react";
import * as api from "../api";
import { unchanged, usePoll } from "../hooks/usePoll";

/// Разделы конфига и документы в них (D-044, D-070). Состав меняется от действий
/// пользователя — завели набор, применили другой (D-071), — поэтому, пока открыт раздел
/// конфига, список опрашивается.
export function useSections(tab: string) {
  const [sections, setSections] = useState<api.ConfigSection[]>([]);
  const reload = useCallback(() => {
    api.configList().then(unchanged(setSections), () => {});
  }, []);
  usePoll(
    reload,
    sections.some((section) => section.id === tab),
  );
  return { sections, setSections, reload };
}
