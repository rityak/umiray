// Замер бюджета кадра для S-013: во что обходятся стеклянные панели поверх холста
// с дождём (D-045).
//
//   set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
//   npm run tauri dev
//   node tools/frame-budget.mjs
//
// Меряем интервалы между кадрами через requestAnimationFrame. При включённой вертикальной
// синхронизации медиана — это период развёртки, а не стоимость отрисовки: на 60 Гц она
// будет около 16.7 мс, даже когда окно ничего не делает. Поэтому смотрим не только медиану,
// а **долю пропущенных кадров** — интервалов длиннее полутора периодов. Именно они видны
// глазом как рывок, и именно они отличают «уложились» от «не уложились».
//
// Замер идёт дважды — с дождём и без, — чтобы отделить стоимость декорации от всего
// остального. Настройка возвращается на место в конце.

import { attach, SHOTS_DIR } from "./cdp.mjs";

const SECONDS = Number(process.env.FRAME_SECONDS ?? 4);

const session = await attach({
  port: Number(process.env.UI_CHECK_PORT ?? 9222),
  shots: process.env.UI_CHECK_SHOTS ?? SHOTS_DIR,
});
await session.send("Performance.enable");

/// Счётчики самого движка. Интервал между кадрами при запасе по бюджету одинаков всегда —
/// он равен периоду развёртки, и по нему не видно, во что отрисовка обошлась. Разница
/// этих счётчиков за время замера показывает потраченное процессорное время.
async function cpu() {
  const { metrics } = await session.send("Performance.getMetrics");
  const at = (name) => metrics.find((m) => m.name === name)?.value ?? 0;
  return {
    task: at("TaskDuration"),
    script: at("ScriptDuration"),
    layout: at("LayoutDuration"),
    style: at("RecalcStyleDuration"),
  };
}

/// Сколько холстов на странице. Если переключение дождя ничего не изменило, сравнивать
/// нечего — а именно так и выглядит замер, который врёт.
const canvases = () => session.eval(`return document.querySelectorAll("canvas").length;`);

/// Интервалы между кадрами. Первый отбрасываем: он меряет не отрисовку, а момент запуска.
async function sample(seconds) {
  const before = await cpu();
  const frames = await session.eval(`
    const gaps = [];
    let last = performance.now();
    const until = last + ${seconds} * 1000;
    await new Promise((done) => {
      const tick = (now) => {
        gaps.push(now - last);
        last = now;
        now < until ? requestAnimationFrame(tick) : done();
      };
      requestAnimationFrame(tick);
    });
    gaps.shift();
    const sorted = [...gaps].sort((a, b) => a - b);
    const at = (share) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * share))];
    const period = sorted[Math.floor(sorted.length / 2)];
    return {
      frames: gaps.length,
      median: period,
      p95: at(0.95),
      max: sorted[sorted.length - 1],
      dropped: gaps.filter((gap) => gap > period * 1.5).length,
    };
  `);
  const after = await cpu();
  return {
    ...frames,
    cpu: Object.fromEntries(Object.keys(after).map((k) => [k, (after[k] - before[k]) * 1000])),
  };
}

/// Тумблер дождя живёт в настройках, а они занимают область раздела целиком (D-081),
/// поэтому после переключения их надо закрыть — иначе замер будет про них, а не про фон.
async function setRain(on) {
  await session.clickReal('[aria-label="Настройки"]');
  const changed = await session.eval(`
    const row = [...document.querySelectorAll("label")].find((l) => l.textContent.includes("Фон с дождём"));
    const box = row?.querySelector('input[type="checkbox"]');
    if (!box) return "тумблера нет";
    if (box.checked !== ${on}) box.click();
    await new Promise((r) => setTimeout(r, 400));
    return "";
  `);
  await session.key("Escape", 27);
  await session.eval("await new Promise((r) => setTimeout(r, 600)); return 1;");
  return changed;
}

/// Как стояло до нас. Замер не имеет права менять настройку пользователя.
///
/// Спрашиваем диск, а не галку: настройки живут в DOM, только пока открыты, и закрытым
/// окном галку было не найти — `was` выходил `null`, а восстановление после замера
/// включало дождь всем подряд.
const was = await session.eval(
  `return (await window.__TAURI_INTERNALS__.invoke("settings_get")).effects;`,
);

const report = (name, m, canvas) =>
  console.log(
    `${name.padEnd(11)} холстов ${canvas} · кадров ${String(m.frames).padStart(4)}` +
      ` · медиана ${m.median.toFixed(1)} мс · макс ${m.max.toFixed(1)} мс` +
      ` · пропущено ${m.dropped} (${((m.dropped / m.frames) * 100).toFixed(1)}%)` +
      `
${" ".repeat(11)} процессор: задачи ${m.cpu.task.toFixed(0)} мс` +
      ` · скрипт ${m.cpu.script.toFixed(0)} мс · раскладка ${m.cpu.layout.toFixed(0)} мс` +
      ` · стили ${m.cpu.style.toFixed(0)} мс`,
  );

console.log(`дождь до замера: ${was}`);

const problem = await setRain(true);
if (problem) {
  console.error(problem);
  process.exit(2);
}
const rainOn = await canvases();
const withRain = await sample(SECONDS);
report("с дождём", withRain, rainOn);

await setRain(false);
const rainOff = await canvases();
const without = await sample(SECONDS);
report("без дождя", without, rainOff);

if (rainOn === rainOff) {
  console.error(`
замер недействителен: холстов поровну (${rainOn}) — тумблер ничего не изменил`);
  await setRain(was);
  process.exit(2);
}

// Возвращаем как было, а не «как удобно»: настройка чужая.
await setRain(was);

const budget = withRain.median * 1.5;
const verdict = withRain.dropped / withRain.frames <= 0.02;
const cost = withRain.cpu.task - without.cpu.task;
console.log(
  `
порог пропуска: ${budget.toFixed(1)} мс (полтора периода развёртки)` +
    `
дождь добавил пропусков: ${withRain.dropped - without.dropped} кадров за ${SECONDS} с` +
    `
дождь стоит процессору: ${cost.toFixed(0)} мс за ${SECONDS} с` +
    ` — ${((cost / (SECONDS * 1000)) * 100).toFixed(1)}% времени` +
    `
` +
    (verdict
      ? "уложились: пропусков не больше 2%"
      : "НЕ уложились: режем число капель, затем радиус блюра"),
);
process.exit(verdict ? 0 : 1);
