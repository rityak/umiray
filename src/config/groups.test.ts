import { expect, test } from "vitest";
import type { Group, Node, Source } from "../api";
import { type Choices, live, preview, read, resolve, write } from "./groups";

const source = (id: string, name: string): Source => ({
  id,
  name,
  url: null,
  updated: null,
  nodes: 0,
  records: false,
});

const node = (name: string, src: string): Node => ({
  name,
  kind: "Vless",
  source: src,
  supported: true,
  country: null,
  delay: null,
  method: null,
  fallback: false,
  address: null,
  edited: false,
});

/// Две подписки. Поимённо не адресуется ни один узел: всякий лежит в провайдере,
/// и группа видит его только через `use:` (S-012, перепроверено на D-122).
const CHOICES: Choices = {
  sources: [source("demo", "Demo VPN"), source("mine", "Мои ссылки")],
  nodes: [
    node("Poland 1", "demo"),
    node("Poland 2", "demo"),
    node("Sweden 0", "demo"),
    node("Poland home", "mine"),
    node("wg-with-noise", "mine"),
  ],
};

const BLANK: Group = {
  name: "Европа",
  kind: "select",
  sources: [],
  proxies: [],
  filter: null,
  url: null,
  interval: null,
  tolerance: null,
  strategy: null,
  extra: [],
  origin: null,
};

const select = (picked: string[]) => ({
  picked,
  substring: "",
  others: [],
  understood: true,
});

test("источник целиком — это живой список: `use` без фильтра", () => {
  const out = write(BLANK, select(["Poland 1", "Poland 2", "Sweden 0"]), CHOICES);
  expect(out.sources).toEqual(["demo"]);
  expect(out.filter).toBeNull();
  expect(out.proxies).toEqual([]);
});

test("часть источника замораживает список точными именами", () => {
  const out = write(BLANK, select(["Poland 1", "Poland 2"]), CHOICES);
  expect(out.sources).toEqual(["demo"]);
  expect(out.filter).toBe("^(Poland 1|Poland 2)$");
  expect(live(CHOICES, new Set(["Poland 1", "Poland 2"]))).toBe(false);
  expect(resolve(out, CHOICES).sort()).toEqual(["Poland 1", "Poland 2"]);
});

test("поимённо не едет ни один узел — источник берётся целиком через use:", () => {
  const out = write(BLANK, select(["Poland home", "wg-with-noise"]), CHOICES);
  // Источник взят целиком, значит имён в файле нет вовсе: ни фильтра, ни `proxies:`.
  expect(out.proxies).toEqual([]);
  expect(out.filter).toBeNull();
  expect(out.sources).toEqual(["mine"]);
  expect(resolve(out, CHOICES).sort()).toEqual(["Poland home", "wg-with-noise"]);
});

test("чужое имя в proxies: сохраняется, но выбором не считается", () => {
  // Так в группе оказывается другая группа или DIRECT — не мы это записали,
  // и трогать его форма не вправе.
  const было: Group = { ...BLANK, proxies: ["DIRECT"] };
  const прочли = read(было, CHOICES);
  expect(прочли.others).toEqual(["DIRECT"]);
  expect(прочли.picked).toEqual([]);
  expect(write(было, прочли, CHOICES).proxies).toEqual(["DIRECT"]);
});

test("круг «прочитали — записали — прочитали» сходится", () => {
  for (const picked of [
    ["Poland 1", "Poland 2", "Sweden 0"],
    ["Poland 1"],
    ["Poland 1", "Poland home", "wg-with-noise"],
  ]) {
    const written = write(BLANK, select(picked), CHOICES);
    expect(read(written, CHOICES).picked.sort()).toEqual([...picked].sort());
  }
});

test("подстрока режет живой список и остаётся подстрокой при чтении", () => {
  const all = ["Poland 1", "Poland 2", "Sweden 0"];
  const out = write(BLANK, { ...select(all), substring: "poland" }, CHOICES);
  expect(out.filter).toBe("(?i)poland");
  expect(resolve(out, CHOICES)).toEqual(["Poland 1", "Poland 2"]);
  const back = read(out, CHOICES);
  expect(back.substring).toBe("poland");
  expect(back.understood).toBe(true);
  // В предпросмотре срезанное показано зачёркнутым, а не исчезает молча.
  expect(preview(out, back, CHOICES).filter((row) => row.out)).toEqual([
    { name: "Sweden 0", out: true },
  ]);
});

test("точка в подстроке остаётся точкой, а не «любым символом»", () => {
  // Подстрока осмысленна только у живого списка, поэтому источник взят целиком.
  const all = ["Poland 1", "Poland 2", "Sweden 0"];
  const out = write(BLANK, { ...select(all), substring: "и." }, CHOICES);
  expect(out.filter).toBe("(?i)и\\.");
  expect(read(out, CHOICES).substring).toBe("и.");
});

test("чужой фильтр форма не трогает и не переписывает", () => {
  const theirs: Group = { ...BLANK, sources: ["demo"], filter: "^Poland \\d+$|Sweden" };
  const back = read(theirs, CHOICES);
  expect(back.understood).toBe(false);
  expect(resolve(theirs, CHOICES)).toEqual(["Poland 1", "Poland 2", "Sweden 0"]);
});

test("чужие пункты группы сохраняются: DIRECT и другие группы форма не выкидывает", () => {
  const theirs: Group = { ...BLANK, proxies: ["DIRECT", "Европа"] };
  const back = read(theirs, CHOICES);
  expect(back.others).toEqual(["DIRECT", "Европа"]);
  expect(write(theirs, { ...back, picked: ["Poland 1"] }, CHOICES).proxies).toEqual([
    "DIRECT",
    "Европа",
  ]);
});
