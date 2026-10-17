import { forwardRef } from "react";
import { KIND_LABELS, KindMark } from "../../components/ui/KindMark";
import { formatClipTime } from "../../lib/format";
import type { ClipSummary } from "../../types";

interface HistoryItemProps {
  clip: ClipSummary;
  /** 行の位置(0 始まり)。番号は先頭の10行だけに付き、1〜9、10行目は 0 */
  index: number;
  selected: boolean;
  nowMs: number;
  previewOpen: boolean;
  onPaste: () => void;
  onTogglePreview: () => void;
}

/** 番号で貼り付けられる行数(キーが 1〜9 と 0 の10個のため) */
const NUMBERED_ROWS = 10;

/** 行頭の番号の表示(1〜9、10行目は 0)。11行目以降は番号キーで選べないので空にする。 */
export const rowNumber = (index: number) =>
  index >= NUMBERED_ROWS ? "" : index === 9 ? "0" : String(index + 1);

/** 履歴の1行(高さ 24px)。履歴の内容は React のテキストとしてだけ描画する。 */
export const HistoryItem = forwardRef<HTMLDivElement, HistoryItemProps>(function HistoryItem(
  { clip, index, selected, nowMs, previewOpen, onPaste, onTogglePreview },
  ref,
) {
  return (
    <div
      ref={ref}
      role="option"
      aria-selected={selected}
      tabIndex={-1}
      onClick={onPaste}
      className={`flex h-6 cursor-default items-center gap-2 rounded-sm px-2 outline-offset-[-2px] ${
        selected ? "bg-select" : "hover:bg-surface"
      }`}
    >
      <span className="flex h-[18px] shrink-0">
        <KindMark kind={clip.kind} />
      </span>
      {clip.pinned ? (
        <span
          role="img"
          aria-label="ピン留め"
          title="ピン留め"
          className="inline-block size-2 shrink-0 rounded-full bg-accent"
        />
      ) : (
        <span className="w-2 shrink-0 text-center font-mono text-[10px] text-text-sub">
          {rowNumber(index)}
        </span>
      )}
      {clip.kind === "image" && clip.thumb_png ? (
        <img
          alt=""
          src={`data:image/png;base64,${clip.thumb_png}`}
          className="size-[18px] shrink-0 rounded-[2px] object-cover"
        />
      ) : null}
      <span className="min-w-0 flex-1 truncate text-[12px]">
        {clip.kind === "image" ? `(${KIND_LABELS.image})` : clip.first_line}
      </span>
      {clip.extra_lines > 0 ? (
        <span
          className={`shrink-0 font-mono text-[10px] ${selected ? "text-select-text" : "text-text-sub"}`}
        >
          +{clip.extra_lines}行
        </span>
      ) : null}
      {selected && (
        <button
          type="button"
          aria-label="プレビューを開閉"
          aria-expanded={previewOpen}
          // クリックでフォーカスを奪わない(検索欄・行のフォーカスを保つ)
          onMouseDown={(e) => e.preventDefault()}
          onClick={(e) => {
            e.stopPropagation();
            onTogglePreview();
          }}
          className="shrink-0 rounded-sm px-1 font-mono text-[10px] text-select-text hover:bg-bg/40"
        >
          {previewOpen ? "▴" : "▾"}
        </button>
      )}
      <span
        className={`shrink-0 font-mono text-[10px] ${selected ? "text-select-text" : "text-text-sub"}`}
      >
        {formatClipTime(clip.copied_at_ms, nowMs)}
      </span>
    </div>
  );
});
