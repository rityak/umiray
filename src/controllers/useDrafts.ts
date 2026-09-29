import { useCallback, useState } from "react";
import { type Drafts, dirty, fromDisk } from "../config/draft";

/// Черновики (D-040, D-138): документов конфига и сырья источников. Живут здесь, а не
/// в редакторах: уход в другой раздел размонтирует редактор, и несохранённое пропало бы
/// вместе с ним.
export function useDrafts() {
  const [configs, setConfigs] = useState<Drafts>({});
  // Черновик переживает смену источника и раздела; private не читает raw вовсе (D-138).
  const [sources, setSources] = useState<Drafts>({});

  const onDraft = useCallback((id: string, text: string) => {
    setConfigs((current) => ({ ...current, [id]: { ...current[id], text } }));
  }, []);

  /// Файл прочитан с диска — правила слияния с черновиком живут в `fromDisk`.
  const onDisk = useCallback((id: string, text: string) => {
    setConfigs((current) => ({ ...current, [id]: fromDisk(current[id], text) }));
  }, []);

  const onSourceDraft = useCallback((id: string, text: string) => {
    setSources((current) => ({ ...current, [id]: { ...current[id], text } }));
  }, []);

  const onSourceDisk = useCallback((id: string, text: string) => {
    setSources((current) => ({ ...current, [id]: fromDisk(current[id], text) }));
  }, []);

  /// После сброса в окне не должно остаться ничего от прошлой жизни.
  const clear = useCallback(() => {
    setConfigs({});
    setSources({});
  }, []);

  /// Есть ли несохранённое хоть где-то: обновление клиента его бы потеряло.
  const unsaved = Object.values(configs).some(dirty) || Object.values(sources).some(dirty);

  return {
    configs,
    sources,
    onDraft,
    onDisk,
    onSourceDraft,
    onSourceDisk,
    clear,
    unsaved,
  };
}
