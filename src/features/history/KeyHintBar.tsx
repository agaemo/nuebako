import { KeyHint } from "../../components/ui/KeyHint";

export interface FooterNote {
  text: string;
  /** error は操作の失敗、warning は結果の注意 */
  tone: "error" | "warning";
}

interface KeyHintBarProps {
  /** 選択行にフォーカスがある(このときだけピン留め・削除の案内を出す) */
  rowFocused: boolean;
  /** OS の識別子。削除のキー(macOS は Cmd+Backspace、それ以外は Ctrl+Backspace)の案内に使う */
  os: string;
  /** 操作の結果の通知。あるときは、キー案内の代わりに出す */
  note: FooterNote | null;
  /** 一覧を表示していない(鍵が使えない)とき、キー案内を出さない */
  listHidden: boolean;
  onOpenSettings: () => void;
}

/** フッター: キー案内と「設定」リンク。貼り付けや削除の結果は、通知として案内の代わりに出す。 */
export function KeyHintBar({ rowFocused, os, note, listHidden, onOpenSettings }: KeyHintBarProps) {
  return (
    <div className="flex h-[26px] shrink-0 items-center justify-between gap-2 border-t border-line px-3">
      {note ? (
        <span
          role="status"
          aria-label="操作の結果"
          className={`min-w-0 truncate text-[11px] ${note.tone === "error" ? "text-error" : "text-warning"}`}
        >
          {note.text}
        </span>
      ) : (
        <span className="flex min-w-0 items-center gap-2">
          {listHidden ? null : (
            <>
              <KeyHint keys="↑↓">選択</KeyHint>
              <KeyHint keys="Enter">貼り付け</KeyHint>
              <KeyHint keys="Tab">プレビュー</KeyHint>
              <KeyHint keys="Esc">閉じる</KeyHint>
              {rowFocused && (
                <>
                  <KeyHint keys="P">ピン留め</KeyHint>
                  <KeyHint keys={os === "macos" ? "Del / ⌘⌫" : "Del / Ctrl+⌫"}>削除</KeyHint>
                </>
              )}
            </>
          )}
        </span>
      )}
      <button
        type="button"
        onClick={onOpenSettings}
        className="shrink-0 rounded-sm px-1 text-[11px] text-text-sub hover:text-text"
      >
        設定
      </button>
    </div>
  );
}
