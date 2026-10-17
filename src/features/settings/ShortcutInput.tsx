import { useState } from "react";
import { toAccelerator } from "../../lib/keys";
import { resumeShortcut, suspendShortcut } from "../../lib/ipc";
import { Button } from "../../components/ui/Button";

interface ShortcutInputProps {
  /** 現在のショートカット(例: `CommandOrControl+Shift+V`) */
  value: string;
  /** 新しいショートカットを登録する。失敗したときはエラーメッセージ、成功したときは null を返す */
  onCommit: (accelerator: string) => Promise<string | null>;
}

/** 表示用に整える(CommandOrControl は OS に依らず Cmd/Ctrl と書く)。 */
function format(accelerator: string): string {
  return accelerator.replace("CommandOrControl", "Cmd/Ctrl").replace(/\+/g, " + ");
}

/** 「キーを押して設定」する欄。クリックで入力待ちになり、修飾キー付きのキーを押すと登録を試みる。Esc で取り消す。 */
export function ShortcutInput({ value, onCommit }: ShortcutInputProps) {
  const [listening, setListening] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /** 入力待ちを終える。入力待ちの間に解除していたショートカットを戻す(失敗しても画面は止めない)。 */
  const stopListening = () => {
    setListening(false);
    void resumeShortcut().catch(() => {});
  };

  /** 入力待ちを始める。押したキーが、グローバルショートカットに奪われないよう、いまのショートカットを解除する。 */
  const startListening = () => {
    setError(null);
    setListening(true);
    void suspendShortcut().catch(() => {});
  };

  const onKeyDown = async (e: React.KeyboardEvent) => {
    if (!listening) return;
    e.preventDefault();
    // Esc は取り消しで、ウィンドウを閉じる操作にはしない
    e.stopPropagation();
    if (e.key === "Escape") {
      stopListening();
      return;
    }
    if (saving) return;
    const accelerator = toAccelerator(e.nativeEvent);
    if (accelerator === null) return; // 修飾キーだけ、または修飾キーなしは待ち続ける
    setSaving(true);
    setListening(false);
    // 確定の前に、解除していたショートカットを戻す(同じ組み合わせを選んだときも、元の状態に戻る)
    try {
      await resumeShortcut();
    } catch {
      // 戻せなくても、続けて登録を試みる
    }
    const message = await onCommit(accelerator);
    setError(message);
    setSaving(false);
  };

  return (
    <div className="flex flex-col items-end gap-1">
      <Button
        aria-label="呼び出しショートカット"
        busy={saving}
        onClick={startListening}
        onKeyDown={onKeyDown}
        onBlur={() => {
          if (listening) stopListening();
        }}
        className="min-w-36 font-mono"
      >
        {listening ? "キーを押してください" : format(value)}
      </Button>
      {error && (
        <p role="alert" className="m-0 max-w-64 text-right text-[11px] text-error">
          {error}
        </p>
      )}
    </div>
  );
}
