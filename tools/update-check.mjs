// Run against a browser demo on UI_CHECK_PORT, never against a live client.
import assert from "node:assert/strict";
import { attach } from "./cdp.mjs";

const session = await attach({ port: Number(process.env.UI_CHECK_PORT ?? 9233) });
const run = (body) =>
  session.eval(`
  const wait = (ms = 300) => new Promise((resolve) => setTimeout(resolve, ms));
  const button = (text, root = document) => [...root.querySelectorAll('button')].find((node) => node.textContent.trim() === text);
  ${body}
`);
try {
  assert.ok(
    await session.until(
      "document.documentElement.dataset.demo === 'true' && !!document.querySelector('.rk-dock-item')",
    ),
  );
  assert.equal(
    await run("return document.documentElement.dataset.demo;"),
    "true",
    "Use the isolated browser demo, not Tauri",
  );
  await run("localStorage.setItem('umiray:lang', 'en'); return true;");
  const marker = `?update-check=${Date.now()}`;
  await session.send("Page.navigate", { url: `http://localhost:1420/${marker}` });
  assert.ok(
    await session.until(
      `location.search === ${JSON.stringify(marker)} && document.documentElement.lang === 'en' && !!document.querySelector('.rk-dock-item') && !!window.__TAURI_INTERNALS__`,
    ),
  );
  await run(`
    window.updateInstalls = 0;
    const original = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (command, args = {}) => {
      if (command === 'updates_check') return { enabled: true, version: '1.0.1', notes: 'Update smoke test' };
      if (command === 'updates_install') {
        window.updateInstalls++;
        args.progress.onmessage({ phase: 'download', downloaded: 50, total: 100 });
        return new Promise((resolve, reject) => {
          window.finishUpdate = () => reject({ kind: 'network', message: 'Test download failed', details: [] });
        });
      }
      return original(command, args);
    };
    button('Settings', document.querySelector('nav')).click();
    await wait(900);
    return true;
  `);
  // The client document is first in the demo's Settings section.
  assert.ok(
    await session.until(
      "[...document.querySelectorAll('button')].some(n => n.textContent.trim() === 'Check for updates')",
    ),
  );
  await run("button('Check for updates').click(); await wait(); return true;");
  assert.ok(await session.until("document.body.textContent.includes('umiray 1.0.1 is available')"));
  await run(`
    document.querySelector('[aria-label="Junk packets before handshake"]').click(); await wait();
    button('Client updates').click(); await wait();
    button('Install update', document.querySelector('dialog[open]')).click(); await wait();
    button('Disconnect and install?', document.querySelector('dialog[open]')).click(); await wait();
    return true;
  `);
  assert.equal(await run("return window.updateInstalls;"), 0, "Unsaved form blocks installation");
  await session.key("Escape", 27);
  await run(
    "document.querySelector('[aria-label=\"Junk packets before handshake\"]').click(); await wait(); return true;",
  );
  await run(`
    button('Groups', document.querySelector('nav[aria-label="Sections"]')).click(); await wait(600);
    document.querySelectorAll('[aria-label="View"] input')[1].click(); await wait(700);
    document.querySelector('.cm-content').focus(); return true;
  `);
  await session.send("Input.insertText", { text: "# unsaved update smoke test\n" });
  await run("await wait(); button('Client updates').click(); await wait(); return true;");
  await run(
    "button('Install update', document.querySelector('dialog[open]')).click(); await wait(); return true;",
  );
  assert.equal(await run("return window.updateInstalls;"), 0, "First click only confirms");
  await run(
    "button('Disconnect and install?', document.querySelector('dialog[open]')).click(); await wait(); return true;",
  );
  assert.equal(await run("return window.updateInstalls;"), 0, "Unsaved YAML blocks installation");
  assert.ok(
    await run(
      "return document.body.textContent.includes('Save or discard your edits before updating the client.');",
    ),
  );
  await session.key("Escape", 27);
  await run(
    "document.querySelector('[aria-label=" +
      JSON.stringify("Revert unsaved changes") +
      "]').click(); await wait(); button('Client updates').click(); await wait(); return true;",
  );
  await run(
    "button('Install update', document.querySelector('dialog[open]')).click(); await wait(); button('Disconnect and install?', document.querySelector('dialog[open]')).click(); await wait(); return true;",
  );
  assert.equal(await run("return window.updateInstalls;"), 1);
  assert.ok(await run("return !!document.querySelector('dialog[open] [role=progressbar]');"));
  await session.key("Escape", 27);
  assert.ok(
    await run("return !!document.querySelector('dialog[open]');"),
    "Download cannot dismiss the modal",
  );
  await session.shot("client-update-progress");
  await run("window.finishUpdate(); await wait(); return true;");
  assert.ok(await run("return document.body.textContent.includes('Test download failed');"));
  assert.ok(
    await run("return !button('Install update', document.querySelector('dialog[open]')).disabled;"),
    "Failure allows retry",
  );
  console.log(
    "ok: available version, confirmation, unsaved forms and YAML, progress, modal and retry",
  );
} catch (error) {
  console.log(
    await run(
      "return { dialogs: [...document.querySelectorAll('dialog')].map(n => ({open:n.open, text:n.innerText})), installs:window.updateInstalls, text:document.body.innerText.slice(0,600) };",
    ),
  );
  throw error;
} finally {
  await session.send("Page.reload");
  session.socket.close();
}
