// Native stable/dev coexistence. Requires both builds and `npm run dev`.
// Uses temporary LOCALAPPDATA, no subscriptions, Proxy only; existing stable stays untouched.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join, resolve, sep } from "node:path";
import { attach } from "./cdp.mjs";

const ps = (script) =>
  JSON.parse(
    execFileSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8" }).trim(),
  );
const listeners = () =>
  ps(
    "ConvertTo-Json -Compress -InputObject @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object { $_.LocalPort -in 2080,3090,3091 } | Select-Object LocalAddress,LocalPort,OwningProcess)",
  );
const processes = () =>
  ps(
    "ConvertTo-Json -Compress -InputObject @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -in 'umiray','umiray-dev','mihomo','mihomo-dev' } | Select-Object Id,ProcessName)",
  );
const running = processes();
assert.ok(
  !running.some((p) => p.ProcessName.endsWith("-dev")),
  "Close existing dev before this test",
);
const stableRunning = running.length > 0;
const before = listeners();
assert.ok(
  !before.some((p) => p.LocalPort === 3091 || (!stableRunning && p.LocalPort === 3090)),
  "Test proxy ports must be free",
);
const stableCore = join(process.env.LOCALAPPDATA, "umiray", "mihomo.exe");
const core =
  process.env.UI_CHECK_CORE ??
  (existsSync(stableCore)
    ? stableCore
    : join(process.env.LOCALAPPDATA, "umiray-client", "mihomo.exe"));
assert.ok(existsSync(core), "Set UI_CHECK_CORE to an existing mihomo executable");
const builds = [
  {
    name: "umiray",
    core: "mihomo.exe",
    exe: "src-tauri/target/release/umiray.exe",
    port: 3090,
    cdp: 9225,
    legacy: "umiray-client",
  },
  {
    name: "umiray-dev",
    core: "mihomo-dev.exe",
    exe: "src-tauri/target/debug/umiray-dev.exe",
    port: 3091,
    cdp: 9226,
    legacy: "umiray-client-dev",
  },
];
for (const build of builds) assert.ok(existsSync(build.exe), `Build ${build.exe} first`);
const releaseDir = resolve(".release");
const temp = mkdtempSync(join(releaseDir, "dev-check-"));
assert.ok(temp.startsWith(releaseDir + sep));
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const invoke = (session, command) =>
  session.eval(`return await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)});`);
const launched = [];
try {
  for (const build of builds.filter((b) => !stableRunning || b.name === "umiray-dev")) {
    const local = join(temp, build.name);
    const old = join(local, build.legacy);
    mkdirSync(join(old, "collections/rules"), { recursive: true });
    copyFileSync(core, join(old, "mihomo.exe"));
    writeFileSync(
      join(old, "settings.json"),
      JSON.stringify({
        version: 2,
        direction: "direct",
        launch: "window",
        private: true,
        autoConnect: false,
        refresh: { onStart: false, everyMinutes: 0 },
      }),
    );
    // Leave the port absent: the actual environment-specific default must be used.
    writeFileSync(join(old, "advanced.yaml"), "tun:\n  enable: false\ndns:\n  enable: false\n");
    for (const id of ["block-ads", "direct-ru"]) {
      const yaml = readFileSync(`collections/rules/${id}.yaml`, "utf8").replace(
        /^title_en:.*\r?\n/m,
        "",
      );
      writeFileSync(join(old, `collections/rules/${id}.yaml`), yaml);
    }
    const env = {
      ...process.env,
      LOCALAPPDATA: local,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${build.cdp}`,
    };
    const child = spawn(build.exe, ["--scheduled"], { env, stdio: "ignore", windowsHide: true });
    const item = { ...build, child, env, old, local };
    launched.push(item);
    for (let i = 0; i < 40; i++) {
      if (
        await fetch(`http://127.0.0.1:${build.cdp}/json/list`).then(
          () => true,
          () => false,
        )
      )
        break;
      assert.equal(child.exitCode, null, `${build.name} exited before opening WebView`);
      await wait(250);
    }
    const session = await attach({ port: build.cdp, reload: false });
    item.session = session;
    assert.ok(await session.until("document.querySelector('header') !== null"));
    item.language = await session.eval("return localStorage.getItem('umiray:lang');");
    assert.ok(existsSync(join(local, build.name, build.core)));
    assert.ok(existsSync(join(old, "mihomo.exe")), "Migration must keep the old core");
    const update = await invoke(session, "updates_check");
    assert.equal(update.enabled, build.name === "umiray", "Dev must not install stable updates");
    const sets = await invoke(session, "rulesets_list");
    assert.ok(sets.some((s) => s.id === "block-ads" && s.titleEn === "Ad blocking"));
    for (const lang of ["en", "ru"]) {
      const origin = await session.eval(
        `localStorage.setItem('umiray:lang', ${JSON.stringify(lang)}); return performance.timeOrigin;`,
      );
      await session.send("Page.reload");
      assert.ok(
        await session.until(
          `performance.timeOrigin !== ${origin} && document.querySelector('header') !== null`,
        ),
      );
      await session.eval(`document.querySelectorAll('.rk-dock-item')[4].click(); return true;`);
      assert.ok(await session.until("document.getElementById('service-client') !== null"));
      const expected = lang === "en" ? "Ad blocking" : "Блокировка рекламы";
      assert.ok(
        await session.until(
          `document.getElementById('service-rules').textContent.includes(${JSON.stringify(expected)})`,
        ),
      );
      const layout = await session.eval(`
        const card = document.getElementById('service-client');
        const version = card.querySelector('[aria-label]');
        const check = [...card.querySelectorAll('button')].find(b => b.textContent.includes(${JSON.stringify(lang === "en" ? "Check for updates" : "Проверить обновления")}));
        if (!version || !check) return false;
        const a = version.getBoundingClientRect(), b = check.getBoundingClientRect();
        return Math.abs(a.top + a.height/2 - b.top - b.height/2) < 3 && !document.getElementById('service-core').querySelector('[aria-label="Client version"]');
      `);
      assert.ok(layout, "Client version and update check must share a row outside the core card");
      await session.eval(
        "document.getElementById('service-client').scrollIntoView({block:'start'}); return true;",
      );
      await wait(300);
      await session.shot(`${build.name}-maintenance-${lang}`);
    }
    assert.equal((await invoke(session, "core_start")).running, true);
    console.log(
      `${build.name}: data migration, localized titles, version/update row, core start passed`,
    );
  }
  const both = listeners();
  for (const build of launched) assert.ok(both.some((p) => p.LocalPort === build.port));
  assert.deepEqual(
    both.filter((p) => p.LocalPort === 2080),
    before.filter((p) => p.LocalPort === 2080),
  );
  for (const original of running) assert.ok(processes().some((p) => p.Id === original.Id));
  const dev = launched.find((b) => b.name === "umiray-dev");
  const duplicate = spawn(dev.exe, ["--scheduled"], {
    env: dev.env,
    stdio: "ignore",
    windowsHide: true,
  });
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      duplicate.kill();
      reject(new Error("Duplicate dev did not exit"));
    }, 10000);
    duplicate.once("exit", () => {
      clearTimeout(timer);
      resolve();
    });
  });
  for (const item of launched)
    assert.equal((await invoke(item.session, "core_status")).running, true);
  await invoke(dev.session, "core_stop");
  const stable = launched.find((b) => b.name === "umiray");
  if (stable) assert.equal((await invoke(stable.session, "core_status")).running, true);
  for (const original of running) assert.ok(processes().some((p) => p.Id === original.Id));
  console.log(
    stableRunning
      ? "Dev proxy on 3091, duplicate launch and stop preserve the existing stable processes and port 2080"
      : "Stable/dev proxies coexist on 3090/3091; duplicate dev and stopping dev preserve stable and port 2080",
  );
} finally {
  for (const item of launched.reverse()) {
    if (item.session) {
      await invoke(item.session, "core_stop").catch(() => {});
      await item.session
        .eval(
          `const value = ${JSON.stringify(item.language ?? null)}; if (value === null) localStorage.removeItem('umiray:lang'); else localStorage.setItem('umiray:lang', value); return true;`,
        )
        .catch(() => {});
      item.session.socket.close();
    }
    if (item.child.exitCode === null && item.child.signalCode === null) item.child.kill();
  }
  for (
    let i = 0;
    i < 40 && launched.some((x) => x.child.exitCode === null && x.child.signalCode === null);
    i++
  )
    await wait(250);
  assert.ok(
    launched.every((x) => x.child.exitCode !== null || x.child.signalCode !== null),
    "Test clients must exit before cleanup",
  );
  rmSync(temp, { recursive: true, force: true });
}
