import { useState } from "react";

/// Чем окно занято, а не просто «занято»: одного флага мало — от него зависит и что
/// заблокировать, и что написать на кнопке, а это разное. Из-за общего флага сброс
/// подписывался «Скачивание…».
export type Job = "power" | "mode" | "install" | "reset" | "update";

export function useJob() {
  const [job, setJob] = useState<Job | null>(null);
  return { job, setJob, busy: job !== null };
}
