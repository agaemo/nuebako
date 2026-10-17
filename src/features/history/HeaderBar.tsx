import { KIND_LABELS } from "../../components/ui/KindMark";
import type { ClipKind } from "../../types";

const MARK_KINDS: ClipKind[] = ["text", "url", "code", "path", "image"];
const MARK_COLORS: Record<ClipKind, string> = {
  text: "bg-kind-text",
  url: "bg-kind-url",
  code: "bg-kind-code",
  path: "bg-kind-path",
  image: "bg-kind-image",
};

interface HeaderBarProps {
  /** 表示している件数 */
  shown: number;
  /** 全件数 */
  total: number;
  /** 一覧を表示していない(鍵が使えない)とき、件数を出さない */
  hideCount?: boolean;
}

/** ブランド名(明朝体)、5色の継ぎ接ぎのマーク、「履歴 1〜10 件 / 全 N 件」。 */
export function HeaderBar({ shown, total, hideCount = false }: HeaderBarProps) {
  const range = shown === 0 ? "0 件" : shown === 1 ? "1 件" : `1〜${shown} 件`;
  return (
    <div className="flex h-7 shrink-0 items-center justify-between px-3">
      <span className="flex items-center font-display text-[14px] leading-[1.3] font-bold">
        nuebako
        <span className="ml-1.5 inline-flex gap-px" aria-hidden="true">
          {MARK_KINDS.map((k) => (
            <i
              key={k}
              title={KIND_LABELS[k]}
              className={`block h-3 w-1 rounded-[1px] ${MARK_COLORS[k]}`}
            />
          ))}
        </span>
      </span>
      {hideCount ? null : (
        <span className="font-mono text-[10px] text-text-sub" data-testid="count">
          履歴 {range} / 全 {total} 件
        </span>
      )}
    </div>
  );
}
