// Разовый замер в живом окне: `node tools/probe.mjs "<выражение>"`.
//
// `ui-check` отвечает на «всё ли на месте», а этот — на «а сколько сейчас вот это».
// Нужен при правке раскладки: посчитать высоты, промахи подложки, число элементов —
// не заводя ради одного числа проверку, которая потом висит вечно.

import { attach } from "./cdp.mjs";

const code = process.argv[2];
if (!code) {
  console.error('нужно выражение: node tools/probe.mjs "document.title"');
  process.exit(1);
}

const session = await attach({
  port: Number(process.env.UI_CHECK_PORT ?? 9222),
  reload: process.argv.includes("--reload"),
});
console.log(JSON.stringify(await session.eval(code), null, 2));
process.exit(0);
