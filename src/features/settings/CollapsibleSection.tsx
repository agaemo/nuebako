import { useId, useState, type ReactNode } from "react";

interface CollapsibleSectionProps {
  title: string;
  /** 初期状態で開くか(S2 では「一般」だけ開く) */
  defaultOpen?: boolean;
  children: ReactNode;
}

/** 見出しをクリックして折りたためるセクション。 */
export function CollapsibleSection({
  title,
  defaultOpen = false,
  children,
}: CollapsibleSectionProps) {
  const [open, setOpen] = useState(defaultOpen);
  const bodyId = useId();
  return (
    <section className="border-b border-line">
      <h2 className="m-0 text-[12px] font-bold leading-[1.4]">
        <button
          type="button"
          aria-expanded={open}
          aria-controls={bodyId}
          onClick={() => setOpen((o) => !o)}
          className="flex w-full items-center gap-2 px-4 py-2 text-left"
        >
          <span aria-hidden="true" className="w-3 text-text-sub">
            {open ? "▾" : "▸"}
          </span>
          {title}
        </button>
      </h2>
      {open && (
        <div id={bodyId} className="flex flex-col gap-3 px-4 pb-4">
          {children}
        </div>
      )}
    </section>
  );
}
