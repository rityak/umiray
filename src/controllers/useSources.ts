import { useCallback, useState } from "react";
import * as api from "../api";

/// Список источников (D-032). Перечитывает его владелец списка, а не раздел, который нажал.
export function useSources() {
  const [sources, setSources] = useState<api.Source[]>([]);
  // Стабильная ссылка: её держат в зависимостях обработчики разделов.
  const reload = useCallback(() => api.sourcesList().then(setSources, () => {}), []);
  return { sources, setSources, reload };
}
