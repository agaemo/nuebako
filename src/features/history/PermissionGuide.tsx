import { useEffect, useRef } from "react";
import { Button } from "../../components/ui/Button";

interface PermissionGuideProps {
  busy: boolean;
  onOpenSettings: () => void;
  onLater: () => void;
  onClose: () => void;
}

/**
 * S4 権限の案内ダイアログ(macOS)。
 * 「あとで(再コピーのみ)」は、再コピーだけ実行してウィンドウを閉じる。Esc は案内だけを閉じる。
 */
export function PermissionGuide({ busy, onOpenSettings, onLater, onClose }: PermissionGuideProps) {
  const openRef = useRef<HTMLButtonElement>(null);
  const laterRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    openRef.current?.focus();
  }, []);

  const onKeyDown = (e: React.KeyboardEvent) => {
    // 履歴ウィンドウ側のキー操作(Enter での貼り付けなど)に渡さない
    e.stopPropagation();
    if (e.key === "Escape") {
      if (!busy) onClose();
    } else if (e.key === "Tab") {
      const active = document.activeElement;
      if (e.shiftKey && active === openRef.current) {
        e.preventDefault();
        laterRef.current?.focus();
      } else if (!e.shiftKey && active === laterRef.current) {
        e.preventDefault();
        openRef.current?.focus();
      }
    }
  };

  return (
    <div
      className="fixed inset-0 z-10 flex items-center justify-center bg-black/40"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !busy) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="guide-title"
        aria-describedby="guide-message"
        onKeyDown={onKeyDown}
        className="w-[340px] rounded-lg border border-line bg-bg p-4 shadow-window"
      >
        <h2 id="guide-title" className="mb-1 text-[13px] font-bold">
          自動で貼り付けるには、アクセシビリティの権限が必要です
        </h2>
        <p id="guide-message" className="mb-3 text-text-sub">
          システム設定で nuebako
          を許可すると、選んだ履歴を直前のアプリへ自動で貼り付けられます。「あとで」を選ぶと、内容のコピーだけを行います。
        </p>
        <div className="flex justify-end gap-2">
          <Button ref={laterRef} busy={busy} onClick={onLater}>
            あとで(再コピーのみ)
          </Button>
          <Button ref={openRef} variant="primary" busy={busy} onClick={onOpenSettings}>
            システム設定を開く
          </Button>
        </div>
      </div>
    </div>
  );
}
