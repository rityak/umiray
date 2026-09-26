/**
 * The splash from `index.html`: what is visible while there is no window yet.
 *
 * The markup lives in the document itself, not here (D-090) — otherwise it would arrive with
 * the very bundle whose wait it covers. From here it is only controlled.
 *
 * A stage is not a percentage: loading has no honest denominator, and a lying bar is worse
 * than none. The line names what we are waiting for **now**.
 */

const NODE = "splash";
const STEP = "splash-step";

/// How long to wait before removing the splash no matter what. A silent backend is no
/// reason to leave a person staring at the runner forever: the window is behind it, and it
/// can show by itself that there is no data.
const CEILING = 8_000;

/// The ceiling while downloading the core (D-094). Separate and much larger than the usual
/// one, because it waits for something else: the usual one guards a **silent** backend, this
/// one fifteen megabytes over someone else's network. It cannot be dropped entirely: a broken
/// download is as silent as a silent backend, and the splash would stay forever.
const DOWNLOAD_CEILING = 180_000;

/// How long the fade lives. The same number as the splash's `transition`: removing the node
/// earlier means a blink.
const FADE = 180;

let closed = false;
let ceiling: ReturnType<typeof setTimeout>;

/// Re-arm the ceiling. There is one, and it is extended, not added a second time.
function arm(ms: number): void {
  clearTimeout(ceiling);
  ceiling = setTimeout(done, ms);
}

/// What is loading now. Does nothing if the splash is already gone.
export function step(text: string): void {
  document.getElementById(STEP)?.replaceChildren(text);
}

/// A step that is **really** long has begun. The usual ceiling will not survive it, and
/// removing the splash mid-download would show a window with nothing to work with.
export function hold(): void {
  arm(DOWNLOAD_CEILING);
}

/// Remove the splash. A second call does nothing — the ceiling closes it too.
export function done(): void {
  if (closed) return;
  closed = true;
  clearTimeout(ceiling);
  const node = document.getElementById(NODE);
  if (!node) return;
  node.dataset.done = "1";
  setTimeout(() => node.remove(), FADE);
}

arm(CEILING);
