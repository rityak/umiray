// Режим пишется кликом и переживает перезапуск приложения.
//
//   npm run dev                 # vite должен работать: dev-сборка грузится с localhost:1420
//   node tools/mode-restart.mjs
//
// Этот инструмент **сам поднимает и гасит окно** — иначе перезапуск не проверить. Поэтому
// он отдельный, а не флаг в `ui-check`: тот только смотрит в уже открытое окно.
//
// Что здесь проверяется. Режим — поле конфига ядра (D-052), а переключатель в шапке его
// правит и **ядро не трогает** (D-060). Значит клик обязан записать файл сразу, без всякого
// запуска, а после перезапуска приложения переключатель обязан показывать **тот же режим**:
// он читается из файла, а не из работающего процесса.
//
// Побочных эффектов нет: ядро здесь не поднимается вовсе. TUN проверяется кликом при любых
// правах — запись в файл перехвата не включает.

import { execSync, spawn } from "node:child_process";
import { attach } from "./cdp.mjs";

const PORT = Number(process.env.UI_CHECK_PORT ?? 9222);
const EXE = "src-tauri/target/debug/umiray-dev.exe";
let failed = 0;

function check(name, ok, detail = "") {
  if (!ok) failed += 1;
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
}

const alive = () =>
  fetch(`http://127.0.0.1:${PORT}/json/list`).then(
    () => true,
    () => false,
  );

/// Ждём состояние порта, а не «подольше»: сон на глазок либо тормозит, либо врёт.
async function until(want, what) {
  for (let i = 0; i < 40; i += 1) {
    if ((await alive()) === want) return true;
    await new Promise((r) => setTimeout(r, 500));
  }
  console.error(`не дождались: ${what}`);
  process.exit(2);
}

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

const kill = () => {
  try {
    execSync("taskkill /IM umiray-dev.exe /F", { stdio: "ignore" });
  } catch {
    // Уже не работает — это и требовалось.
  }
};

/// Режим читаем из самого файла: редактор рисует только видимые строки, а сравнивать
/// документы целиком нельзя — запись пересобирает YAML и меняет форматирование.
async function fileMode(session) {
  const text = await session.eval(
    `return await window.__TAURI_INTERNALS__.invoke("config_read", { id: "advanced" });`,
  );
  return /tun:[\s\S]*?enable:\s*(true|false)/.exec(text)?.[1] ?? null;
}

const pick = (session, mode) =>
  session.eval(`
    const input = [...document.querySelectorAll('input[name="mode"]')].find((i) => i.value === ${JSON.stringify(mode)});
    if (!input) return false;
    input.click();
    return true;
  `);

const shown = (session) =>
  session.eval(`return document.querySelector('input[name="mode"]:checked')?.value ?? null;`);

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

if (!(await alive())) {
  launch();
  await until(true, "окно с отладочным портом");
}

let session = await attach({ port: PORT });
const was = await fileMode(session);
console.log(`режим в файле до проверки: tun.enable ${was}`);

const elevated = await session.eval(`
  return await window.__TAURI_INTERNALS__.invoke("core_status").then((s) => s.elevated);
`);

// --- клик пишет файл --------------------------------------------------------

check("переключатель нашёлся", (await pick(session, "local")) === true);
await session.until(
  `(await window.__TAURI_INTERNALS__.invoke("config_read", { id: "advanced" })).includes("enable: false")`,
  { timeout: 10000 },
);
check(
  "клик «Proxy» записал режим в файл",
  (await fileMode(session)) === "false",
  "tun.enable false",
);
check(
  "и не поднял ядро: режим — не подключение (D-060)",
  (await session.eval(`return document.body.textContent.includes("Отключено");`)) === true,
);

await pick(session, "tun");
await session.until(
  `(await window.__TAURI_INTERNALS__.invoke("config_read", { id: "advanced" })).includes("enable: true")`,
  { timeout: 10000 },
);
check("клик «TUN» записал режим в файл", (await fileMode(session)) === "true");
check(
  "TUN тоже не поднимает ядро",
  (await session.eval(`return document.body.textContent.includes("Отключено");`)) === true,
  elevated ? "окно от администратора — тем более важно" : "прав нет, и они не понадобились",
);

// --- перезапуск -------------------------------------------------------------

kill();
await until(false, "окно закрылось");
launch();
await until(true, "окно поднялось снова");
session = await attach({ port: PORT });

check("режим пережил перезапуск", (await fileMode(session)) === "true", "tun.enable true");
// Переключатель читает файл, а не процесс (D-060): ядро не работает, а выбранный режим тот же.
check(
  "после перезапуска переключатель в TUN",
  (await shown(session)) === "tun",
  String(await shown(session)),
);
check(
  "ядро при этом не поднято",
  (await session.eval(`return document.body.textContent.includes("Отключено");`)) === true,
);
check(
  "окно поднялось без ошибки",
  (await session.eval(`return document.querySelector('[role="alert"]') === null;`)) === true,
);

// Файл чужой — возвращаем как было.
await session.eval(`
  await window.__TAURI_INTERNALS__.invoke("mode_set", { mode: ${JSON.stringify(was === "true" ? "tun" : "local")} });
  return 1;
`);
check("режим возвращён как был", (await fileMode(session)) === was, `tun.enable ${was}`);

console.log(failed ? `\nпровалено проверок: ${failed}` : "\nвсе проверки прошли");
process.exit(failed ? 1 : 0);
