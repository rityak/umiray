import { describe, expect, it } from "vitest";
import { githubPage, githubPageOf } from "./github";

describe("githubPage", () => {
  it("turns a raw file into its page", () => {
    expect(
      githubPage(
        "https://raw.githubusercontent.com/itdoginfo/allow-domains/main/Russia/inside-raw.lst",
      ),
    ).toBe("https://github.com/itdoginfo/allow-domains/blob/main/Russia/inside-raw.lst");
  });

  it("turns a release asset into the release", () => {
    expect(
      githubPage(
        "https://github.com/runetfreedom/russia-blocked-geosite/releases/latest/download/ru-blocked.txt",
      ),
    ).toBe("https://github.com/runetfreedom/russia-blocked-geosite/releases/latest");
    expect(githubPage("https://github.com/owner/repo/releases/download/v1.2/list.txt")).toBe(
      "https://github.com/owner/repo/releases/tag/v1.2",
    );
  });

  it("reads raw links on github.com and jsDelivr mirrors", () => {
    expect(githubPage("https://github.com/owner/repo/raw/main/a/b.lst")).toBe(
      "https://github.com/owner/repo/blob/main/a/b.lst",
    );
    expect(githubPage("https://cdn.jsdelivr.net/gh/owner/repo@main/a/b.lst")).toBe(
      "https://github.com/owner/repo/blob/main/a/b.lst",
    );
  });

  it("has no page for other hosts", () => {
    expect(githubPage("https://community.antifilter.download/list/domains.lst")).toBeNull();
    expect(githubPage("http://raw.githubusercontent.com/o/r/main/x")).toBeNull();
    expect(githubPage("not a url")).toBeNull();
  });

  it("takes the first address that leads to GitHub", () => {
    expect(
      githubPageOf([
        "https://antifilter.download/list/domains.lst",
        "https://raw.githubusercontent.com/o/r/main/x.lst",
      ]),
    ).toBe("https://github.com/o/r/blob/main/x.lst");
    expect(githubPageOf(["https://antifilter.download/list/domains.lst"])).toBeNull();
  });
});
