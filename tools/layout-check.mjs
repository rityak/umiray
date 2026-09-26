// Regression check on the browser demo; never touches a live core.
import assert from "node:assert/strict";
import { attach } from "./cdp.mjs";

const session = await attach({ port: 9333, reload: false });
assert.equal(await session.eval("return document.documentElement.dataset.demo;"), "true");
const previous = await session.eval("return localStorage.getItem('umiray:appearance');");
try {
  for (const layout of ["inset", "islands"]) {
    await session.eval(`
      localStorage.setItem('umiray:appearance', JSON.stringify({layout: ${JSON.stringify(layout)}, pill: true}));
      localStorage.removeItem('umiray:rules-hint-hidden');
    `);
    await session.send("Page.reload");
    assert(
      await session.until(
        "document.querySelector('#tab-rules') && !document.getElementById('splash')",
      ),
    );
    await session.eval("await new Promise(resolve => setTimeout(resolve, 1000));");
    await session.send("Emulation.setDeviceMetricsOverride", {
      width: 984,
      height: 691,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await session.click("#tab-rules");
    assert(
      await session.until(`document.querySelector('[aria-label="Edit rule 1 values"]') !== null`),
    );
    await session.click('[aria-label="Edit rule 1 values"]');
    await session.eval(
      "const field = document.querySelector('.um-rule-values'); field.focus(); field.select();",
    );
    const values = [
      ...Array.from({ length: 15 }, (_, i) => `demo-${i}.example.org`),
      "^file{1,3}$",
    ];
    await session.send("Input.insertText", { text: `${values.join("\n")}\n` });
    await session.eval("await new Promise(resolve => setTimeout(resolve, 300));");
    assert.equal(
      await session.eval("return document.querySelector('.um-rule-values').value;"),
      `${values.join("\n")}\n`,
    );
    await session.click('[aria-label="Collapse rule 1"]');
    assert.equal(await session.eval("return document.querySelector('.um-rule-values');"), null);
    await session.click('[aria-label="Edit rule 1 values"]');
    assert.equal(
      await session.eval("return document.querySelector('.um-rule-values').value;"),
      `${values.join("\n")}\n`,
    );
    const geometry = await session.eval(`
      const field = document.querySelector('.um-rule-values');
      const header = document.querySelector('.rk-shell-header');
      const body = document.querySelector('.rk-shell-content');
      const dock = document.querySelector('.rk-shell-dock');
      return {
        layout: document.querySelector('.rk-shell').dataset.variant,
        values: field.value.trim().split('\\n').length,
        height: field.offsetHeight, radius: parseFloat(getComputedStyle(field).borderRadius),
        shape: header.dataset.shape, surface: header.classList.contains('rk-surface'),
        drag: document.querySelector('header').hasAttribute('data-tauri-drag-region'),
        overflow: getComputedStyle(body).overflowY,
        safeBottom: body.getBoundingClientRect().bottom - parseFloat(getComputedStyle(body).paddingBottom),
        dockTop: dock.getBoundingClientRect().top,
        horizontal: document.documentElement.scrollWidth > innerWidth,
      };
    `);
    assert.equal(geometry.shape, "none");
    assert.equal(geometry.layout, layout);
    assert.equal(geometry.values, 16);
    assert.equal(geometry.surface, false);
    assert.equal(geometry.drag, true);
    assert.equal(geometry.overflow, "hidden");
    assert.equal(geometry.horizontal, false);
    assert(geometry.radius <= 24 && geometry.height > 50);
    assert(geometry.safeBottom <= geometry.dockTop);
    console.log(layout, geometry);
    await session.shot(`layout-${layout}`);
    await session.click('[aria-label="Rule 1 actions"]');
    await session.eval(`
      [...document.querySelectorAll('[role="menuitem"]')].find(node => node.textContent.trim() === 'Move rule 1 down').click();
    `);
    assert(
      await session.until(
        `document.querySelector('textarea[aria-label="Rule 2 values"]') !== null`,
      ),
    );
    assert.equal(await session.eval("return document.activeElement.dataset.focus;"), "2:menu");
    assert.equal(
      await session.eval(
        "return document.querySelector('.um-rule-values').value.trim().split('\\n').length;",
      ),
      16,
    );
    await session.click('[aria-label="Rule 2 actions"]');
    await session.eval(`
      [...document.querySelectorAll('[role="menuitem"]')].find(node => node.textContent.trim() === 'Delete rule 2').click();
    `);
    assert.equal(await session.eval("return document.querySelectorAll('.um-rule').length;"), 4);
    await session.eval(`
      [...document.querySelectorAll('[role="menuitem"]')].find(node => node.textContent.trim() === 'Delete this rule?').click();
    `);
    assert(await session.until("document.querySelectorAll('.um-rule').length === 3"));
    assert(
      await session.until(
        `!document.querySelector('[aria-label="Revert unsaved changes"]').disabled`,
      ),
    );
    await session.click('[aria-label="Revert unsaved changes"]');
    assert(await session.until("document.querySelectorAll('.um-rule').length === 4"));
    await session.click('[aria-label="Edit rule 1 values"]');
    assert.equal(
      await session.eval(
        "return document.querySelector('.um-rule-values').value.includes('demo-0');",
      ),
      false,
    );
    // The oldest render intentionally arrives last; it must not replace the newest draft.
    await session.eval(`
      const invoke = window.__TAURI_INTERNALS__.invoke;
      let calls = 0;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        const delay = command === 'rules_render' ? (++calls === 1 ? 450 : 0) : 0;
        const result = await invoke(command, args);
        if (delay) await new Promise(resolve => setTimeout(resolve, delay));
        return result;
      };
      const field = document.querySelector('.um-rule-values'); field.focus(); field.select();
    `);
    await session.send("Input.insertText", { text: "old.example.org" });
    await session.eval("document.querySelector('.um-rule-values').select();");
    await session.send("Input.insertText", { text: "latest.example.org\n^file{1,3}$" });
    await session.eval("await new Promise(resolve => setTimeout(resolve, 650));");
    await session.click('input[value="code"]');
    assert(await session.until("document.querySelector('.cm-content') !== null"));
    assert.equal(
      await session.eval(`
        const text = document.querySelector('.cm-content').textContent;
        return text.includes('latest.example.org') && text.includes('^file{1,3}$') && !text.includes('old.example.org');
      `),
      true,
    );
    await session.click('[aria-label="Revert unsaved changes"]');
  }
  await session.click("#tab-connection");
  await session.until("document.querySelector('.rk-callout-close') !== null");
  await session.click(".rk-callout-close");
  assert.equal(
    await session.eval("return localStorage.getItem('umiray:rules-hint-hidden');"),
    "true",
  );
  await session.click("#tab-logs");
  await session.click("#tab-connection");
  assert.equal(
    await session.eval(
      "return document.body.innerText.includes('In Rules mode, your rules assign the exits.');",
    ),
    false,
  );
  console.log("Rules hint stays dismissed after section changes.");
} finally {
  await session.eval(
    previous === null
      ? "localStorage.removeItem('umiray:appearance');"
      : `localStorage.setItem('umiray:appearance', ${JSON.stringify(previous)});`,
  );
  session.socket.close();
}
