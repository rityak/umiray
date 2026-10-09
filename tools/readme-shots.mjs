// Capture only the browser demo, never a real client's data.
//   npm run dev
//   msedge --headless=new --remote-debugging-port=9333 --window-size=1200,780 about:blank
//   node tools/readme-shots.mjs
import { attach } from "./cdp.mjs";

const session = await attach({ port: 9333, shots: "screenshots", reload: false });
await session.send("Page.navigate", { url: "http://localhost:1420/" });
await session.until(
  "location.origin === 'http://localhost:1420' && document.querySelector('#tab-connection') !== null",
);
if (!(await session.eval("return document.documentElement.dataset.demo === 'true';"))) {
  throw new Error("Screenshots require the browser demo.");
}
await session.send("Emulation.setDeviceMetricsOverride", {
  width: 1200,
  height: 780,
  deviceScaleFactor: 1,
  mobile: false,
});
await session.eval("localStorage.setItem('umiray:lang', 'en');");
await session.send("Page.reload");
await session.until(
  "document.documentElement.lang === 'en' && document.querySelector('#tab-connection') !== null",
);
await session.until("document.getElementById('splash') === null");

const pause = (ms) => session.eval(`await new Promise(resolve => setTimeout(resolve, ${ms}));`);
/// A button or a segment by the start of its text: view switches carry a count ("Sources 2").
/// Inside an open dialog, only its controls.
const press = (text) =>
  session.eval(`
    const scope = document.querySelector('dialog[open]') ?? document;
    [...scope.querySelectorAll('button, label')]
      .find(node => node.textContent.trim().startsWith(${JSON.stringify(text)}))?.click();
  `);
const pick = (combobox, option) =>
  session.eval(`
    [...document.querySelectorAll('[role="combobox"]')]
      .find(node => node.textContent.includes(${JSON.stringify(combobox)}))?.click();
    await new Promise(resolve => setTimeout(resolve, 300));
    [...document.querySelectorAll('[role="option"]')]
      .find(node => node.textContent.trim() === ${JSON.stringify(option)})?.click();
  `);

await pause(1000);
// The demo starts elevated, so the "always as administrator" offer covers the top.
await press("Not now");
await session.click("button.rk-power");
await pause(5000);

/// Name, dock tab, and what to open there before the shot. The README positions the client
/// neutrally: rule sets and the wizard's first step name blocked services, so the shots
/// show custom rules and the capture step instead.
const SHOTS = [
  ["connection", "connection", () => press("Nodes")],
  ["sources", "connection", () => press("Sources")],
  ["groups", "groups"],
  ["routing", "rules", () => press("Custom rules")],
  ["settings", "advanced", () => pick("Umiray Settings", "Mihomo Settings")],
  ["logs", "logs"],
  [
    "setup",
    "connection",
    async () => {
      await session.click('[aria-label="Setup wizard"]');
      for (const _ of ["Subscription", "Capture"]) {
        await pause(600);
        await press("Next");
      }
    },
  ],
];
for (const [name, tab, open] of SHOTS) {
  await session.click(`#tab-${tab}`);
  await pause(500);
  await open?.();
  await pause(1200);
  const text = await session.eval("return document.body.innerText;");
  if (/[А-Яа-яЁё]/.test(text)) throw new Error(`Russian text remains in ${name}`);
  if (/\bVPN\b/.test(text.replaceAll("OpenVPN", ""))) throw new Error(`"VPN" shows in ${name}`);
  console.log(await session.shot(name));
}
await session.send("Page.reload");
await session.until("document.getElementById('splash') === null");

await session.click("#tab-advanced");
await pick("Mihomo Settings", "Umiray Settings");
await session.until("document.querySelector('[aria-label=\"Interface language\"]') !== null");
await session.click('[aria-label="Interface language"]');
await session.eval(`
  [...document.querySelectorAll('[role="option"]')]
    .find(node => node.textContent.trim() === 'Russian')?.click();
`);
if (
  !(await session.until(
    "document.documentElement.lang === 'ru' && document.body.innerText.includes('Соединение')",
  ))
) {
  throw new Error("The Russian language selection did not apply.");
}
await session.click("#tab-advanced");
await session.until("document.querySelector('[aria-label=\"Язык интерфейса\"]') !== null");
await session.click('[aria-label="Язык интерфейса"]');
await session.eval(`
  [...document.querySelectorAll('[role="option"]')]
    .find(node => node.textContent.trim() === 'English')?.click();
`);
if (
  !(await session.until(
    "document.documentElement.lang === 'en' && document.body.innerText.includes('Connection')",
  ))
) {
  throw new Error("The English language selection did not apply.");
}
console.log("Language selector: RU → EN verified.");
session.socket.close();
