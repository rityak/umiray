import { beforeEach, describe, expect, it, vi } from "vitest";

const answer = vi.hoisted(() => ({ value: undefined as unknown }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: async () => answer.value }));

const { asAppError, nodesList, udpGet, updatesCheck, UpdateProgress, rulesetsList } = await import(
  "./api"
);

describe("the Rust boundary", () => {
  beforeEach(() => {
    answer.value = undefined;
  });

  it("a wrong-shaped answer is a refusal naming the command and field, not undefined deep in the window", async () => {
    answer.value = [{ name: "Poland 1", kind: "Vless" }];
    const error = await nodesList().then(
      () => expect.fail("a wrong-shaped answer got through"),
      asAppError,
    );
    expect(error).toMatchObject({ kind: "unexpected" });
    expect(error.details[0]).toBe("nodes_list");
    expect(error.details[1]).toContain("source");
  });

  it("extra fields are dropped: the window is promised only what is declared", async () => {
    answer.value = { on: true, nodes: 3, secret: "x" };
    expect(await udpGet()).toEqual({ on: true, nodes: 3 });
  });

  it("rule sets accept optional English titles and reject invalid translations", async () => {
    const set = { id: "ads", title: "Ads", on: false, rules: ["MATCH,DIRECT"] };
    answer.value = [set];
    expect(await rulesetsList()).toEqual(answer.value);
    answer.value = [{ ...set, titleEn: "Ad blocking" }];
    expect(await rulesetsList()).toEqual(answer.value);
    answer.value = [{ ...set, titleEn: 123 }];
    await expect(rulesetsList()).rejects.toMatchObject({ kind: "unexpected" });
  });

  it("update metadata and progress are validated before reaching the window", async () => {
    answer.value = { enabled: true, version: "1.0.1", notes: "Fixes" };
    expect(await updatesCheck()).toEqual(answer.value);
    answer.value = { enabled: true, version: 101, notes: null };
    await expect(updatesCheck()).rejects.toMatchObject({ kind: "unexpected" });
    expect(
      UpdateProgress.safeParse({ phase: "install", downloaded: 100, total: 100 }).success,
    ).toBe(true);
    expect(
      UpdateProgress.safeParse({ phase: "execute", downloaded: -1, total: null }).success,
    ).toBe(false);
  });
});
