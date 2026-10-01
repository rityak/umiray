import { expect, test } from "vitest";
import type * as api from "./api";
import { headline } from "./engines";

const status: api.Status = {
  active: null,
  running: false,
  mode: null,
  desiredMode: "local",
  restartReason: null,
  trouble: null,
  port: null,
  corePresent: false,
  qdPresent: true,
  elevated: true,
  alwaysAdmin: false,
  systemProxy: false,
  foreignProxy: null,
  autostart: false,
  killSwitch: false,
  started: null,
};

test("the header names the client's state and the engine carrying traffic", () => {
  const off = headline({ status, qd: null, powering: false });
  expect([off.tone, off.label]).toEqual(["off", "Disconnected"]);

  const qd = headline({ status: { ...status, active: "qd" }, qd: null, powering: false });
  expect([qd.tone, qd.label]).toEqual(["on", "Connected · qd"]);

  const mihomo = { ...status, active: "mihomo" as const, running: true, mode: "tun" as const };
  const on = headline({ status: mihomo, qd: null, powering: false });
  expect([on.tone, on.label]).toEqual(["on", "Connected · mihomo"]);

  expect(headline({ status: mihomo, qd: null, powering: true }).tone).toBe("connecting");
});
