// Сквозная цепочка кликом: подписка → таблица узлов → выбор → правка → перезапуск.
//
//   npm run dev              # vite должен работать: dev-сборка грузится с localhost:1420
//   node tools/chain.mjs
//
// Инструмент **сам поднимает и гасит окно**: без перезапуска клиента проверять нечего —
// вопрос ровно в том, что переживает его выбор и правка. Поэтому отдельный файл, а не флаг
// в `ui-check`: тот смотрит в уже открытое окно.
//
// Побочные эффекты: обновляет подписку (сетевой запрос к панели), выбирает узел и дописывает
// в текст источника одну строку. Строку убирает за собой, выбор возвращает исходный.
// Ядро не поднимается — цепочка про то, что помнит клиент, а не про туннель.

import { execSync, spawn } from "node:child_process";
import { attach } from "./cdp.mjs";

const PORT = Number(process.env.UI_CHECK_PORT ?? 9222);
const EXE = "src-tauri/target/debug/umiray.exe";
// Ссылка, которую ядро само не читает (D-063), — зато и не пытается никуда идти.
const ADDED =
  "wireguard://YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXowMTIzNDU%3D@1.2.3.4:51820" +
  "?address=10.0.0.2/32&publickey=YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXowMTIzNDU%3D#цепочка-проверки";
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
    execSync("taskkill /IM umiray.exe /F", { stdio: "ignore" });
  } catch {
    // Уже не работает — это и требовалось.
  }
};

const invoke = (session, name, args = {}) =>
  session.eval(
    `return await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)}, ${JSON.stringify(args)});`,
  );

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
const before = await invoke(session, "settings_get");
const sources = await invoke(session, "sources_list");
const source = sources.find((one) => one.url !== null);
if (!source) {
  console.error("нет ни одной подписки — цепочку начинать не с чего");
  process.exit(2);
}
const text = await invoke(session, "sources_read", { id: source.id });

// --- подписка ---------------------------------------------------------------

const began = Math.floor(Date.now() / 1000);
await session.eval(`
  [...document.querySelectorAll(".rk-dock-item")].find((t) => t.textContent.includes("Соединение"))?.click();
  await new Promise((r) => setTimeout(r, 500));
  document.querySelector('[aria-label="Обновить подписки и перемерить задержки"]').click();
  return 1;
`);
await session.until(
  `(await window.__TAURI_INTERNALS__.invoke("sources_list")).some((s) => s.updated >= ${began})`,
  { timeout: 60000 },
);
const refreshed = (await invoke(session, "sources_list")).find((one) => one.id === source.id);
check("подписка обновилась кликом", refreshed.updated >= began, `updated ${refreshed.updated}`);

// --- таблица узлов и выбор --------------------------------------------------

const chosen = await session.eval(`
  [...document.querySelectorAll(".rk-dock-item")].find((t) => t.textContent.includes("Соединение"))?.click();
  await new Promise((r) => setTimeout(r, 500));
  const view = [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === "Таблица");
  if (view) { view.click(); await new Promise((r) => setTimeout(r, 500)); }
  const rows = [...document.querySelectorAll("tbody tr")];
  if (!rows.length) return null;
  const row = rows[Math.min(1, rows.length - 1)];
  const name = row.querySelector("td")?.textContent.trim() ?? null;
  row.click();
  await new Promise((r) => setTimeout(r, 800));
  return name;
`);
check("таблица узлов открылась и строка нажалась", chosen !== null, String(chosen));
const picked = await invoke(session, "settings_get");
check(
  "выбор доехал до настроек",
  picked.selected !== null && chosen.includes(picked.selected),
  `${picked.selected} · направление ${picked.direction}`,
);

// --- правка -----------------------------------------------------------------

await invoke(session, "sources_write", { id: source.id, text: `${text.trimEnd()}\n${ADDED}\n` });
const edited = (await invoke(session, "sources_list")).find((one) => one.id === source.id);
check(
  "правка источника принята",
  edited.nodes === refreshed.nodes + 1,
  `узлов было ${refreshed.nodes}, стало ${edited.nodes}`,
);

// --- перезапуск -------------------------------------------------------------

kill();
await until(false, "окно закрылось");
launch();
await until(true, "окно поднялось заново");
session = await attach({ port: PORT });

const after = await invoke(session, "settings_get");
const source_after = (await invoke(session, "sources_list")).find((one) => one.id === source.id);
const nodes_after = await invoke(session, "nodes_list");

check(
  "после перезапуска выбран тот же узел",
  after.selected === picked.selected,
  `${after.selected}`,
);
check(
  "правка на месте",
  nodes_after.some((node) => node.name === "цепочка-проверки"),
  `узлов ${nodes_after.length}`,
);
check(
  "дата обновления свежая",
  source_after.updated >= began,
  `${new Date(source_after.updated * 1000).toISOString()}`,
);

// --- прибираемся ------------------------------------------------------------

await invoke(session, "sources_write", { id: source.id, text });
await invoke(session, "direction_set", {
  direction: before.direction,
  node: before.selected,
});
const back = (await invoke(session, "sources_list")).find((one) => one.id === source.id);
const restored = await invoke(session, "settings_get");
check(
  "текст источника вернулся",
  back.nodes === refreshed.nodes,
  `узлов ${back.nodes} против ${refreshed.nodes}`,
);
check(
  "выбор вернулся",
  restored.selected === before.selected && restored.direction === before.direction,
  `${restored.selected} · ${restored.direction}`,
);

console.log(failed ? `\nпровалено проверок: ${failed}` : "\nвсе проверки прошли");
process.exit(failed ? 1 : 0);
