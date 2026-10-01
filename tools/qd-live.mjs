// qd за контрактом ядра, живьём (D-154, D-057–D-059): туннель поднят к ответу `connect`,
// убитый процесс поднимается, осиротевший прибирается по пути, потерянный туннель живого
// qd клиент не трогает — его возвращает сам qd.
//
//   npm run dev
//   tools\as-admin.cmd tools\qd-live.mjs      # qd без прав не встаёт; UAC — человек
//
// Нужны `qd-dev.exe` и подписка в `%LOCALAPPDATA%\umiray-dev\qd` (можно скопировать
// `qd.exe` и `qd\client.db*` установленного клиента: устройство qd — хэш машины, нового
// слота в подписке копия не займёт).
//
// Побочные эффекты: окно поднимается и убивается, ядро в настройках возвращается как было.
// qd на 80 с замораживается — ради «процесс стоял, туннель потерян» (как после сна); что
// qd перехватывает, это время стоит.

import { execSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { attach } from "./cdp.mjs";

const PORT = Number(process.env.UI_CHECK_PORT ?? 9222);
const EXE = "src-tauri/target/debug/umiray-dev.exe";
const QD = join(process.env.LOCALAPPDATA ?? "", "umiray-dev", "qd-dev.exe");
const ORPHANAGE = join(tmpdir(), "umiray-qd-orphan");
/// Дольше `goneFor` у qd (75 с): после такой паузы он сам признаёт туннель потерянным.
const FROZEN_MS = 80000;
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

const kill = (image) => {
  try {
    execSync(`taskkill /IM ${image} /F`, { stdio: "ignore" });
  } catch {
    // Уже не работает — для нашей цели это то же самое.
  }
};

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

async function until(ok, what, ms = 30000) {
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

/// Заморозить или отпустить процесс целиком — так его видит узел, пока машина спит.
function freeze(pid, on) {
  const call = on ? "NtSuspendProcess" : "NtResumeProcess";
  const script =
    `$t = Add-Type -Name Nt -Namespace W -PassThru -MemberDefinition '[DllImport("ntdll.dll")] public static extern int ${call}(IntPtr h);';` +
    `[void]$t::${call}((Get-Process -Id ${pid}).Handle)`;
  const encoded = Buffer.from(script, "utf16le").toString("base64");
  execSync(`powershell -NoProfile -EncodedCommand ${encoded}`, { stdio: "ignore" });
}

const invoke = (session, command, args = {}) =>
  session.eval(
    `return await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)});`,
  );

const active = async (session) => (await invoke(session, "core_status")).active;

// --- поехали ---------------------------------------------------------------

try {
  execSync("net session", { stdio: "ignore" });
} catch {
  console.error("нужны права администратора: tools\\as-admin.cmd tools\\qd-live.mjs");
  process.exit(2);
}
if (
  !(await fetch("http://localhost:1420/").then(
    () => true,
    () => false,
  ))
) {
  console.error("vite не отвечает на 1420: сначала `npm run dev`");
  process.exit(2);
}
if (!existsSync(QD)) {
  console.error(`qd нет на диске (${QD})`);
  process.exit(2);
}
if (pids("qd-dev.exe").length > 0 || pids("qd.exe").length > 0) {
  console.error(
    "qd уже запущен: второй клиент qd на машине не встанет, а проверка убивает по имени",
  );
  process.exit(2);
}

kill("umiray-dev.exe");
await until(async () => !(await windowUp()), "прежнее окно закрылось", 10000);

// --- осиротевший qd прибирается при запуске (D-059) -------------------------

// stdin держим открытым: `-embedded` выходит, когда его закрывают.
mkdirSync(ORPHANAGE, { recursive: true });
const orphan = spawn(QD, ["-embedded", "-ui-port", "0", "-state", join(ORPHANAGE, "client.db")], {
  stdio: ["pipe", "ignore", "ignore"],
});
orphan.on("error", () => {});
await wait(2000);
check("подставной qd жив", pids("qd-dev.exe").length === 1, "иначе проверять нечего");

launch();
if (!(await until(windowUp, "окно с отладочным портом"))) {
  kill("umiray-dev.exe");
  process.exit(2);
}
check(
  "клиент прибрал осиротевший qd при запуске",
  await until(() => pids("qd-dev.exe").length === 0, "qd исчез", 10000),
);
// Убитый qd отпускает свою базу не сразу.
rmSync(ORPHANAGE, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 });

const session = await attach({ port: PORT });
const was = (await invoke(session, "settings_get")).engine;
await invoke(session, "settings_update", { patch: { engine: "qd" } });

try {
  // --- `connect` отвечает поднятым туннелем ---------------------------------

  const started = await invoke(session, "core_start");
  check("ответ на запуск — уже работающий qd", started.active === "qd", `active ${started.active}`);
  const said = await invoke(session, "qd_status");
  check(
    "qd сам говорит connected",
    said.state?.connected === true,
    JSON.stringify(said.state?.node?.name),
  );
  const before = pids("qd-dev.exe");

  // --- убитый процесс поднимается (D-057) -----------------------------------

  kill("qd-dev.exe");
  const back = await until(() => {
    const now = pids("qd-dev.exe");
    return now.length === 1 && now[0] !== before[0];
  }, "qd вернулся после падения");
  await until(async () => (await active(session)) === "qd", "qd доложился подключённым", 30000);
  check("qd поднялся сам после taskkill /F", back && (await active(session)) === "qd");
  const logs = await invoke(session, "core_logs", { engine: "qd" });
  check(
    "в логе видно, что произошло",
    logs.some((line) => line.includes("ядро упало — подъём 1/")),
    logs.find((line) => line.includes("umiray:")) ?? "своей строки нет",
  );

  // --- туннель потерян, процесс жив: это дело qd -----------------------------

  const [pid] = pids("qd-dev.exe");
  const meddling = (lines) => lines.filter((line) => /ядро упало|автоподъём остановлен/.test(line));
  const known = meddling(logs).length;
  console.log(`замораживаю qd (pid ${pid}) на ${FROZEN_MS / 1000} с…`);
  freeze(pid, true);
  try {
    await wait(FROZEN_MS);
  } finally {
    freeze(pid, false);
  }
  let dropped = false;
  const returned = await until(
    async () => {
      const now = await active(session);
      if (now !== "qd") dropped = true;
      return dropped && now === "qd";
    },
    "qd вернул туннель сам",
    90000,
  );
  const after = await invoke(session, "core_logs", { engine: "qd" });
  check("qd признал туннель потерянным", dropped, "иначе проверка ничего не доказала");
  check("и вернул его сам", returned);
  check("тем же процессом", pids("qd-dev.exe")[0] === pid, `${pid} → ${pids("qd-dev.exe")[0]}`);
  const meddled = meddling(after).slice(known);
  check("клиент не вмешивался", meddled.length === 0, meddled.join(" | "));
  for (const line of after.filter((l) => /roam|carry|umiray:/.test(l)).slice(-8)) {
    console.log(`     ${line}`);
  }

  // --- выключение не воскрешает ----------------------------------------------

  await invoke(session, "core_stop");
  await wait(4000);
  check("штатное выключение qd не воскрешает", (await active(session)) === null);
} finally {
  await invoke(session, "settings_update", { patch: { engine: was } });
}

// --- клиент умер — qd с ним (D-058) ----------------------------------------

kill("umiray-dev.exe");
const gone = await until(
  () => pids("umiray-dev.exe").length === 0 && pids("qd-dev.exe").length === 0,
  "qd ушёл следом за клиентом",
  10000,
);
check("падение клиента убило qd", gone, gone ? "" : `осталось: ${pids("qd-dev.exe")}`);
if (!gone) kill("qd-dev.exe");

console.log(failed ? `\nпровалено проверок: ${failed}` : "\nвсе проверки прошли");
process.exit(failed ? 1 : 0);
