import { useEffect, useRef } from "react";
import { Button } from "./Button";

interface ConfirmDialogProps {
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel?: string;
  /** 処理中。Esc と背景のクリックでは閉じず、ボタンは押しても何も起きない */
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * 確認ダイアログ。初期フォーカスはキャンセル(誤操作で実行しないため)。
 * Tab はダイアログの中だけを循環する。
 */
export function ConfirmDialog({
  title,
  message,
  confirmLabel,
  cancelLabel = "キャンセル",
  busy = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    cancelRef.current?.focus();
  }, []);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      if (!busy) onCancel();
      return;
    }
    if (e.key === "Tab") {
      // 2つのボタンの間だけを循環させる
      const active = document.activeElement;
      if (e.shiftKey && active === cancelRef.current) {
        e.preventDefault();
        confirmRef.current?.focus();
      } else if (!e.shiftKey && active === confirmRef.current) {
        e.preventDefault();
        cancelRef.current?.focus();
      }
    }
  };

  return (
    <div
      className="fixed inset-0 z-10 flex items-center justify-center bg-black/40"
      data-testid="dialog-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !busy) onCancel();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        aria-describedby="confirm-message"
        onKeyDown={onKeyDown}
        className="w-[340px] rounded-lg border border-line bg-bg p-4 shadow-window"
      >
        <h2 id="confirm-title" className="mb-1 text-[13px] font-bold">
          {title}
        </h2>
        <p id="confirm-message" className="mb-3 text-text-sub">
          {message}
        </p>
        <div className="flex justify-end gap-2">
          <Button ref={cancelRef} busy={busy} onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button ref={confirmRef} variant="danger" busy={busy} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
