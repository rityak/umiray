import ts from "typescript";
import { expect, test } from "vitest";
import ru from "./locales/ru";

const sources = import.meta.glob<string>(
  ["./**/*.ts", "./**/*.tsx", "!./locales/**", "!./**/*.test.ts", "!./dev/mock.ts"],
  { query: "?raw", import: "default", eager: true },
);

const backend = import.meta.glob<string>("../src-tauri/src/**/*.rs", {
  query: "?raw",
  import: "default",
  eager: true,
});

const STRING = String.raw`"((?:[^"\\]|\\.)*)"`;
const SINGLE = new RegExp(String.raw`\bt[k]?\(\s*${STRING}`, "g");
const PLURAL = new RegExp(String.raw`\btn\(\s*[^,]+,\s*${STRING}\s*,\s*${STRING}`, "g");

function used(): { plain: Set<string>; plural: Set<string> } {
  const plain = new Set<string>();
  const plural = new Set<string>();
  for (const text of Object.values(sources)) {
    for (const match of text.matchAll(SINGLE)) plain.add(JSON.parse(`"${match[1]}"`));
    for (const match of text.matchAll(PLURAL)) plural.add(JSON.parse(`"${match[2]}"`));
  }
  return { plain, plural };
}

test("every text in code has a Russian translation of the right shape", () => {
  const { plain, plural } = used();
  const missing = [...plain].filter((key) => typeof ru[key] !== "string");
  const missingPlural = [...plural].filter((key) => !Array.isArray(ru[key]));
  expect(missing).toEqual([]);
  expect(missingPlural).toEqual([]);
});

test("the dictionary holds nothing the code no longer says", () => {
  const { plain, plural } = used();
  const unused = Object.keys(ru).filter(
    (key) =>
      !plain.has(key) &&
      !plural.has(key) &&
      !Object.values(backend).some((text) => text.includes(JSON.stringify(key))),
  );
  expect(unused).toEqual([]);
});

test("translations preserve substitution names", () => {
  const slots = (text: string) => [...new Set(text.match(/\{\w+\}/g) ?? [])].sort();
  for (const [key, entry] of Object.entries(ru)) {
    for (const text of typeof entry === "string" ? [entry] : entry) {
      expect(slots(text), key).toEqual(slots(key));
    }
  }
});

test("interface text and translation keys use English", () => {
  // These literals interpret existing user data or old log records; they are not UI copy.
  const legacy = new Set(["# Ваш узел", "отказ"]);
  const russian: string[] = [];
  for (const [path, source] of Object.entries(sources)) {
    const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
    const visit = (node: ts.Node) => {
      if (
        (ts.isStringLiteralLike(node) ||
          ts.isJsxText(node) ||
          node.kind === ts.SyntaxKind.TemplateHead ||
          node.kind === ts.SyntaxKind.TemplateMiddle ||
          node.kind === ts.SyntaxKind.TemplateTail) &&
        /[А-Яа-яЁё]/.test(node.getText(file)) &&
        !legacy.has(ts.isStringLiteralLike(node) ? node.text : "")
      ) {
        russian.push(`${path}: ${node.getText(file)}`);
      }
      ts.forEachChild(node, visit);
    };
    visit(file);
  }
  expect(russian).toEqual([]);
  expect(Object.keys(ru).filter((key) => /[А-Яа-яЁё]/.test(key))).toEqual([]);
});

test("diagnostic navigation has English keys and Russian translations", () => {
  const registry = backend["../src-tauri/src/diag.rs"]
    .split("pub const TOOLS:")[1]
    .split("];", 1)[0];
  for (const match of registry.matchAll(/(?:title|group|hint): "([^"]+)"/g)) {
    const key = match[1];
    expect(key, "diagnostic navigation").not.toMatch(/[А-Яа-яЁё]/);
    if (!/^(DNS|TLS SNI|DNS spoofing|DNS leak)$/.test(key)) {
      expect(typeof ru[key], key).toBe("string");
    }
  }
});
