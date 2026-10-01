// Kill switch галкой в окне при живом TUN (D-073): галка ставит запрет сразу, ядро умерло —
// наружу мимо туннеля не уходит ничего; галки нет — уходит (отрицательный контроль).
//
//   npm run dev
//   tools\as-admin.cmd tools\killswitch-live.mjs     # брандмауэр и TUN — с правами; UAC — человек
//
// Нужны `mihomo-dev.exe` и рабочий узел в dev-каталоге. Побочные эффекты: машина дважды
// остаётся без сети на несколько секунд — весь смысл запрета; режим, ядро и галка в
// настройках возвращаются как были, брандмауэр — сверяется с тем, что было до проверки.

import { execSync, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { attach } from "./cdp.mjs";

const PORT = Number(process.env.UI_CHECK_PORT ?? 9222);
const EXE = "src-tauri/target/debug/umiray-dev.exe";
const CORE = join(process.env.LOCALAPPDATA ?? "", "umiray-dev", "mihomo-dev.exe");
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

/// Внешний адрес напрямую (Node системный прокси не читает) — или `null`, если связи нет.
async function ip() {
  try {
    const answer = await fetch("https://api.ipify.org", { signal: AbortSignal.timeout(1500) });
    return (await answer.text()).trim();
  } catch {
    return null;
  }
}

/// Что видно снаружи за `ms`: адреса, которые ответили, и сколько раз связи не было.
async function sample(ms) {
  const seen = [];
  for (const end = Date.now() + ms; Date.now() < end; ) seen.push(await ip());
  return seen;
}

const firewall = () =>
  execSync(
    'powershell -NoProfile -Command "Get-NetFirewallProfile | ForEach-Object { \\"$($_.Name)=$($_.Enabled)=$($_.DefaultOutboundAction)\\" }"',
    { encoding: "utf8" },
  )
    .trim()
    .split(/\r?\n/)
    .join(" ");

const invoke = (session, command, args = {}) =>
  session.eval(
    `return await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)});`,
  );

async function fileMode(session) {
  const text = await invoke(session, "config_read", { id: "advanced" });
  return /tun:[\s\S]*?enable:\s*(true|false)/.exec(text)?.[1] ?? null;
}

async function open() {
  launch();
  if (!(await until(windowUp, "окно с отладочным портом"))) {
    kill("umiray-dev.exe");
    process.exit(2);
  }
  return attach({ port: PORT });
}

async function tunUp(session) {
  await invoke(session, "mode_set", { mode: "tun" });
  const status = await invoke(session, "core_start");
  return status.running && status.mode === "tun";
}

// --- поехали ---------------------------------------------------------------

try {
  execSync("net session", { stdio: "ignore" });
} catch {
  console.error("нужны права администратора: tools\\as-admin.cmd tools\\killswitch-live.mjs");
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
if (!existsSync(CORE) || pids("mihomo-dev.exe").length > 0) {
  console.error(`ядра нет на диске или оно уже запущено (${CORE})`);
  process.exit(2);
}
const HOME = await ip();
if (!HOME) {
  console.error("нет связи и без VPN — сравнивать не с чем");
  process.exit(2);
}
const walls = firewall();
// Страховка на любой выход: проверка упала, пока клиента нет, а запрет стоит, — машина
// осталась бы без сети до следующего запуска клиента. Возвращаем профили как были.
process.on("exit", () => {
  if (firewall() === walls) return;
  const back = walls
    .split(" ")
    .map((profile) => profile.split("="))
    .map(
      ([name, on, out]) =>
        `Set-NetFirewallProfile -Name ${name} -Enabled ${on} -DefaultOutboundAction ${out}`,
    )
    .join("; ");
  execSync(`powershell -NoProfile -Command "${back}"`, { stdio: "ignore" });
  console.log(`брандмауэр возвращён страховкой: ${firewall()}`);
});

kill("umiray-dev.exe");
await until(async () => !(await windowUp()), "прежнее окно закрылось", 10000);
let session = await open();
const settings = await invoke(session, "settings_get");
const was = {
  engine: settings.engine,
  killSwitch: settings.killSwitch,
  tun: await fileMode(session),
};
await invoke(session, "settings_update", { patch: { engine: "mihomo" } });

try {
  check("TUN поднят", await tunUp(session));
  check("через TUN мир видит не домашний адрес", ((await ip()) ?? HOME) !== HOME);

  // --- галка при живом TUN ставит запрет сразу --------------------------------

  const on = await invoke(session, "system_kill_switch_set", { on: true });
  check("галка поставила запрет", on.killSwitch === true);
  check("исходящее по умолчанию — Block", /=True=Block/.test(firewall()), firewall());

  // Ядро умирает, надзор поднимает его через секунду-две (D-057): в эту щель и смотрим.
  kill("mihomo-dev.exe");
  const gap = await sample(4000);
  check(
    "ядро умерло — домашний адрес не виден",
    !gap.includes(HOME),
    `${gap.filter((x) => !x).length} × нет связи`,
  );
  await until(async () => (await invoke(session, "core_status")).running, "ядро поднялось", 30000);

  // Клиент умер вместе с ядром — поднимать некому, запрет обязан держать (D-073).
  kill("umiray-dev.exe");
  await until(() => pids("mihomo-dev.exe").length === 0, "ядро ушло с клиентом", 10000);
  const dead = await sample(5000);
  check(
    "клиента нет — связи нет вовсе",
    dead.every((x) => x === null),
    dead.join(","),
  );
  session = await open();
  check(
    "запуск клиента вернул сеть",
    await until(async () => (await ip()) === HOME, "сеть вернулась", 20000),
  );

  // --- галки нет — ядро умерло, трафик уходит открыто --------------------------

  check("TUN снова поднят", await tunUp(session));
  const off = await invoke(session, "system_kill_switch_set", { on: false });
  check("галка снята — запрета нет", off.killSwitch === false && firewall() === walls, firewall());
  kill("umiray-dev.exe");
  await until(() => pids("mihomo-dev.exe").length === 0, "ядро ушло с клиентом", 10000);
  check("без галки мир видит домашний адрес (контроль)", (await ip()) === HOME);
  session = await open();
} finally {
  await invoke(session, "system_kill_switch_set", { on: false }).catch(() => {});
  await invoke(session, "core_stop").catch(() => {});
  await invoke(session, "mode_set", { mode: was.tun === "true" ? "tun" : "local" }).catch(() => {});
  await invoke(session, "settings_update", {
    patch: { engine: was.engine, killSwitch: was.killSwitch },
  }).catch(() => {});
}
check("брандмауэр как до проверки", firewall() === walls, firewall());
kill("umiray-dev.exe");

console.log(failed ? `\nпровалено проверок: ${failed}` : "\nвсе проверки прошли");
process.exit(failed ? 1 : 0);
