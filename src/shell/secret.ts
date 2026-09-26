/**
 * Private mode (D-127).
 *
 * The client is shown on screen more often than it seems: a stream, a screenshot in a chat,
 * a meeting with a shared desktop. Meanwhile the screen shows server addresses and
 * subscription names — what a person and their provider are recognised by.
 *
 * We hide **addresses and source names**, not everything: country, protocol, delay and node
 * count say nothing about the person, and without them the window stops being a window.
 */

/// Replace with dots. The length stays similar so the column does not jump when toggled,
/// but not exact: the exact number of characters is already a hint.
export function mask(text: string): string {
  const length = Math.min(12, Math.max(4, Math.round(text.length / 2) + 2));
  return "•".repeat(length);
}

/// Hide if enabled. Empty stays empty: dots in place of a dash would read as "there is
/// something here, but you will not be shown".
export function hide(text: string | null | undefined, on: boolean): string | null {
  if (text === null || text === undefined || text === "") return text ?? null;
  return on ? mask(text) : text;
}

/// The hover hint. In private mode there must be none at all: a hidden address popping up
/// under the cursor is a hidden address on screen.
export function tip(text: string | null | undefined, on: boolean): string | undefined {
  return on ? undefined : (text ?? undefined);
}
