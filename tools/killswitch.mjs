// Что происходит с трафиком, когда ядро умирает при поднятом TUN (S-015).
//
//   node tools/killswitch.mjs
//   ...и убить `mihomo-dev.exe` (диспетчер задач → снять задачу). Именно ядро, не приложение:
//   выход через окно — это штатная остановка, она давно проверена.
//
// Вопрос ровно один и он про безопасность: адаптер исчезает вместе с процессом, маршрут
// по умолчанию возвращается на физическую карту — и трафик может **пойти в открытую**,
// пока пользователь считает себя защищённым.
//
// Договариваться «на счёт три» бессмысленно, поэтому наблюдатель ждёт смерти сам и с этого
// мгновения меряет: есть ли связь, какой адрес отдаёт мир, остался ли адаптер.

import { execSync } from "node:child_process";

/// Адрес без VPN. Всё сравнение — с ним: совпал после смерти ядра, значит утечка.
const HOME = process.argv[2] ?? "5.167.5.45";
const WINDOW_MS = 30000;

const alive = () => {
  try {
    return execSync('tasklist /FI "IMAGENAME eq mihomo-dev.exe" /NH', {
      encoding: "latin1",
    }).includes("mihomo");
  } catch {
    return false;
  }
};

const ps = (script) => {
  try {
    return execSync(`powershell -NoProfile -Command "${script}"`, { encoding: "utf8" }).trim();
  } catch {
    return "";
  }
};

const adapter = () => ps("(Get-NetAdapter -Name Meta -ErrorAction SilentlyContinue).Status");
const routes = () =>
  ps(
    "(Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue | Sort-Object RouteMetric | ForEach-Object { $_.InterfaceAlias }) -join ','",
  );

/// Внешний адрес — или причина, по которой его нет. Короткий таймаут: «связи нет» здесь
/// такой же ответ, как адрес, и ждать его 30 секунд незачем.
async function ip() {
  const control = AbortSignal.timeout(3000);
  try {
    const answer = await fetch("https://api.ipify.org", { signal: control });
    return await answer.text();
  } catch (e) {
    return `нет связи (${e.name})`;
  }
}

const stamp = (t0) => `+${((Date.now() - t0) / 1000).toFixed(1)} с`;

console.log(`домашний адрес для сравнения: ${HOME}`);
console.log(`адаптер сейчас: ${adapter() || "нет"} · маршруты: ${routes()}`);
if (!alive()) {
  console.error("ядро не запущено — сначала поднимите TUN");
  process.exit(2);
}
console.log(`через TUN сейчас: ${await ip()}`);
console.log("\nжду смерти ядра — убейте mihomo-dev.exe в диспетчере задач…");

while (alive()) await new Promise((r) => setTimeout(r, 200));

const t0 = Date.now();
console.log(`\nядро умерло. адаптер: ${adapter() || "исчез"} · маршруты: ${routes()}`);

const seen = [];
while (Date.now() - t0 < WINDOW_MS) {
  const at = stamp(t0);
  const answer = await ip();
  console.log(`  ${at.padStart(9)} — ${answer}`);
  seen.push(answer);
  await new Promise((r) => setTimeout(r, 1500));
}

const leaked = seen.some((x) => x === HOME);
const silent = seen.every((x) => x.startsWith("нет связи"));
console.log(
  `\nадаптер: ${adapter() || "исчез"} · маршруты: ${routes()}\n` +
    (leaked
      ? "ВЕРДИКТ: трафик ушёл в открытую — мир увидел домашний адрес. Нужен kill switch."
      : silent
        ? "ВЕРДИКТ: связи нет — трафик встал, наружу ничего не утекло."
        : `ВЕРДИКТ: ни то ни другое, смотреть глазами: ${[...new Set(seen)].join(" | ")}`),
);
