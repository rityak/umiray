/// Подбор настроек мастером (D-105, D-162): замер плюс одна запись в «Настройки mihomo».

import { z } from "zod";
import { call } from "./call";

export const Verdict = z.enum(["ok", "warn", "bad", "idle"]);
export type Verdict = z.infer<typeof Verdict>;

export const Report = z.object({
  /// Что подбирали: `dns-race` или `pmtu`.
  tool: z.string(),
  verdict: Verdict,
  /// Что записано — одной строкой для человека.
  headline: z.string(),
});
export type Report = z.infer<typeof Report>;

export const Tuning = z.enum(["recommended", "dns-race", "pmtu"]);
export type Tuning = z.infer<typeof Tuning>;

/// Из каких DNS выбирает подбор (S-033): чистые, режущие рекламу или любые.
export const DnsFilter = z.enum(["clean", "ads", "any"]);
export type DnsFilter = z.infer<typeof DnsFilter>;

/// Подобрать и записать. Запись доезжает до живого ядра; `dnsFilter` нужен только `dns-race`.
export const diagApply = (id: Tuning, dnsFilter?: DnsFilter) =>
  call(Report, "diag_apply", { id, dnsFilter });
