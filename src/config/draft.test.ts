import { describe, expect, it } from "vitest";
import { dirty, fromDisk } from "./draft";

describe("fromDisk", () => {
  it("picks up the file if it was not edited", () => {
    const draft = { text: "tun:\n  enable: false\n", saved: "tun:\n  enable: false\n" };
    // This is what a mode switch from the header looks like: the file on disk changed behind the editor.
    expect(fromDisk(draft, "tun:\n  enable: true\n").text).toBe("tun:\n  enable: true\n");
  });

  it("leaves unsaved edits alone", () => {
    const draft = { text: "my edit", saved: "before" };
    expect(fromDisk(draft, "now from disk").text).toBe("my edit");
  });

  it("moves the baseline to the fresh file even for an edited draft", () => {
    // Otherwise "Revert" would go back to the state before someone else's write, undoing it.
    const draft = { text: "my edit", saved: "before" };
    expect(fromDisk(draft, "now from disk").saved).toBe("now from disk");
  });

  it("first open of a section: no draft yet", () => {
    expect(fromDisk(undefined, "from disk")).toEqual({ text: "from disk", saved: "from disk" });
  });

  it("after reading nothing counts as unsaved if not edited", () => {
    expect(dirty(fromDisk({ text: "a", saved: "a" }, "b"))).toBe(false);
  });

  it("an edited draft stays unsaved after reading too", () => {
    expect(dirty(fromDisk({ text: "mine", saved: "a" }, "b"))).toBe(true);
  });
});
