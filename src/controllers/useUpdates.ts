import { useEffect, useState } from "react";
import * as api from "../api";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import type { Job } from "./useJob";

type Deps = {
  job: Job | null;
  setJob: (job: Job | null) => void;
  /// Несохранённое где угодно: обновление перезапустит окно, и оно пропало бы.
  unsaved: () => boolean;
  report: (message: Message | null) => void;
  setStatus: (status: api.Status) => void;
};

/// Обновления самого клиента (D-149): проверить, скачать, поставить.
export function useUpdates({ job, setJob, unsaved, report, setStatus }: Deps) {
  const [info, setInfo] = useState<api.UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [open, setOpen] = useState(false);
  const [progress, setProgress] = useState<api.UpdateProgress | null>(null);

  useEffect(() => {
    if (job !== "update") return;
    // Removing the confirm button moves focus outside the dialog; capture Escape on the window.
    const preventEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") event.preventDefault();
    };
    window.addEventListener("keydown", preventEscape, true);
    return () => window.removeEventListener("keydown", preventEscape, true);
  }, [job]);

  const check = async () => {
    setChecking(true);
    try {
      setInfo(await api.updatesCheck());
    } catch (error) {
      report(failure(error));
    } finally {
      setChecking(false);
    }
  };

  const install = async () => {
    if (job !== null) return;
    if (unsaved()) {
      report(notice(t("Save or discard your edits before updating the client.")));
      return;
    }
    setJob("update");
    report(null);
    setProgress({ phase: "download", downloaded: 0, total: null });
    try {
      await api.updatesInstall(setProgress);
    } catch (error) {
      report(failure(error));
    } finally {
      setJob(null);
      setProgress(null);
      api.coreStatus().then(setStatus, () => {});
    }
  };

  return { info, setInfo, checking, open, setOpen, progress, check, install };
}
