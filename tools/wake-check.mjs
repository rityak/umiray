// Сеть сменилась под ногами — клиент перепроверил узлы сам (D-112).
//
//   npm run dev                      # окно грузится с localhost:1420
//   node tools/wake-check.mjs        # поднимет окно, если его нет
//
// Повод наблюдателя — сон или смена интерфейса — из кода не создать, поэтому создаём
// **то же самое изменение таблицы адресов**, которое они дают: временный link-local адрес
// на активном адаптере и обратно. Это работа `tools/addr-blip.cmd`, и ему нужны права —
// он попросит их сам, обычным окном UAC.
//
// **Нажать «Да» в UAC должен человек.** Проверка ждёт этого до полутора минут и говорит,
// чего ждёт: сама она прав не просит и просить не может.
//
// Что проверяется: в кольце лога ядра появляется строка «сеть сменилась, узлы
// перепроверены» — то есть наблюдатель сработал, ядро ответило и узлы обойдены заново.

import { spawn } from "node:child_process";
import { existsSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { attach } from "./cdp.mjs";

const PORT = Number(process.env.UI_CHECK_PORT ?? 9222);
const MARK = join(tmpdir(), "umiray-wake-check.txt");
const SAID = "сеть сменилась, узлы перепроверены";
let failed = 0;

function check(name, ok, detail = "") {
  if (!ok) failed += 1;
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
}

const listening = () =>
  fetch(`http://127.0.0.1:${PORT}/json/list`).then(
    () => true,
    () => false,
  );

async function until(want, what, timeout = 30000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await want()) return true;
    await new Promise((r) => setTimeout(r, 500));
  }
  console.error(`не дождались: ${what}`);
  return false;
}

if (!(await listening())) {
  console.error("окно с отладочным портом не найдено.");
  console.error("следующий шаг: запустите `npm run dev`, затем");
  console.error("  set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222");
  console.error("  src-tauri/target/debug/umiray.exe --scheduled");
  process.exit(2);
}

const session = await attach({ port: PORT, reload: false });
await session.until("document.querySelector('header') !== null");

// Сторож смотрит только на работающее ядро — иначе смена сети его не касается.
const running = await session.eval(
  `return (await window.__TAURI_INTERNALS__.invoke("core_status")).running;`,
);
if (!running) {
  console.error("ядро не работает: наблюдателю нечего перепроверять.");
  console.error("следующий шаг: нажмите питание в окне и повторите проверку");
  process.exit(2);
}

const was = await session.eval(
  `return (await window.__TAURI_INTERNALS__.invoke("core_logs")).length;`,
);
rmSync(MARK, { force: true });

console.log("\nсейчас появится окно UAC — нажмите «Да», чтобы скрипт моргнул адресом.\n");
spawn("cmd", ["/c", resolve("tools/addr-blip.cmd")], { detached: true, stdio: "ignore" }).unref();

const blinked = await until(() => existsSync(MARK), "подтверждение UAC и моргание адресом", 90000);
check(
  "адрес моргнул",
  blinked && readFileSync(MARK, "utf8").trim() !== "no-adapter",
  blinked ? readFileSync(MARK, "utf8").trim() : "UAC не подтвердили?",
);

const said = await until(
  async () =>
    (await session.eval(`return await window.__TAURI_INTERNALS__.invoke("core_logs");`))
      .slice(was)
      .some((line) => line.includes(SAID)),
  "строка про смену сети в логе",
  40000,
);
check("клиент заметил смену сети и перепроверил узлы", said);
if (said) {
  const lines = (
    await session.eval(`return await window.__TAURI_INTERNALS__.invoke("core_logs");`)
  ).filter((line) => line.includes(SAID));
  console.log(`     ${lines.at(-1)}`);
}

console.log(failed === 0 ? "\nнаблюдатель за сетью работает" : `\n${failed} провалов`);
process.exit(failed === 0 ? 0 : 1);
