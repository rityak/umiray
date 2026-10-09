// VOLT Relay в живом TUN (D-176): Relay спрашивает имена у ядра и выходит через физический
// адаптер, а не обратно в туннель — иначе ядро пересобрало бы поток и обход пропал молча.
//
//   npm run dev
//   powershell -NoProfile -File tools/volt-dev.ps1     # бинарники VOLT из umiray-core\dist
//   tools\as-admin.cmd tools\volt-live.mjs             # WinDivert и TUN — с правами; UAC — человек
//
// Нужны `mihomo-dev.exe` и VOLT в dev-каталоге. Побочные эффекты: на время проверки dev-клиент
// в TUN с выходом DIRECT через VOLT; режим, выход и настройки VOLT возвращаются как были.

import { execSync, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { attach } from "./cdp.mjs";

const PORT = Number(process.env.UI_CHECK_PORT ?? 9222);
const EXE = "src-tauri/target/debug/umiray-dev.exe";
const DATA = join(process.env.LOCALAPPDATA ?? "", "umiray-dev");
const TARGETS = ["https://www.youtube.com/robots.txt", "https://discord.com/api/v10/gateway"];
let failed = 0;

function check(name, ok, detail = "") {
  if (!ok) failed += 1;
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
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

const invoke = (session, command, args = {}) =>
  session.eval(
    `return await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)});`,
  );

async function get(url) {
  try {
    const answer = await fetch(url, { signal: AbortSignal.timeout(10000) });
    await answer.arrayBuffer();
    return answer.status;
  } catch (error) {
    return error.cause?.code ?? error.name;
  }
}

// --- поехали ---------------------------------------------------------------

try {
  execSync("net session", { stdio: "ignore" });
} catch {
  console.error("нужны права администратора: tools\\as-admin.cmd tools\\volt-live.mjs");
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
for (const file of ["mihomo-dev.exe", "volt/volt-relay.exe", "volt/WinDivert.dll"]) {
  if (!existsSync(join(DATA, file))) {
    console.error(`нет ${join(DATA, file)}: ядро ставит клиент, VOLT — tools/volt-dev.ps1`);
    process.exit(2);
  }
}

kill("umiray-dev.exe");
await until(async () => !(await windowUp()), "прежнее окно закрылось", 10000);
spawn(EXE, ["--scheduled"], {
  detached: true,
  stdio: "ignore",
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
}).unref();
if (!(await until(windowUp, "окно с отладочным портом"))) process.exit(2);
const session = await attach({ port: PORT });

const settings = await invoke(session, "settings_get");
const advanced = await invoke(session, "config_read", { id: "advanced" });
const was = {
  engine: settings.engine,
  direction: settings.direction,
  selected: settings.selected ?? null,
  tun: /tun:[\s\S]*?enable:\s*(true|false)/.exec(advanced)?.[1] === "true",
  volt: (await invoke(session, "volt_get")).options,
};

try {
  await invoke(session, "settings_update", { patch: { engine: "mihomo" } });
  await invoke(session, "direction_set", { direction: "direct", node: null });
  await invoke(session, "mode_set", { mode: "tun" });
  await invoke(session, "volt_set", {
    options: {
      ...was.volt,
      directEnabled: true,
      scope: "direct",
      vpnEnabled: false,
      mode: "relay",
    },
  });
  const status = await invoke(session, "core_start");
  check("TUN поднят", status.running && status.mode === "tun");

  const started = (await invoke(session, "core_logs", { engine: "mihomo" })).filter((line) =>
    line.includes("VOLT Relay started"),
  );
  const ready = started.at(-1) ?? "";
  check("Relay привязан к адаптеру", /adapter \d+/.test(ready), ready);
  check("Relay спрашивает имена у ядра", /DNS http:\/\/127\.0\.0\.1:\d+\/dns\/query/.test(ready));

  for (const url of TARGETS) {
    const code = await get(url);
    check(`через DIRECT-VOLT: ${url}`, code === 200, String(code));
  }

  // fetch держит соединения живыми несколько секунд — пока они открыты, видно, с какого
  // адреса вышел Relay: из туннеля это был бы адрес TUN 198.18.0.0/15.
  const local = execSync(
    'powershell -NoProfile -Command "$p = (Get-Process volt-relay -ErrorAction SilentlyContinue).Id; ' +
      "if ($p) { Get-NetTCPConnection -OwningProcess $p -State Established -ErrorAction SilentlyContinue | " +
      'Where-Object RemotePort -eq 443 | ForEach-Object LocalAddress }"',
    { encoding: "utf8" },
  )
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  check(
    "Relay вышел через физический адаптер",
    local.length > 0 && local.every((address) => !/^198\.1[89]\./.test(address)),
    local.join(", ") || "соединений Relay не видно",
  );

  // На `log-level: info` ядро пишет строку на соединение; на `warning` — молчит, и тогда
  // эта часть ничего не доказывает.
  const log = await invoke(session, "core_logs", { engine: "mihomo" });
  if (log.some((line) => /\[(TCP|UDP)\]/.test(line))) {
    const ours = log.filter(
      (line) => line.includes("DIRECT-VOLT") && /youtube\.com|discord\.com/.test(line),
    );
    check("ядро увело запросы проверки в DIRECT-VOLT", ours.length > 0, `${ours.length} строк`);
    const loop = log.filter((line) => /\[(TCP|UDP)\].*volt-relay\.exe/.test(line));
    check("в логе ядра нет соединений самого Relay", loop.length === 0, loop[0] ?? "");
  } else {
    console.log("skip лог ядра без строк соединений (log-level не info)");
  }

  await invoke(session, "core_stop");
  await until(
    async () =>
      (await invoke(session, "core_logs", { engine: "mihomo" })).some((line) =>
        line.includes("VOLT Relay stopped"),
      ),
    "итог Relay",
    10000,
  );
  const stopped =
    (await invoke(session, "core_logs", { engine: "mihomo" }))
      .filter((line) => line.includes("VOLT Relay stopped"))
      .at(-1) ?? "";
  const transformed = Number(/transformed: (\d+)/.exec(stopped)?.[1] ?? 0);
  const failures = Number(/capture failures: (\d+)/.exec(stopped)?.[1] ?? -1);
  check("Relay изменил пакеты", transformed > 0, stopped);
  check("ошибок перехвата нет", failures === 0);
} finally {
  await invoke(session, "core_stop").catch(() => {});
  await invoke(session, "volt_set", { options: was.volt }).catch(() => {});
  await invoke(session, "mode_set", { mode: was.tun ? "tun" : "local" }).catch(() => {});
  await invoke(session, "direction_set", { direction: was.direction, node: was.selected }).catch(
    () => {},
  );
  await invoke(session, "settings_update", { patch: { engine: was.engine } }).catch(() => {});
}
kill("umiray-dev.exe");

console.log(failed ? `\nпровалено проверок: ${failed}` : "\nвсе проверки прошли");
process.exit(failed ? 1 : 0);
