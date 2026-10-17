import { useEffect, useState } from "react";
import { KIND_LABELS } from "../../components/ui/KindMark";
import { Badge } from "../../components/ui/Badge";
import { formatClipDateTime } from "../../lib/format";
import { getClipDetail } from "../../lib/ipc";
import type { ClipDetail } from "../../types";

/** 折りたたみのプレビュー(高さ 96px。画像のときは 240px)。選択中の履歴の全文または画像と、メタ情報を表示する。 */
export function PreviewPane({ clipId, tall = false }: { clipId: number | null; tall?: boolean }) {
  const [detail, setDetail] = useState<ClipDetail | null>(null);

  useEffect(() => {
    if (clipId === null) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    getClipDetail(clipId)
      .then((d) => {
        if (!cancelled) setDetail(d);
      })
      .catch(() => {
        if (!cancelled) setDetail(null);
      });
    return () => {
      cancelled = true;
    };
  }, [clipId]);

  const current = detail && detail.id === clipId ? detail : null;

  return (
    <section
      aria-label="プレビュー"
      className={`flex shrink-0 flex-col gap-1 border-t border-line bg-surface px-3 py-1 ${tall ? "h-60" : "h-24"}`}
    >
      {current ? (
        <>
          <div className="flex items-center gap-2 text-[11px] text-text-sub">
            <span className="text-text">{KIND_LABELS[current.kind]}</span>
            {current.source_app_name ? <Badge>{current.source_app_name}</Badge> : null}
            <span className="font-mono text-[10px]">
              {formatClipDateTime(current.copied_at_ms)}
            </span>
            <span className="ml-auto font-mono text-[10px]">{current.char_count} 文字</span>
          </div>
          {current.image_png ? (
            <img
              alt="画像のプレビュー"
              src={`data:image/png;base64,${current.image_png}`}
              className="min-h-0 w-full flex-1 object-contain"
            />
          ) : (
            <pre className="m-0 min-h-0 flex-1 overflow-auto font-body text-[12px] break-all whitespace-pre-wrap">
              {current.text}
            </pre>
          )}
        </>
      ) : null}
    </section>
  );
}
