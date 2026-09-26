import { expect, test } from "vitest";
import { ruleValues } from "./rule-values";

test("bulk input preserves regex commas and process paths", () => {
  expect(ruleValues(" example.org\r\n\r\n^file{1,3}$\nC:\\Program Files\\app.exe\n")).toEqual([
    "example.org",
    "^file{1,3}$",
    "C:\\Program Files\\app.exe",
  ]);
});
