import { forwardRef } from "react";
import type { StatusMessage } from "./statusMessages";

/** 一覧の上の帯。記録の停止、保存の問題、権限なしなど、いまの状態を文で伝える。 */
export const StatusBand = forwardRef<HTMLDivElement, { messages: StatusMessage[] }>(
  function StatusBand({ messages }, ref) {
    return (
      <div
        ref={ref}
        role="status"
        aria-label="状態"
        className="flex shrink-0 flex-col gap-1 border-y border-line bg-surface px-3 py-1 text-[11px]"
      >
        {messages.map((m) => (
          <div key={m.key}>
            <p className="m-0 text-warning">{m.text}</p>
            {m.detail ? (
              <p className="m-0 font-mono text-[10px] break-all text-text-sub">{m.detail}</p>
            ) : null}
          </div>
        ))}
      </div>
    );
  },
);
