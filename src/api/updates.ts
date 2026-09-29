/// Обновления самого клиента (D-149).

import { Channel } from "@tauri-apps/api/core";
import { z } from "zod";
import { call, done } from "./call";

export const UpdateInfo = z.object({
  enabled: z.boolean(),
  version: z.string().nullable(),
  notes: z.string().nullable(),
});
export type UpdateInfo = z.infer<typeof UpdateInfo>;
export const UpdateProgress = z.object({
  phase: z.enum(["download", "install"]),
  downloaded: z.number().nonnegative(),
  total: z.number().nonnegative().nullable(),
});
export type UpdateProgress = z.infer<typeof UpdateProgress>;
export const updatesCheck = () => call(UpdateInfo, "updates_check");
export const updatesInstall = (onProgress: (progress: UpdateProgress) => void) => {
  const progress = new Channel<unknown>();
  progress.onmessage = (raw) => {
    const parsed = UpdateProgress.safeParse(raw);
    if (parsed.success) onProgress(parsed.data);
  };
  return call(done, "updates_install", { progress });
};
