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
//   set UI_CHECK_SETUP=1     пройти мастер первого запуска до подключения (D-162); нужно
//                            окно, где он открыт, — у заглушки это `?fresh`

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
  // Карточка справа в «Соединении»: узлы или источники (D-160).
  const side = async (name) => {
    const item = [...document.querySelectorAll('[aria-label="Узлы или источники"] label')].find((l) => l.textContent.includes(name));
    item?.click();
    await wait(500);
    return item !== undefined;
  };
  const menuItem = (name) => byText('[role="menuitem"]', name);
`;
const run = (body) => session.eval(PAGE + body);

// --- мастер первого запуска (D-162) ------------------------------------------

if (process.env.UI_CHECK_SETUP === "1") {
  const next = "byText('dialog[open] button', 'Далее')?.click(); await wait(500);";
  const opened = await session.until(
    "[...document.querySelectorAll('dialog[open] h2, dialog[open] h1')].some((h) => h.textContent.includes('Настройка umiray'))",
    { timeout: 10000 },
  );
  check("мастер открывается после заставки", opened);
  const walked = await run(`
    // «Рекомендованная» с блокировкой рекламы (D-169): переключатель под карточкой.
    const ads = [...document.querySelectorAll('dialog[open] input[type="checkbox"]')].find((i) =>
      i.closest('label')?.textContent.includes('Блокировка рекламы'),
    );
    ads?.click();
    await wait(200);
    ${next}
    // На живом клиенте «Далее» сначала записывает и мерит — десятки секунд; ждём «Тонкости».
    const reached = async (text) => {
      for (let i = 0; i < 240 && !document.querySelector('dialog[open]')?.textContent.includes(text); i++) await wait(500);
      return document.querySelector('dialog[open]')?.textContent.includes(text) ?? false;
    };
    await reached('Узнавать сайт');
    const disputed = document.querySelectorAll('dialog[open] input[type="checkbox"]').length;
    ${next}
    await reached('Подписка или ссылка');
    const input = document.querySelector('dialog[open] input[type="url"]');
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    // Одноразовая ссылка в никуда: годится и заглушке, и живому клиенту — не подписка,
    // слота устройства не занимает.
    set.call(input, 'vless://00000000-0000-0000-0000-000000000000@127.0.0.1:9#ui-check');
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await wait(200);
    byText('dialog[open] button', 'Добавить').click();
    await wait(2500);
    const added = document.querySelectorAll('dialog[open] .rk-item-title').length > 0;
    ${next}
    const capture = [...document.querySelectorAll('dialog[open] [aria-label="Перехват"] input')].length;
    ${next}
    await wait(800);
    // Маршрут — тот же список, что в «Соединении» (D-166): DIRECT, AUTO, потом узлы.
    const pressed = () =>
      document.querySelector('dialog[open] [aria-pressed="true"]')?.closest('[data-node]')?.getAttribute('data-node') ?? null;
    const start = pressed();
    // Плитка — обёртка; нажимается её кнопка с именем узла. Третья — первый настоящий узел.
    const tile = document.querySelectorAll('dialog[open] [data-node]')[2];
    const node = tile?.getAttribute('data-node') ?? null;
    (tile?.matches('button') ? tile : tile?.querySelector('button'))?.click();
    await wait(300);
    const picked = pressed();
    byText('dialog[open] button', 'Подключить')?.click();
    return { ads: ads?.checked ?? null, disputed, added, capture, start, node, picked };
  `);
  check(
    "блокировка рекламы — галка под «Рекомендованной»",
    walked.ads === true,
    String(walked.ads),
  );
  check("«Тонкости»: три спорные опции (D-169)", walked.disputed === 3, String(walked.disputed));
  check("ссылка из мастера заводит источник", walked.added, JSON.stringify(walked));
  check("перехват: три варианта", walked.capture === 3, String(walked.capture));
  check("маршрут по умолчанию — DIRECT (D-162)", walked.start === "DIRECT", String(walked.start));
  check("узел выбирается плиткой", walked.node !== null && walked.picked === walked.node);
  // «Рекомендованная» на живом клиенте меряет резолверы одноразовым ядром — десятки секунд.
  check(
    "«Подключить» закрывает мастер и поднимает VPN",
    await session.until(
      "!document.querySelector('dialog[open]') && document.querySelector('header [role=\"status\"]').textContent.includes('Подключён')",
      { timeout: 120000 },
    ),
  );
}

// --- шапка и dock ------------------------------------------------------------

const head = await run(`
  const bar = document.querySelector('header');
  const status = bar.querySelector('[role="status"]');
  const grip = bar.querySelector('.rk-titlebar-center');
  const logo = bar.querySelector('img[alt="umiray"]');
  return {
    logo: !!logo?.complete && logo.naturalWidth > 0,
    status: status?.textContent.trim() ?? '',
    add: !!bar.querySelector('[aria-label="Добавить подписку или ссылку"]'),
    look: !!bar.querySelector('[aria-label="Оформление"]'),
    wizard: !!bar.querySelector('[aria-label="Мастер настройки"]'),
    controls: bar.querySelectorAll('.rk-titlebar-controls button').length,
    drags: grip?.hasAttribute('data-tauri-drag-region') ?? false,
    tabs: [...document.querySelectorAll('.rk-dock-item')].map((b) => b.textContent.trim()),
    icons: [...document.querySelectorAll('.rk-dock-item')].every((b) => b.querySelector('svg')),
  };
`);
check("логотип в шапке загрузился", head.logo);
check(
  "состояние в шапке названо словом",
  /Подключён|Отключён|Запуск|Ядро не найдено/.test(head.status),
  head.status,
);
check("«+», мастер и «Оформление» в шапке", head.add && head.wizard && head.look);
check("три кнопки окна", head.controls === 3, String(head.controls));
check("пустое место шапки тянет окно (B-013)", head.drags);
// Источники — вид в «Соединении», а не раздел (D-160); «Инструментов» нет (D-167).
const ORDER = ["Соединение", "Группы", "Маршрутизация", "Настройки", "Логи"];
check(
  "пять разделов в dock, по порядку",
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
    // Выход выбирается в списке (D-166): DIRECT и AUTO — первые строки, сегмента «Маршрут» нет.
    first: [...document.querySelectorAll('[data-node]')].slice(0, 2).map((n) => n.dataset.node),
    segment: !!document.querySelector('[aria-label="Маршрут"]'),
    exit: !!document.querySelector('[aria-label="VPN"]')?.closest('.rk-card')?.querySelector('.rk-item-title')?.textContent.trim(),
    load: !!byText('.rk-card-title', 'Трафик'),
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
  "выход — в списке: DIRECT и AUTO первыми, без сегмента «Маршрут»",
  JSON.stringify(conn.first) === '["DIRECT","AUTO"]' && !conn.segment,
  conn.first.join(" · "),
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

// «+» — одно меню на всё (D-160, D-165): ссылка, файл, вручную, WARP. Ссылка — окно с курсором в поле.
await session.clickReal('[aria-label="Добавить подписку или ссылку"]');
const add = await run(`
  await wait(400);
  const items = [...document.querySelectorAll('[role="menuitem"]')].filter((i) => i.checkVisibility()).map((i) => i.textContent);
  menuItem('Подписка или ссылка')?.click();
  await wait(400);
  const dialog = document.querySelector('dialog[open]');
  return { items: items.length, open: !!dialog, focus: document.activeElement?.getAttribute('type') ?? '' };
`);
check("«+» в шапке открывает меню из четырёх", add.items === 4, String(add.items));
check("пункт «Подписка или ссылка» открывает окно ссылки", add.open);
check("курсор в поле ссылки", add.focus === "url", add.focus);
await session.key("Escape", 27);
check("Esc закрывает окно", await run("return !document.querySelector('dialog[open]');"));
await session.clickReal('#section-panel [aria-label="Добавить источник"]');
check(
  "«+» в карточке узлов открывает то же меню",
  await run(
    "await wait(400); return [...document.querySelectorAll('[role=\"menuitem\"]')].filter((i) => i.checkVisibility()).length === 4;",
  ),
);
await session.key("Escape", 27);

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
  await dock('Соединение');
  await side('Источники');
  out.sources = !!document.querySelector('[aria-label="Как часто обновлять подписки"]');
  await side('Узлы');
  await dock('Группы');
  out.groups = !!byText('button', 'Группа') && !!byText('.rk-divider', 'имена, занятые клиентом');
  await dock('Маршрутизация');
  out.routing =
    !!document.querySelector('[aria-label="Набор маршрутизации"]') &&
    !!byText('.rk-card-title', 'MATCH') &&
    // Тумблер маршрутизации (D-166) — в полосе раздела.
    [...document.querySelectorAll('input[role="switch"]')].some((i) => i.closest('label')?.textContent.includes('Маршрутизация'));
  // D-158: две страницы одного документа; «Маршрут» открыт первым — rule sets и готовые наборы.
  out.ruleSets =
    !!document.querySelector('[aria-label="Страница маршрутизации"]') &&
    !!byText('.rk-card-title', 'Rule sets') &&
    !!byText('.rk-card-title', 'Готовые наборы');
  await dock('Настройки');
  out.settings = !!document.querySelector('[aria-label="Разделы настроек"]') && !!document.querySelector('[aria-label="Документ настроек"]');
  [...document.querySelectorAll('[aria-label="Вид"] input')][1]?.click();
  await wait(1200);
  out.code = !!document.querySelector('.cm-editor');
  [...document.querySelectorAll('[aria-label="Вид"] input')][0]?.click();
  await wait(300);
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
    "document.querySelector('header [role=\"status\"]').textContent.includes('Подключён')",
    { timeout: 30000 },
  );
  check("питание поднимает ядро", up);
  // Приём и отдача — плитками, а в сжатой под баннером карточке строкой «↓ … ↑ …»
  // (STYLEGUIDE, «Соединение»): годится любой вид, лишь бы оба числа были.
  const load = await session.until(
    "!!document.querySelector('.rk-stat') || /↓.*↑/.test(document.querySelector('main')?.innerText ?? '')",
    { timeout: 10000 },
  );
  check("нагрузка показывает приём и отдачу", load);
  await session.shot("connected");
  if (was !== "true") {
    await session.clickReal('[aria-label="VPN"]');
    check(
      "и гасит его",
      await session.until(
        "document.querySelector('header [role=\"status\"]').textContent.includes('Отключён')",
      ),
    );
  }
}

if (process.env.UI_CHECK_ADD === "1") {
  await run("await dock('Соединение'); await side('Источники'); return 1;");
  if (await run("return !!byText('.rk-card-title', 'Мои ссылки');")) {
    console.log(
      "--   UI_CHECK_ADD пропущен: «Мои ссылки» уже есть, одноразовую ссылку не отличить",
    );
  } else {
    await session.clickReal('[aria-label="Добавить подписку или ссылку"]');
    await run(`
      await wait(400);
      menuItem('Подписка или ссылка')?.click();
      await wait(400);
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
    // Что было до проверки — по id, выходу и тумблеру, а не по имени: имена наборов
    // повторяются, и откат «по подписи» однажды применил чужой набор. «Использовать»
    // включает маршрутизацию (D-166) — её прежнее положение тоже возвращаем.
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const was = await invoke('presets_list');
    const route = await invoke('connection_snapshot');
    const routing = (await invoke('settings_get')).routing;
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
    // Откат: прежний набор снова применён, выход и тумблер — прежние, свой набор убран.
    if (was.active) await invoke('presets_select', { id: was.active });
    await invoke('direction_set', { direction: route.direction, node: route.node });
    await invoke('routing_set', { on: routing });
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
