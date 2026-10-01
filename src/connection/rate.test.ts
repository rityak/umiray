import { expect, test } from "vitest";
import { smooth } from "./rate";

test("the traffic line rounds a spike off but keeps it where it was", () => {
  const line = smooth([0, 0, 0, 0, 64, 0, 0, 0, 0]);
  expect(line).toHaveLength(9);
  expect(Math.max(...line)).toBe(line[4]);
  expect(line[4]).toBeLessThan(64);
  expect(line[3]).toBeGreaterThan(0);
  expect(line[3]).toBeCloseTo(line[5]);
});

test("a steady rate stays the same, ends included", () => {
  expect(smooth([5, 5, 5, 5])).toEqual([5, 5, 5, 5]);
});
