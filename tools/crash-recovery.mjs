// Падение никого не оставляет висеть: ядро упало — клиент поднял его заново (D-057);
// клиент упал — ядро умерло с ним (D-058); ядро всё-таки осиротело — клиент прибрал
// его при запуске (D-059).
//
//   npm run dev                      # dev-сборка грузится с localhost:1420
//   node tools/crash-recovery.mjs
//
// Инструмент **сам поднимает и убивает окно**: падение клиента иначе не проверить, а
// `taskkill /F` — единственный способ не дать ему прибраться по-человечески. Поэтому он
// отдельный, а не флаг в `ui-check`: тот смотрит в уже открытое окно и ничего не убивает.
//
// Проверка **отказывается идти, если `mihomo-dev.exe` уже запущен**: своё она убивает по имени
// файла (`taskkill /IM`) и отличить чужое ядро от нашего осиротевшего не может. Клиент
// ищет сироту по полному пути бинаря (D-154) — проверке эта точность не нужна.
//
// Побочные эффекты: ядро запускается в local-режиме и гасится, режим в «Настройках»
// возвращается как был. Осиротевшее ядро для проверки поднимается из того же файла, что
// у клиента, — иначе уборка по пути его не увидела бы, — но в отдельном каталоге и без
// единого слушающего порта: чужого трафика оно не видит.

import { execSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { attach } from "./cdp.mjs";

const PORT = Number(process.env.UI_CHECK_PORT ?? 9222);
const EXE = "src-tauri/target/debug/umiray-dev.exe";
// Каталог **отладочной** сборки: `EXE` выше — она же (D-116).
const CORE = join(process.env.LOCALAPPDATA ?? "", "umiray-dev", "mihomo-dev.exe");
/// Каталог для подставного ядра: свой, чтобы не путаться с рабочим `run/` клиента.
const ORPHANAGE = join(tmpdir(), "umiray-orphan-check");
let failed = 0;

function check(name, ok, detail = "") {
  if (!ok) failed += 1;
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
}

/// Список PID процесса по имени. CSV, а не `/NH`: разбор по столбцам не зависит от того,
/// на каком языке Windows и сколько там пробелов.
function pids(image) {
  try {
    return execSync(`tasklist /FI "IMAGENAME eq ${image}" /FO CSV /NH`, { encoding: "latin1" })
      .split("\n")
      .map((line) => /^"[^"]+","(\d+)"/.exec(line.trim())?.[1])
      .filter(Boolean);
  } catch {
    return [];
  }
}

const kill = (image) => {
  try {
    execSync(`taskkill /IM ${image} /F`, { stdio: "ignore" });
  } catch {
    // Уже не работает — для нашей цели это то же самое.
  }
};

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

/// Ждём состояние, а не «подольше»: сон на глазок либо тормозит, либо врёт.
async function until(ok, what, ms = 25000) {
  for (let waited = 0; waited < ms; waited += 500) {
    if (await ok()) return true;
    await wait(500);
  }
  console.error(`не дождались: ${what}`);
  return false;
}

const windowUp = () =>
  fetch(`http://127.0.0.1:${PORT}/json/list`).then(
    () => true,
    () => false,
  );

/// `--scheduled` обязателен: на машине с заведённой задачей (D-087) запуск без него
/// уходит в повышенный процесс, а туда отладочный порт не доезжает (GOTCHAS).
function launch() {
  spawn(EXE, ["--scheduled"], {
    detached: true,
    stdio: "ignore",
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
    },
  }).unref();
}

/// Ядро, которое никому не подчиняется, — то самое, что остаётся после падения клиента
/// без клетки. Слушать ему нечего: порты выключены, API выключено.
function orphan() {
  mkdirSync(ORPHANAGE, { recursive: true });
  writeFileSync(
    join(ORPHANAGE, "config.yaml"),
    "mixed-port: 0\nexternal-controller: ''\nlog-level: silent\n",
  );
  spawn(CORE, ["-d", ORPHANAGE], { detached: true, stdio: "ignore" }).unref();
}

const invoke = (session, command, args = {}) =>
  session.eval(
    `return await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)});`,
  );

/// Режим читаем из файла: он и есть владелец поля (D-052).
async function fileMode(session) {
  const text = await invoke(session, "config_read", { id: "advanced" });
  return /tun:[\s\S]*?enable:\s*(true|false)/.exec(text)?.[1] ?? null;
}

// --- поехали ---------------------------------------------------------------

if (
  !(await fetch("http://localhost:1420/").then(
    () => true,
    () => false,
  ))
) {
  console.error("vite не отвечает на 1420: сначала `npm run dev`");
  process.exit(2);
}
if (!existsSync(CORE)) {
  console.error(`ядра нет на диске (${CORE}): скачайте его в настройках, проверять нечего`);
  process.exit(2);
}
if (pids("mihomo-dev.exe").length > 0) {
  console.error("mihomo-dev.exe уже запущен: он может быть чужой, а проверка убивает по имени");
  process.exit(2);
}

kill("umiray-dev.exe");
await until(async () => !(await windowUp()), "прежнее окно закрылось", 10000);

// --- осиротевшее ядро прибирается при запуске (D-059) -----------------------

orphan();
await wait(1500);
check("подставное ядро живо", pids("mihomo-dev.exe").length === 1, "иначе проверять нечего");

launch();
if (!(await until(windowUp, "окно с отладочным портом"))) process.exit(2);
check(
  "клиент прибрал осиротевшее ядро при запуске",
  await until(() => pids("mihomo-dev.exe").length === 0, "ядро исчезло", 10000),
);
rmSync(ORPHANAGE, { recursive: true, force: true });

const session = await attach({ port: PORT });
const was = await fileMode(session);
console.log(`режим в файле до проверки: tun.enable ${was}`);

// --- ядро падает ------------------------------------------------------------

await invoke(session, "mode_set", { mode: "local" });
await invoke(session, "core_start");
const before = pids("mihomo-dev.exe");
check("ядро запущено", before.length === 1, `pid ${before[0]}`);

kill("mihomo-dev.exe");
// Ждём **новый pid**, а не флаг `running`. Статус после `taskkill` отстаёт: убитый процесс
// ещё не пожат, `try_wait` докладывает «жив», и первый же опрос — сразу после убийства —
// возвращает `true`. Проверка на флаг ловила бы собственную гонку, а не подъём.
const back = await until(() => {
  const now = pids("mihomo-dev.exe");
  return now.length === 1 && now[0] !== before[0];
}, "ядро вернулось после падения");
// Новый pid появляется **раньше**, чем подъём состоялся: супервизор ждёт от ядра ответа
// на `/version` и только потом признаёт запуск, а журнал фазы и строку надзора пишет
// и вовсе после. Без этого ожидания следующие две проверки читают недописанное.
await until(
  async () => (await invoke(session, "core_status")).running,
  "ядро доложилось готовым",
  15000,
);
const after = pids("mihomo-dev.exe");
check("ядро поднялось само после падения", back && after.length === 1, `pid ${after[0]}`);
check("это новый процесс, а не тот же", after[0] !== before[0], `${before[0]} → ${after[0]}`);
check(
  "поднялось в том же режиме",
  (await invoke(session, "core_status")).mode === "local",
  String((await invoke(session, "core_status")).mode),
);
const logs = await invoke(session, "core_logs", { engine: "mihomo" });
check(
  "в логе видно, что произошло",
  logs.some((line) => line.includes("umiray: ядро упало")),
  logs.find((line) => line.includes("umiray:")) ?? "своей строки нет",
);

// Уборка бьёт по пути бинаря, а работающее ядро запущено из того же файла, поэтому вторая
// копия обязана гаснуть **до** неё: иначе запуск ярлыка второй раз убивал бы ядро
// работающего клиента (D-046 + D-059).
launch();
await wait(4000);
check("вторая копия не тронула работающее ядро", pids("mihomo-dev.exe").length === 1);
check("и не осталась висеть сама", pids("umiray-dev.exe").length === 1);

// Отрицательный контроль: без него «поднимается всегда» прошло бы проверку наравне
// с «поднимается после падения», а это разные вещи — нажатая кнопка питания обязана выключать.
await invoke(session, "core_stop");
await wait(4000);
check("штатное выключение ядро не воскрешает", pids("mihomo-dev.exe").length === 0);

// Файл чужой — возвращаем как было, пока окно ещё живо.
await invoke(session, "mode_set", { mode: was === "true" ? "tun" : "local" });
check("режим возвращён как был", (await fileMode(session)) === was, `tun.enable ${was}`);

// --- падает клиент ----------------------------------------------------------

await invoke(session, "core_start");
check("ядро снова запущено", pids("mihomo-dev.exe").length === 1);

// Именно /F: штатный выход гасит ядро сам, и проверял бы он не клетку, а обработчик.
kill("umiray-dev.exe");
const gone = await until(
  async () => pids("umiray-dev.exe").length === 0 && pids("mihomo-dev.exe").length === 0,
  "ядро ушло следом за клиентом",
  10000,
);
check("падение клиента убило ядро", gone, gone ? "" : `осталось: ${pids("mihomo-dev.exe")}`);
if (!gone) kill("mihomo-dev.exe");

console.log(failed ? `\nпровалено проверок: ${failed}` : "\nвсе проверки прошли");
process.exit(failed ? 1 : 0);
