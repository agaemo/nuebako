import { useCallback, useEffect, useRef, useState } from "react";
import { deleteClip, listClips, setPinned } from "../../lib/ipc";
import { onHistoryChanged, onHistoryWindowShown } from "../../lib/events";
import { unlistenAll } from "./unlisten";
import type { ClipSummary } from "../../types";

/**
 * 削除したあとにフォーカス・選択する行の id。次の行、無ければ前の行、0件なら null(検索欄)。
 */
export function nextSelectionAfterDelete(items: ClipSummary[], deletedId: number): number | null {
  const i = items.findIndex((c) => c.id === deletedId);
  if (i < 0) return null;
  return items[i + 1]?.id ?? items[i - 1]?.id ?? null;
}

/**
 * 履歴一覧の取得と更新。
 * - 検索のたびに req_id を進め、古い応答は捨てる
 * - 変更系(削除・ピン留め)の実行中に届いた応答では state を上書きせず、完了後に取り直す
 * - await をまたぐ処理は、state ではなく ref から最新の値を読む
 */
export function useHistory() {
  const [items, setItems] = useState<ClipSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [query, setQueryState] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [mutating, setMutating] = useState(false);
  /** 応答を反映した時刻。1つの応答の時刻表示はこの値でまとめて描画する */
  const [nowMs, setNowMs] = useState(() => Date.now());

  const reqIdRef = useRef(0);
  const queryRef = useRef("");
  const mutatingRef = useRef(0);
  const itemsRef = useRef<ClipSummary[]>([]);

  const refresh = useCallback(async () => {
    const reqId = ++reqIdRef.current;
    try {
      const page = await listClips(queryRef.current, reqId);
      if (page.req_id !== reqIdRef.current) return; // 古い応答
      if (mutatingRef.current > 0) return; // 変更の完了後に取り直す
      itemsRef.current = page.items;
      setItems(page.items);
      setTotal(page.total);
      setNowMs(Date.now());
      setLoaded(true);
    } catch {
      // 取得失敗は一覧を変えない(鍵が使えないときなどは、画面側が状態を別に表示する)
      if (reqId === reqIdRef.current) setLoaded(true);
    }
  }, []);

  const setQuery = useCallback(
    (q: string) => {
      queryRef.current = q;
      setQueryState(q);
      void refresh();
    },
    [refresh],
  );

  /** 変更系の実行。実行中は一覧の応答を反映せず、終わったら(失敗しても)取り直す。 */
  const mutate = useCallback(
    async (fn: () => Promise<unknown>) => {
      mutatingRef.current++;
      setMutating(true);
      try {
        await fn();
      } finally {
        mutatingRef.current--;
        if (mutatingRef.current === 0) setMutating(false);
        await refresh();
      }
    },
    [refresh],
  );

  const remove = useCallback((id: number) => mutate(() => deleteClip(id)), [mutate]);
  const togglePin = useCallback(
    (id: number, pinned: boolean) => mutate(() => setPinned(id, pinned)),
    [mutate],
  );

  useEffect(() => {
    void refresh();
    const unlisten = [
      onHistoryChanged(() => void refresh()),
      onHistoryWindowShown(() => void refresh()),
    ];
    return () => {
      unlistenAll(unlisten);
    };
  }, [refresh]);

  return {
    items,
    itemsRef,
    total,
    query,
    setQuery,
    loaded,
    mutating,
    nowMs,
    refresh,
    remove,
    togglePin,
  };
}
