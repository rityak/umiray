import { indentWithTab } from "@codemirror/commands";
import { yaml } from "@codemirror/lang-yaml";
import { HighlightStyle, indentUnit, syntaxHighlighting } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { basicSetup } from "codemirror";
import { useEffect, useRef } from "react";
import { t, tk } from "../i18n";

/// CodeMirror's unlayered base theme overrides rootik's layer, so bind kit tokens
/// explicitly while the host supplies the background (D-142).
const theme = EditorView.theme(
  {
    "&": { height: "100%" },
    ".cm-gutters": {
      backgroundColor: "var(--rk-well-soft)",
      color: "var(--rk-text-3)",
      border: "none",
    },
    ".cm-activeLine, .cm-activeLineGutter": {
      backgroundColor: "var(--rk-editor-active-line)",
    },
    ".cm-cursor": { borderLeftColor: "var(--rk-accent-text)", borderLeftWidth: "2px" },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground, ::selection": {
      backgroundColor: "var(--rk-editor-selection)",
    },
    ".cm-content": { fontFamily: "var(--rk-font-mono)", padding: "8px 0" },
    ".cm-scroller": { fontFamily: "var(--rk-font-mono)", lineHeight: "1.55" },
    "&.cm-focused": { outline: "none" },
    ".cm-searchMatch": {
      backgroundColor: "color-mix(in oklab, var(--rk-warn) 30%, transparent)",
    },
  },
  { dark: true },
);

/**
 * Use HighlightStyle, since basicSetup does not produce .tok-* classes.
 * Its fallback style yields to these kit-token colors regardless of extension order.
 */
const highlight = HighlightStyle.define([
  { tag: tags.comment, color: "var(--rk-text-2)", fontStyle: "italic" },
  {
    tag: [tags.propertyName, tags.definition(tags.propertyName), tags.attributeName],
    color: "var(--rk-accent)",
  },
  // YAML's unquoted scalars use content, including most values in our files (S-014).
  { tag: [tags.string, tags.special(tags.string), tags.content], color: "var(--rk-success)" },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "var(--rk-warn)" },
  // Make anchors and references immediately recognizable.
  { tag: [tags.labelName, tags.variableName], color: "var(--rk-warn)" },
  { tag: [tags.typeName, tags.meta], color: "var(--rk-text-2)" },
  {
    tag: [tags.keyword, tags.operator, tags.punctuation, tags.separator],
    color: "var(--rk-text-2)",
  },
  { tag: tags.invalid, color: "var(--rk-danger)" },
]);

/// YAML forbids tabs; use the two-space indentation of mihomo examples.
const INDENT = "  ";

const PHRASES = [
  tk("Find"),
  tk("Replace"),
  tk("next"),
  tk("previous"),
  tk("all"),
  tk("match case"),
  tk("regexp"),
  tk("by word"),
  tk("replace"),
  tk("replace all"),
  tk("close"),
  tk("Go to line"),
  tk("go"),
  tk("current match"),
  tk("on line"),
  tk("Selection deleted"),
  tk("replaced match on line $"),
  tk("replaced $ matches"),
  tk("Control character"),
];

export type Props = {
  value: string;
  onChange: (text: string) => void;
  /// Generated config is owned by the client and shown read-only.
  readOnly?: boolean;
  /// Link lists are plain text: their URL fragments are node names, not comments (D-065).
  plain?: boolean;
};

/**
 * Keep one editor instance and synchronize only changed external text, preventing
 * echoed keystrokes from resetting the cursor (D-042).
 */
export default function CodeMirror({ value, onChange, readOnly = false, plain = false }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const latest = useRef(onChange);
  latest.current = onChange;

  // CodeMirror owns the document after mounting; the next effect handles external changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: document synchronization is separate
  useEffect(() => {
    if (!host.current) return;
    const editor = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup,
          EditorState.phrases.of(Object.fromEntries(PHRASES.map((phrase) => [phrase, t(phrase)]))),
          keymap.of([indentWithTab]),
          indentUnit.of(INDENT),
          ...(plain ? [] : [yaml()]),
          syntaxHighlighting(highlight),
          theme,
          EditorView.lineWrapping,
          EditorState.readOnly.of(readOnly),
          EditorView.editable.of(!readOnly),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) latest.current(update.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = editor;
    return () => {
      editor.destroy();
      view.current = null;
    };
  }, [readOnly, plain]);

  useEffect(() => {
    const editor = view.current;
    if (!editor || editor.state.doc.toString() === value) return;
    editor.dispatch({
      changes: { from: 0, to: editor.state.doc.length, insert: value },
    });
  }, [value]);

  return <div ref={host} className="rk-code-editor-theme selectable h-full overflow-hidden" />;
}
