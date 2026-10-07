/// Документ «Клиент» (D-068): замеры, страны узлов, маскировка WireGuard.

import { z } from "zod";
import { call, done } from "./call";
import { Status } from "./core";
import { PingMethod } from "./nodes";

/// Чем мерить задержку (D-069). Живёт в `client.yaml`, а не в настройках: это документ,
/// который правится и руками тоже (D-068).
/// Через сколько часов перепрашивать страну узла; 0 — не спрашивать (D-084).
export const clientGeoGet = () => call(z.number(), "client_geo_get");
export const clientGeoSet = (hours: number) => call(done, "client_geo_set", { hours });

/// Чем маскировать рукопожатие WireGuard (D-118). Ноль в поле — «не трогать»: у самого
/// AmneziaWG отдельного флага нет, выключенное состояние и есть ноль.
///
/// Два яруса, и разница между ними не косметическая: `jc/jmin/jmax` работают с **любым**
/// сервером WireGuard, а `s*` и `h*` меняют формат пакетов и требуют сервера с AmneziaWG.
export const Mask = z.object({
  jc: z.number(),
  jmin: z.number(),
  jmax: z.number(),
  s1: z.number(),
  s2: z.number(),
  s3: z.number(),
  s4: z.number(),
  h1: z.number(),
  h2: z.number(),
  h3: z.number(),
  h4: z.number(),
});
export type Mask = z.infer<typeof Mask>;
export const MASK_FIELDS = [
  "jc",
  "jmin",
  "jmax",
  "s1",
  "s2",
  "s3",
  "s4",
  "h1",
  "h2",
  "h3",
  "h4",
] as const;
export const clientMaskGet = () => call(Mask, "client_mask_get");
export const clientMaskSet = (mask: Mask) => call(Mask, "client_mask_set", { mask });

export const clientPingGet = () => call(PingMethod, "client_ping_get");
export const clientPingSet = (method: PingMethod) => call(done, "client_ping_set", { method });

/// Куда бьёт проверка живости — одна цель на ядро и на замер клиента (D-108).
export const clientHealthGet = () => call(z.string(), "client_health_get");
export const clientHealthSet = (url: string) => call(done, "client_health_set", { url });
/// Как часто группы перепроверяют узлы, секунд (AUTO, автогруппы, источники).
export const clientHealthIntervalGet = () => call(z.number(), "client_health_interval_get");
export const clientHealthIntervalSet = (seconds: number) =>
  call(Status, "client_health_interval_set", { seconds });

/// Готовые цели. Все трое отдают 204 без тела; какую из них не режут в конкретной
/// стране, знает только пользователь — оттого это список, а не константа. Свою можно
/// вписать в документ «Клиент» кодом: форма покажет её отдельной строкой.
export const HEALTH_TARGETS: { url: string; label: string }[] = [
  { url: "http://cp.cloudflare.com/generate_204", label: "Cloudflare" },
  { url: "http://www.gstatic.com/generate_204", label: "Google (gstatic)" },
  { url: "http://www.google.com/generate_204", label: "Google" },
];
