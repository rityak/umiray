/**
 * Where a rule set lives on GitHub, from the address it is downloaded from: a raw file
 * becomes its page, a release asset becomes the release. Not GitHub — `null`, no button.
 */
export function githubPage(url: string): string | null {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return null;
  }
  if (parsed.protocol !== "https:") return null;
  const parts = parsed.pathname.split("/").filter(Boolean);
  const repo = (owner?: string, name?: string) =>
    owner && name ? `https://github.com/${owner}/${name}` : null;

  if (parsed.hostname === "raw.githubusercontent.com") {
    const [owner, name, ...file] = parts;
    const base = repo(owner, name);
    return base && file.length > 1 ? `${base}/blob/${file.join("/")}` : base;
  }
  if (parsed.hostname === "cdn.jsdelivr.net" && parts[0] === "gh") {
    const [, owner, named, ...file] = parts;
    const [name, ref] = (named ?? "").split("@");
    const base = repo(owner, name);
    return base && ref && file.length > 0 ? `${base}/blob/${ref}/${file.join("/")}` : base;
  }
  if (parsed.hostname === "github.com") {
    const [owner, name, kind, ...rest] = parts;
    const base = repo(owner, name);
    if (!base) return null;
    if (kind === "releases") {
      // releases/latest/download/<file> and releases/download/<tag>/<file>
      if (rest[0] === "latest") return `${base}/releases/latest`;
      if (rest[0] === "download" && rest[1]) return `${base}/releases/tag/${rest[1]}`;
      return `${base}/releases`;
    }
    if (kind === "raw" && rest.length > 1) return `${base}/blob/${rest.join("/")}`;
    if (kind === "blob" || kind === "tree") return `https://github.com${parsed.pathname}`;
    return base;
  }
  return null;
}

/// The first address of a rule set that leads to GitHub.
export function githubPageOf(urls: string[]): string | null {
  for (const url of urls) {
    const page = githubPage(url);
    if (page) return page;
  }
  return null;
}
