/// A config file draft: what is in the editor and what is on disk. The difference between
/// them is exactly "unsaved".
export type Draft = { text: string; saved: string };
export type Drafts = Record<string, Draft>;

export const dirty = (draft: Draft | undefined): boolean =>
  draft !== undefined && draft.text !== draft.saved;

/**
 * The draft after reading the file from disk.
 *
 * Not only the editor writes config files: the mode switch edits `advanced.yaml`, a
 * direction change rewrites `groups.yaml` and `rules.yaml` (D-056). So the section checks
 * the disk on every open — otherwise it would show the text from before those writes, and
 * saving over it would roll them back.
 *
 * An edited draft is left alone: what is unsaved belongs to the user. But `saved` is
 * always refreshed — so "unsaved" shows the difference with the **fresh** file, and
 * "Revert" goes back to it, not to the one from the day before yesterday.
 */
export function fromDisk(draft: Draft | undefined, text: string): Draft {
  return { text: dirty(draft) && draft ? draft.text : text, saved: text };
}
