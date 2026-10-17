import { KeyHint } from "../../components/ui/KeyHint";

interface KeyHintBarProps {
  /** 選択行にフォーカスがある(このときだけピン留め・削除の案内を出す) */
  rowFocused: boolean;
  /** 自動貼り付けの権限がなく、再コピーのみになる */
  noPermissionNote: boolean;
  /** 貼り付けのキー送信に失敗した(クリップボードには設定済み) */
  sendFailedNote: boolean;
  /** 一覧を表示していない(鍵が使えない)とき、キー案内を出さない */
  listHidden: boolean;
  onOpenSettings: () => void;
}

/** フッター: キー案内と「設定」リンク。権限の案内は、結果の理由に応じて出し分ける。 */
export function KeyHintBar({
  rowFocused,
  noPermissionNote,
  sendFailedNote,
  listHidden,
  onOpenSettings,
}: KeyHintBarProps) {
  return (
    <div className="flex h-[26px] shrink-0 items-center justify-between gap-2 border-t border-line px-3">
      {noPermissionNote || sendFailedNote ? (
        <span role="status" className="min-w-0 truncate text-[11px] text-warning">
          {noPermissionNote
            ? "自動貼り付けには権限が必要です(再コピーのみ)"
            : "貼り付けの送信に失敗しました(内容はコピー済みです)"}
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
                  <KeyHint keys="Del">削除</KeyHint>
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
