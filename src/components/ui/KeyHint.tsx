import type { ReactNode } from "react";

/** キー操作の案内。例: <KeyHint keys="Enter">貼り付け</KeyHint> */
export function KeyHint({ keys, children }: { keys: string; children: ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1 whitespace-nowrap text-[11px] text-text-sub">
      <kbd className="rounded-[3px] border border-line bg-surface px-1 font-mono text-[10px] text-text">
        {keys}
      </kbd>
      {children}
    </span>
  );
}
