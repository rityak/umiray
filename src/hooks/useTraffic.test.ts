import { describe, expect, it } from "vitest";
import type { NodeTraffic } from "../api";
import { nodeSpeed, smooth, speed, ZERO } from "./useTraffic";

const at = (down: number, up: number, nodes: NodeTraffic[] = []) => ({
  down,
  up,
  connections: 0,
  nodes,
});

const node = (name: string, down: number, connections = 1): NodeTraffic => ({
  node: name,
  down,
  up: 0,
  connections,
});

describe("speed", () => {
  it("divides the increase by the elapsed time", () => {
    expect(speed(at(1000, 500), at(4000, 2000), 1000)).toEqual({ down: 3000, up: 1500 });
  });

  it("counts in seconds, not in poll ticks", () => {
    expect(speed(at(0, 0), at(3000, 0), 1500)).toEqual({ down: 2000, up: 0 });
  });

  // This case is why the function exists: the core restarted, counters went back to zero.
  it("clamps a counter reset to zero instead of showing a negative rate", () => {
    expect(speed(at(9_000_000, 5_000_000), at(1200, 300), 1500)).toEqual(ZERO);
  });

  it("does not divide by zero when two samples arrive in one instant", () => {
    expect(speed(at(0, 0), at(500, 500), 0)).toEqual(ZERO);
  });
});

describe("smooth", () => {
  it("two stages round both the rise and fall of a short burst", () => {
    expect(smooth(smooth([{ down: 8000, up: 400 }, ZERO, ZERO, ZERO, ZERO]))).toEqual([
      { down: 500, up: 25 },
      { down: 750, up: 37.5 },
      { down: 843.75, up: 42.1875 },
      { down: 843.75, up: 42.1875 },
      { down: 791.015625, up: 39.55078125 },
    ]);
  });

  it("a burst decays gradually instead of making a plateau with an abrupt drop", () => {
    expect(smooth([{ down: 8000, up: 400 }, ZERO, ZERO, ZERO], 4)).toEqual([
      { down: 2000, up: 100 },
      { down: 1500, up: 75 },
      { down: 1125, up: 56.25 },
      { down: 843.75, up: 42.1875 },
    ]);
  });

  it("continues the average across ticks without recalculating older points", () => {
    const values = [{ down: 8000, up: 400 }, ZERO, ZERO];
    const first = smooth(values.slice(0, 2));
    expect([...first, ...smooth(values.slice(2), 4, first.at(-1))]).toEqual(smooth(values));
    expect(smooth([])).toEqual([]);
    expect(smooth([ZERO])).toEqual([ZERO]);
  });
});

describe("nodeSpeed", () => {
  it("counts the increase for each node separately", () => {
    const rates = nodeSpeed(
      at(0, 0, [node("Poland 1", 1000), node("Sweden 0", 500)]),
      at(0, 0, [node("Poland 1", 4000), node("Sweden 0", 500)]),
      1000,
    );
    expect(rates["Poland 1"].down).toBe(3000);
    expect(rates["Sweden 0"].down).toBe(0);
  });

  it("a node missing from the previous sample counts from zero", () => {
    const rates = nodeSpeed(at(0, 0, []), at(0, 0, [node("Estonia-1", 1500)]), 1500);
    expect(rates["Estonia-1"]).toEqual({ down: 1000, up: 0, connections: 1 });
  });

  // A connection closed between ticks: its bytes drop out of the node's sum, and the
  // difference goes negative. Showing "-2 MB/s" is worse than undercounting the tail.
  it("clamps a drop in the sum to zero", () => {
    const rates = nodeSpeed(
      at(0, 0, [node("Poland 1", 9000)]),
      at(0, 0, [node("Poland 1", 10)]),
      1500,
    );
    expect(rates["Poland 1"].down).toBe(0);
  });

  it("a node that stopped carrying disappears from the answer along with its rate", () => {
    const rates = nodeSpeed(at(0, 0, [node("Poland 1", 9000)]), at(0, 0, []), 1500);
    expect(rates["Poland 1"]).toBeUndefined();
  });
});
