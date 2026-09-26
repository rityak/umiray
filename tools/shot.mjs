// Снимок живого окна: `node tools/shot.mjs <имя> ["<подготовка>"]`.
//
// Подготовка — то же выражение, что у `probe.mjs`: нажать кнопку, открыть вкладку,
// прокрутить. Снимок ложится в `shots/`.

import { attach, SHOTS_DIR } from "./cdp.mjs";

const name = process.argv[2] ?? "shot";
const prepare = process.argv[3];

const session = await attach({ reload: false });
if (prepare) await session.eval(prepare);
await session.shot(name);
console.log(`${SHOTS_DIR}/${name}.png`);
process.exit(0);
