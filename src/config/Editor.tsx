import { lazy, Suspense } from "react";
import { Spinner } from "rootik";
import { t } from "../i18n";
import type { Props } from "./CodeMirror";

/// CodeMirror arrives as a separate chunk and **only once it is opened**.
///
/// The reason is the same white window the splash exists for: the editor with its
/// highlighting and YAML parsing weighs more than the rest of the window combined, and it
/// is needed in a couple of sections, not at startup.
const CodeMirror = lazy(() => import("./CodeMirror"));

/**
 * The code editor. Only the loading boundary lives here — everything else is in
 * `CodeMirror.tsx`, and there is no reason to call it directly: both entry points want
 * exactly this behaviour.
 *
 * The waiting caption is the same as for a document not yet read from disk: for the
 * user it is one and the same wait, and it must not go by two names.
 */
export default function Editor(props: Props) {
  return (
    <Suspense fallback={<Spinner label={t("Loading")} />}>
      <CodeMirror {...props} />
    </Suspense>
  );
}
