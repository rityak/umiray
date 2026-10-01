// Мягкая остановка работает и из процесса **без своей консоли** (D-103, S-021).
//
//   npm run dev                      # окно грузится с localhost:1420 и в этой сборке тоже
//   npm run tauri build -- --no-bundle
//   node tools/soft-stop.mjs
//
// Зачем отдельно от `cargo test live`. Механизм проверен из тестового процесса, а у него
// консоль **есть**: `FreeConsole` там правда что-то освобождает. В релизе клиент —
// GUI-процесс, консоли у него нет вовсе, и вопрос «прицепится ли он к чужой» остаётся
// открытым ровно до этого прогона. Поэтому здесь берётся релизный бинарь: только у него
// стоит `windows_subsystem = "windows"`. Откуда он берёт разметку, роли не играет —
// проверяется не окно, а то, как процесс без консоли гасит ядро.
//
// Запускаем через `Start-Process`, а не из этой оболочки: GUI-процесс своей консоли
// не заводит, но **наследует родительскую**, если она есть. Запущенный из терминала,
// он получил бы её — и `FreeConsole` внутри `interrupt` освобождал бы настоящую консоль,
// то есть проверялся бы ровно тот случай, которого мы избегаем.
//
// Признак мягкого выхода — `run/cache.db`: ядро сохраняет карту подменных адресов только
// при штатном завершении. Чтобы зелёный не оказался ложным, рядом идёт **контрольный
// прогон**: то же ядро убивается `/F`, и файл обязан остаться нетронутым. Без контроля
// проверка доказывала бы лишь то, что файл вообще меняется.
//
// Инструмент сам поднимает и гасит окно; ядро запускается в local-режиме, режим
// в «Настройках» возвращается как был. Идти отказывается, если `mihomo.exe` уже работает.

import { execSync } from "node:child_process";
import { existsSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { attach } from "./cdp.mjs";

const PORT = Number(process.env.UI_CHECK_PORT ?? 9222);
const EXE = "src-tauri/target/release/umiray.exe";
// Каталог **релизной** сборки: проверка берёт релизный бинарь, а у него и каталог
// боевой — отладочный свой (D-116).
const CACHE = join(process.env.LOCALAPPDATA ?? "", "umiray", "run", "cache.db");
let failed = 0;

function check(name, ok, detail = "") {
  if (!ok) failed += 1;
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
}

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

const stamp = () => (existsSync(CACHE) ? statSync(CACHE).mtimeMs : 0);

/// Ждём состояние, а не «подольше»: сон на глазок либо тормозит, либо врёт.
async function until(want, what, timeout = 20000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await want()) return true;
    await new Promise((r) => setTimeout(r, 250));
  }
  console.error(`не дождались: ${what}`);
  return false;
}

const kill = (image) => {
  try {
    execSync(`taskkill /IM ${image} /F`, { stdio: "ignore" });
  } catch {
    // Уже не работает — это и требовалось.
  }
};

// --- поехали ---------------------------------------------------------------

if (!existsSync(EXE)) {
  console.error(`нет релизной сборки: ${EXE} — сначала \`npm run tauri build -- --no-bundle\``);
  process.exit(2);
}
if (pids("mihomo.exe").length > 0) {
  console.error("mihomo.exe уже работает: своё от чужого проверка не отличит");
  process.exit(2);
}

/// `--scheduled` обязателен: на машине с заведённой задачей (D-087) запуск без него
/// уходит в повышенный процесс, а туда отладочный порт не доезжает (GOTCHAS).
execSync(
  `powershell -NoProfile -NonInteractive -Command "$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=${PORT}'; Start-Process -FilePath '${resolve(EXE)}' -ArgumentList '--scheduled'"`,
  { stdio: "ignore" },
);

// Ждём отладочный порт, а не появление процесса: вебвью отдаёт его позже, чем
// Windows заводит сам процесс.
const listening = () =>
  fetch(`http://127.0.0.1:${PORT}/json/list`).then(
    () => true,
    () => false,
  );
if (!(await until(listening, "отладочный порт релизного окна", 40000))) {
  kill("umiray.exe");
  process.exit(2);
}

const session = await attach({ port: PORT, reload: false });
if (!(await session.until("document.querySelector('header') !== null"))) {
  console.error("окно не нарисовалось: dev-сервер запущен? (`npm run dev`)");
  kill("umiray.exe");
  process.exit(2);
}

const wasMode = await session.eval(
  `return document.querySelector('input[name="mode"]:checked')?.value ?? null;`,
);
/// Local, а не TUN: релизное окно поднято без прав, и перехват просто не встанет.
/// Клик пишет файл, и ждать надо записи, а не отрисовки: нажать питание раньше —
/// значит поднимать TUN без прав и получить отказ вместо проверки.
await session.eval(`
  const local = [...document.querySelectorAll('input[name="mode"]')].find((i) => i.value === "local");
  local?.click();
  return true;
`);
if (
  !(await session.until(
    `(await window.__TAURI_INTERNALS__.invoke("core_status")).desiredMode === "local"`,
    { timeout: 10000 },
  ))
) {
  console.error("режим не переключился в local");
  kill("umiray.exe");
  process.exit(2);
}

const power = "header button[aria-pressed]";

async function connect() {
  // Ждём, пока кнопка снова готова: сразу после остановки окно ещё в «Отключении»,
  // и нажатие туда не доходит вовсе.
  await session.until(
    `(() => { const b = document.querySelector(${JSON.stringify(power)}); return b && !b.disabled && b.getAttribute("aria-pressed") === "false"; })()`,
    { timeout: 20000 },
  );
  await session.click(power);
  if (
    !(await session.until(`document.body.textContent.includes("Подключён")`, { timeout: 30000 }))
  ) {
    console.error("ядро не поднялось");
    kill("umiray.exe");
    kill("mihomo.exe");
    process.exit(2);
  }
  return pids("mihomo.exe")[0];
}

// --- мягкая остановка -------------------------------------------------------

const core = await connect();
console.log(`ядро поднялось, pid ${core}`);
const before = stamp();
// Секунда, чтобы отметка времени точно отличалась: у NTFS она грубее миллисекунды.
await new Promise((r) => setTimeout(r, 1200));

await session.click(power);
const stopped = await until(() => !pids("mihomo.exe").includes(core), "ядро вышло");
check("ядро погасло по нажатию", stopped);
check(
  "карта подменных адресов сохранена — значит выход был штатным",
  stamp() > before,
  `cache.db ${new Date(stamp()).toISOString()}`,
);

// --- контрольный прогон: то же самое, но убийством ---------------------------

const again = await connect();
console.log(`ядро поднялось заново, pid ${again}`);
const beforeKill = stamp();
await new Promise((r) => setTimeout(r, 1200));
kill("mihomo.exe");
await until(() => !pids("mihomo.exe").includes(again), "убитое ядро исчезло");
check(
  "контроль: убитое `/F` карту не сохраняет",
  stamp() === beforeKill,
  "иначе признак ничего не доказывает",
);

// --- прибираемся ------------------------------------------------------------

if (wasMode) {
  await session.eval(`
    const back = [...document.querySelectorAll('input[name="mode"]')].find((i) => i.value === ${JSON.stringify(wasMode)});
    back?.click();
    return true;
  `);
  await new Promise((r) => setTimeout(r, 500));
}
kill("umiray.exe");
kill("mihomo.exe");

console.log(
  failed === 0 ? "\nмягкая остановка работает и без своей консоли" : `\n${failed} провалов`,
);
process.exit(failed === 0 ? 0 : 1);
