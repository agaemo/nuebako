import { useCallback, useEffect, useRef, useState } from "react";

/** 通知を出しておく時間。操作の失敗を読めるだけ残し、次の操作の邪魔にならないよう自動で消す。 */
export const NOTICE_MS = 4000;

/** 操作の失敗などを短時間だけ知らせる通知。続けて通知したときは、新しい方の時間で消す。 */
export function useNotice() {
  const [notice, setNotice] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const clear = useCallback(() => {
    clearTimeout(timer.current);
    setNotice(null);
  }, []);

  const notify = useCallback((message: string) => {
    clearTimeout(timer.current);
    setNotice(message);
    timer.current = setTimeout(() => setNotice(null), NOTICE_MS);
  }, []);

  useEffect(() => () => clearTimeout(timer.current), []);

  return { notice, notify, clear };
}
