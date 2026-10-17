import type { ReactNode } from "react";

/** 小さな表示ラベル(コピー元アプリ名など)。 */
export function Badge({ children }: { children: ReactNode }) {
  return (
    <span className="rounded-sm border border-line px-1 font-mono text-[10px] leading-4 text-text-sub">
      {children}
    </span>
  );
}
