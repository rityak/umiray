import { Prose } from "rootik";
import { blocks, spans } from "./notes";

function Inline({ text }: { text: string }) {
  return spans(text).map((span, at) =>
    span.kind === "code" ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: the parts of one line never move
      <code key={at}>{span.text}</code>
    ) : span.kind === "strong" ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: the parts of one line never move
      <strong key={at}>{span.text}</strong>
    ) : (
      span.text
    ),
  );
}

/** The update's notes as the kit's prose: what changed, not a markdown listing. */
export default function ReleaseNotes({ notes }: { notes: string }) {
  return (
    <Prose size="sm" className="selectable">
      {blocks(notes).map((block, at) =>
        block.kind === "heading" ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: notes are read once and never reordered
          <h4 key={at}>
            <Inline text={block.text} />
          </h4>
        ) : block.kind === "list" ? (
          // `list-disc`: Tailwind's reset takes the markers, Prose doesn't return them (ROOTIK §5).
          // biome-ignore lint/suspicious/noArrayIndexKey: notes are read once and never reordered
          <ul key={at} className="list-disc">
            {block.items.map((item, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: items of one list never move
              <li key={i}>
                <Inline text={item} />
              </li>
            ))}
          </ul>
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: notes are read once and never reordered
          <p key={at}>
            <Inline text={block.text} />
          </p>
        ),
      )}
    </Prose>
  );
}
