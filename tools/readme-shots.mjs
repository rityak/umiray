// Capture only the browser demo, never a real client's data.
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
await session.eval("await new Promise(resolve => setTimeout(resolve, 1000));");
await session.eval(`
  [...document.querySelectorAll('button')].find(node => node.textContent.trim() === "Don't pin")?.click();
`);
await session.click('button[aria-label="VPN"]');
await session.eval("await new Promise(resolve => setTimeout(resolve, 5000));");
for (const [name, tab] of [
  ["connection", "connection"],
  ["sources", "sources"],
  ["groups", "groups"],
  ["routing", "rules"],
  ["settings", "advanced"],
  ["tools", "diag"],
  ["logs", "logs"],
]) {
  await session.click(`#tab-${tab}`);
  await session.eval(`
    [...document.querySelectorAll('button')].find(node => node.textContent.trim() === "Don't pin")?.click();
  `);
  if (name === "settings") {
    await session.eval(`
      const trigger = [...document.querySelectorAll('[role="combobox"]')]
        .find(node => node.textContent.includes('Umiray Settings'));
      trigger?.click();
    `);
    await session.eval(`
      [...document.querySelectorAll('[role="option"]')]
        .find(node => node.textContent.includes('Mihomo Settings'))?.click();
    `);
  }
  if (name === "routing") {
    await session.until(`document.querySelector('[aria-label="Edit rule 1 values"]') !== null`);
    await session.click('[aria-label="Edit rule 1 values"]');
  }
  await session.eval("await new Promise(resolve => setTimeout(resolve, 800));");
  const text = await session.eval("return document.body.innerText;");
  if (/[А-Яа-яЁё]/.test(text)) throw new Error(`Russian text remains in ${name}`);
  console.log(await session.shot(name));
}
await session.click("#tab-advanced");
await session.eval(`
  [...document.querySelectorAll('[role="combobox"]')]
    .find(node => node.textContent.includes('Mihomo Settings'))?.click();
`);
await session.eval(`
  [...document.querySelectorAll('[role="option"]')]
    .find(node => node.textContent.includes('Umiray Settings'))?.click();
`);
await session.until("document.querySelector('[aria-label=\"Interface language\"]') !== null");
await session.click('[aria-label="Interface language"]');
await session.eval(`
  [...document.querySelectorAll('[role="option"]')]
    .find(node => node.textContent.trim() === 'Русский')?.click();
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
