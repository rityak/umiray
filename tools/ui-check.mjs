// Проверка интерфейса в живом окне. Разговор со страницей — в `cdp.mjs`.
//
// Окно на rootik (D-142), и проверка смотрит на его разметку: `.rk-dock-item`, роли
// радиокнопок, `aria-label` значков. Цвет и размеры — забота набора, здесь их не меряют.
//
// Где взять окно:
//   - настоящее: set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
//     и `npm run tauri dev`;
//   - без Tauri: `npm run dev` отвечает заглушкой бэкенда (src/dev/mock.ts), а Edge
//     поднимается с `--remote-debugging-port=9222 --headless=new http://localhost:1420/`.
//
// По умолчанию проверка ничего не запускает и ничего не пишет. Сценарии с последствиями —
// отдельными флагами:
//   set UI_CHECK_CONNECT=1   поднять ядро кнопкой, увидеть трафик, погасить
//   set UI_CHECK_ADD=1       завести одноразовую ссылку и удалить её (отказ, если
//                            источник «Мои ссылки» уже есть)
//   set UI_CHECK_PRESETS=1   завести набор, применить, получить отказ на удаление
//                            применённого, вернуть прежний и убрать свой

import { attach, SHOTS_DIR } from "./cdp.mjs";

const port = Number(process.env.UI_CHECK_PORT ?? 9222);
const session = await attach({ port, shots: process.env.UI_CHECK_SHOTS ?? SHOTS_DIR });

let failed = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failed += 1;
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
};

/// Помощники страницы: приклеиваются к каждому выражению.
const PAGE = `
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const dock = async (name) => {
    const item = [...document.querySelectorAll('.rk-dock-item')].find((b) => b.textContent.includes(name));
    item?.click();
    await wait(700);
    return item !== undefined;
  };
  const byText = (selector, text) =>
    [...document.querySelectorAll(selector)].find((n) => n.textContent.trim().includes(text));
  const panel = () => document.getElementById('section-panel');
`;
const run = (body) => session.eval(PAGE + body);

// --- шапка и dock ------------------------------------------------------------

const head = await run(`
  const bar = document.querySelector('header');
  const status = bar.querySelector('[role="status"]');
  const grip = bar.querySelector('.rk-titlebar-center');
  return {
    status: status?.textContent.trim() ?? '',
    add: !!bar.querySelector('[aria-label="Добавить подписку или ссылку"]'),
    look: !!bar.querySelector('[aria-label="Оформление"]'),
    controls: bar.querySelectorAll('.rk-titlebar-controls button').length,
    drags: grip?.hasAttribute('data-tauri-drag-region') ?? false,
    tabs: [...document.querySelectorAll('.rk-dock-item')].map((b) => b.textContent.trim()),
    icons: [...document.querySelectorAll('.rk-dock-item')].every((b) => b.querySelector('svg')),
  };
`);
check(
  "состояние в шапке названо словом",
  /Подключено|Отключено|Запуск|Ядро не найдено/.test(head.status),
  head.status,
);
check("«+» и «Оформление» в шапке", head.add && head.look);
check("три кнопки окна", head.controls === 3, String(head.controls));
check("пустое место шапки тянет окно (B-013)", head.drags);
const ORDER = [
  "Соединение",
  "Источники",
  "Группы",
  "Маршрутизация",
  "Настройки",
  "Инструменты",
  "Логи",
];
check(
  "семь разделов в dock, по порядку",
  JSON.stringify(head.tabs) === JSON.stringify(ORDER),
  head.tabs.join(" · "),
);
check("у каждого раздела значок", head.icons);

// --- соединение --------------------------------------------------------------

const conn = await run(`
  await dock('Соединение');
  const radios = (label) => [...document.querySelectorAll('[aria-label="' + label + '"] input[type="radio"]')].map((r) => r.closest('label')?.textContent.trim());
  return {
    power: document.querySelector('[aria-label="VPN"]')?.hasAttribute('aria-pressed') ?? false,
    modes: radios('Перехват'),
    routes: radios('Маршрут'),
    exit: !!document.querySelector('[aria-label="VPN"]')?.closest('.rk-card')?.querySelector('.rk-item-title')?.textContent.trim(),
    load: !!byText('.rk-card-title', 'Нагрузка'),
    nodes: !!byText('.rk-card-title', 'Узлы'),
  };
`);
check("питание — кнопка с состоянием", conn.power);
check(
  "перехват: Proxy · System · TUN",
  JSON.stringify(conn.modes) === '["Proxy","System","TUN"]',
  conn.modes.join(" · "),
);
check(
  "маршрут: Direct · Auto · Manual · Rules",
  JSON.stringify(conn.routes) === '["Direct","Auto","Manual","Rules"]',
  conn.routes.join(" · "),
);
check("выход назван", conn.exit);
check("нагрузка и узлы на месте", conn.load && conn.nodes);

const views = await run(`
  const toggle = document.querySelector('[aria-label="Вид списка"]');
  if (!toggle) return { found: false };
  const tiles = document.querySelectorAll('[data-node]').length;
  toggle.querySelectorAll('input')[1].click();
  await wait(400);
  const table = !!panel().querySelector('tbody');
  toggle.querySelectorAll('input')[0].click();
  await wait(400);
  return { found: true, tiles, table };
`);
check(
  "узлы плитками и таблицей (D-079)",
  views.found !== false && (views.tiles === 0 || views.table),
  JSON.stringify(views),
);

// «+» обещает «добавить» — значит открыться должно окно ссылки с курсором в поле (D-120).
await session.clickReal('[aria-label="Добавить подписку или ссылку"]');
const add = await run(`
  await wait(400);
  const dialog = document.querySelector('dialog[open]');
  return { open: !!dialog, focus: document.activeElement?.getAttribute('type') ?? '' };
`);
check("«+» открывает окно ссылки", add.open);
check("курсор в поле ссылки", add.focus === "url", add.focus);
await session.key("Escape", 27);
check("Esc закрывает окно", await run("return !document.querySelector('dialog[open]');"));

// --- оформление --------------------------------------------------------------

await session.clickReal('[aria-label="Оформление"]');
const look = await run(`
  await wait(400);
  // По значениям, а не подписям: подписи формы переведены (\`ruTranslate\`).
  const layout = document.querySelector('input[value="inset"]')?.closest('.rk-segmented');
  const inset = layout?.querySelector('input[value="inset"]');
  const before = document.querySelector('.rk-shell').dataset.variant;
  inset?.click();
  await wait(300);
  const after = document.querySelector('.rk-shell').dataset.variant;
  layout?.querySelector('input[value="' + before + '"]')?.click();
  await wait(300);
  return { page: !!byText('.rk-page-title', 'Оформление'), before, after, back: document.querySelector('.rk-shell').dataset.variant };
`);
check("«Оформление» — форма rootik", look.page);
check(
  "раскладка islands / inset переключается и возвращается",
  look.after === "inset" && look.back === look.before,
  JSON.stringify(look),
);
await run(
  "document.querySelector('[aria-label=\"Закрыть\"].rk-icon-button, .rk-page-actions button')?.click(); await wait(300); return 1;",
);

// --- разделы -----------------------------------------------------------------

const sections = await run(`
  const out = {};
  await dock('Источники');
  out.sources = !!byText('.rk-card-title', 'Добавить источник');
  await dock('Группы');
  out.groups = !!byText('button', 'Группа') && !!byText('.rk-divider', 'имена, занятые клиентом');
  await dock('Маршрутизация');
  out.routing = !!document.querySelector('[aria-label="Набор маршрутизации"]') && !!byText('.rk-card-title', 'MATCH');
  await dock('Настройки');
  out.settings = !!document.querySelector('[aria-label="Разделы настроек"]') && !!document.querySelector('[aria-label="Что настраиваем"]');
  [...document.querySelectorAll('[aria-label="Вид"] input')][1]?.click();
  await wait(1200);
  out.code = !!document.querySelector('.cm-editor');
  [...document.querySelectorAll('[aria-label="Вид"] input')][0]?.click();
  await wait(300);
  await dock('Инструменты');
  out.tools = document.querySelectorAll('[data-rk-nav-item]').length > 1;
  // Точно, а не подстрокой: в живом клиенте рядом стоит «config-test».
  [...document.querySelectorAll('[data-rk-nav-item]')].find((n) => n.textContent.trim() === 'config')?.click();
  await wait(800);
  out.config = !!document.querySelector('[data-effective] .rk-code-block');
  await dock('Логи');
  await wait(1800);
  // «На людях» лог закрыт (D-127) — это тоже исправный раздел.
  out.logs = !!document.querySelector('.rk-log') || !!byText('.rk-empty-title', 'Пусто') || !!byText('.rk-empty-title', 'Лог скрыт');
  await dock('Соединение');
  return out;
`);
for (const [name, ok] of Object.entries(sections)) check(`раздел: ${name}`, ok);

// --- режим разработчика ------------------------------------------------------

const dev = await run(`
  const key = 'header [aria-label="Режим разработчика"]';
  const button = document.querySelector(key);
  if (!button) return { есть: false };
  button.click();
  await wait(800);
  const out = { есть: true, отладка: !!document.querySelector('[data-dev]') };
  document.querySelector(key).click();
  await wait(500);
  out.вышли = !document.querySelector('[data-dev]');
  return out;
`);
if (dev.есть) {
  check("режим разработчика открывает отладку", dev.отладка);
  check("и закрывается той же кнопкой", dev.вышли);
} else {
  console.log("--   режима разработчика нет: собрано не в dev");
}

// --- узкое окно --------------------------------------------------------------

await session.send("Emulation.setDeviceMetricsOverride", {
  width: 680,
  height: 600,
  deviceScaleFactor: 1,
  mobile: false,
});
const narrow = await run(`
  await wait(600);
  return { wide: document.documentElement.scrollWidth, main: document.querySelector('main').scrollWidth - document.querySelector('main').clientWidth };
`);
check("680 px: окно не шире себя", narrow.wide <= 680 && narrow.main <= 1, JSON.stringify(narrow));
await session.shot("narrow");
await session.send("Emulation.clearDeviceMetricsOverride");

// --- сценарии с последствиями -------------------------------------------------

if (process.env.UI_CHECK_CONNECT === "1") {
  await run("await dock('Соединение'); return 1;");
  const was = await run(
    "return document.querySelector('[aria-label=\"VPN\"]').getAttribute('aria-pressed');",
  );
  if (was === "true") await session.clickReal('[aria-label="VPN"]');
  await session.until(
    "document.querySelector('[aria-label=\"VPN\"]').getAttribute('aria-pressed') === 'false'",
  );
  await session.clickReal('[aria-label="VPN"]');
  const up = await session.until(
    "document.querySelector('header [role=\"status\"]').textContent.includes('Подключено')",
    { timeout: 30000 },
  );
  check("питание поднимает ядро", up);
  const load = await session.until("!!document.querySelector('.rk-stat')", { timeout: 10000 });
  check("нагрузка показывает приём и отдачу", load);
  await session.shot("connected");
  if (was !== "true") {
    await session.clickReal('[aria-label="VPN"]');
    check(
      "и гасит его",
      await session.until(
        "document.querySelector('header [role=\"status\"]').textContent.includes('Отключено')",
      ),
    );
  }
}

if (process.env.UI_CHECK_ADD === "1") {
  await run("await dock('Источники'); return 1;");
  if (await run("return !!byText('.rk-card-title', 'Мои ссылки');")) {
    console.log(
      "--   UI_CHECK_ADD пропущен: «Мои ссылки» уже есть, одноразовую ссылку не отличить",
    );
  } else {
    await session.clickReal('[aria-label="Добавить подписку или ссылку"]');
    await run(`
      const input = document.querySelector('dialog[open] input[type="url"]');
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
      set.call(input, 'vless://00000000-0000-0000-0000-000000000000@127.0.0.1:9#ui-check');
      input.dispatchEvent(new Event('input', { bubbles: true }));
      await wait(200);
      byText('dialog[open] button', 'Добавить').click();
      return 1;
    `);
    const mine =
      "[...document.querySelectorAll('.rk-card-title')].some((n) => n.textContent.includes('Мои ссылки'))";
    check("ссылка заводит «Мои ссылки»", await session.until(mine, { timeout: 15000 }));
    await run(`
      const card = [...document.querySelectorAll('.rk-card')].find((c) => c.querySelector('.rk-card-title')?.textContent.includes('Мои ссылки'));
      const remove = [...card.querySelectorAll('button')].find((b) => b.textContent.includes('Удалить'));
      remove.click(); await wait(300);
      [...card.querySelectorAll('button')].find((b) => b.textContent.includes('Точно'))?.click();
      await wait(1500);
      return 1;
    `);
    check(
      "и удаляется вторым нажатием",
      await run("return !byText('.rk-card-title', 'Мои ссылки');"),
    );
  }
}

if (process.env.UI_CHECK_PRESETS === "1") {
  const PICK = `
    const picker = () => document.querySelector('[aria-label="Набор маршрутизации"]');
    const pick = async (label) => {
      picker().click();
      await wait(300);
      [...document.querySelectorAll('[role="option"]')].find((o) => o.textContent.includes(label))?.click();
      await wait(1200);
    };
    // Действия над набором — в меню «⋯»: открыть и нажать пункт по тексту.
    const act = async (label) => {
      document.querySelector('[aria-label="Действия с набором"]').click();
      await wait(300);
      [...document.querySelectorAll('[role="menuitem"]')].find((o) => o.textContent.includes(label))?.click();
      await wait(300);
    };
    const drop = async () => {
      await act('Удалить набор');
      [...document.querySelectorAll('[role="menuitem"]')].find((o) => o.textContent.includes('Точно удалить'))?.click();
      await wait(1500);
    };
  `;
  const flow = await run(`${PICK}
    await dock('Маршрутизация');
    const before = picker().textContent.trim();
    await act('Новый набор');
    await wait(1200);
    const created = picker().textContent.trim();
    byText('button', 'Использовать')?.click();
    await wait(1500);
    const applied = !!byText('.rk-badge', 'используется');
    // Используемый не удаляется: второе нажатие подтверждает, отказ приходит сообщением.
    await drop();
    const refused = !!document.querySelector('.rk-callout[data-tone="danger"]') && picker().textContent.includes(created);
    // Откат: прежний набор снова применён, свой убран.
    await pick(before);
    byText('button', 'Использовать')?.click();
    await wait(1500);
    await pick(created);
    await drop();
    const options = () => { picker().click(); return wait(300).then(() => { const list = [...document.querySelectorAll('[role="option"]')].map((o) => o.textContent); picker().click(); return list; }); };
    const left = await options();
    return { before, created, applied, refused, gone: !left.some((o) => o.includes(created)) };
  `);
  check("новый набор заводится и открывается", flow.created !== flow.before, JSON.stringify(flow));
  check("и применяется", flow.applied);
  check("применённый не удаляется", flow.refused);
  check("откат: прежний применён, свой убран", flow.gone);
}

console.log(failed === 0 ? "\nвсё на месте" : `\nпровалено: ${failed}`);
process.exit(failed === 0 ? 0 : 1);
