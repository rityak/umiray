type Props = {
  children: React.ReactNode;
  className?: string;
  ref?: React.Ref<HTMLDivElement>;
};

/**
 * The scrolling part of a section: takes the remaining height and scrolls its cards.
 *
 * The page itself never scrolls — in no section (D-146): otherwise content slid under the
 * dock and the whole window got a scrollbar. `contain: layout` keeps overflow from leaking
 * out (GOTCHAS). No negative side margins: they stuck out past the window content and gave
 * it a horizontal scrollbar.
 */
export default function Scroll({ children, className = "", ref }: Props) {
  return (
    <div
      ref={ref}
      className={`flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pb-1 [contain:layout] ${className}`}
    >
      {children}
    </div>
  );
}
