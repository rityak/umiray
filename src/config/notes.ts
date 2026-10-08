/**
 * Release notes are the CHANGELOG section (markdown) as GitHub got it. Only what that file
 * uses is read: headings, `-` lists, paragraphs; inline `code` and **bold**. The rest stays
 * text — nothing from the network goes in as HTML.
 */
export type Block =
  | { kind: "heading"; text: string }
  | { kind: "list"; items: string[] }
  | { kind: "text"; text: string };

export function blocks(markdown: string): Block[] {
  const out: Block[] = [];
  let gap = true;
  for (const raw of markdown.replace(/\r\n?/g, "\n").split("\n")) {
    const line = raw.trim();
    const last = out.at(-1);
    const heading = /^#{1,6}\s+(.*)$/.exec(line);
    const item = /^[-*]\s+(.*)$/.exec(line);
    if (!line) gap = true;
    else if (heading) out.push({ kind: "heading", text: heading[1] });
    else if (item && last?.kind === "list") last.items.push(item[1]);
    else if (item) out.push({ kind: "list", items: [item[1]] });
    // A wrapped line continues what it follows: a list item or a paragraph.
    else if (!gap && last?.kind === "list") last.items[last.items.length - 1] += ` ${line}`;
    else if (!gap && last?.kind === "text") last.text += ` ${line}`;
    else out.push({ kind: "text", text: line });
    if (line) gap = false;
  }
  return out;
}

export type Span = { kind: "text" | "code" | "strong"; text: string };

export function spans(text: string): Span[] {
  return text
    .split(/(`[^`]+`|\*\*[^*]+\*\*)/)
    .filter(Boolean)
    .map((part) =>
      part.startsWith("`")
        ? { kind: "code", text: part.slice(1, -1) }
        : part.startsWith("**")
          ? { kind: "strong", text: part.slice(2, -2) }
          : { kind: "text", text: part },
    );
}
