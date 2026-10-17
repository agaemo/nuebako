import { useCallback, useEffect, useRef, useState } from "react";
import { deleteClip, listClips, setPinned, toAppError } from "../../lib/ipc";
import { onHistoryChanged, onHistoryWindowShown } from "../../lib/events";
import { unlistenAll } from "./unlisten";
import type { ClipSummary } from "../../types";

/** 1回に取得する件数。一覧は10行分の領域で、スクロールで続きを読む。 */
export const PAGE_SIZE = 50;
/** list_clips が受け付ける limit の上限(Rust 側で丸められる値に合わせる) */
const MAX_LIMIT = 200;

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
 * - 取得のたびに req_id を進め、最新でない応答は捨てる(検索語の変更、取り直し、続きの読み込みのどれも同じ番号を使うので、互いに古い応答を上書きしない)
 * - 取り直しの応答が届くまでは stale になり、画面は旧一覧への操作を止める
 * - 変更系(削除・ピン留め)の実行中に届いた応答では state を上書きせず、完了後に取り直す
 * - await をまたぐ処理は、state ではなく ref から最新の値を読む
 */
export function useHistory() {
  const [items, setItems] = useState<ClipSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [matched, setMatched] = useState(0);
  const [query, setQueryState] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  const [mutating, setMutating] = useState(false);
  /** 応答を反映した時刻。1つの応答の時刻表示はこの値でまとめて描画する */
  const [nowMs, setNowMs] = useState(() => Date.now());

  const reqIdRef = useRef(0);
  const queryRef = useRef("");
  const mutatingRef = useRef(0);
  const itemsRef = useRef<ClipSummary[]>([]);
  const matchedRef = useRef(0);
  /** 取り直しの応答待ち。続きの読み込みは、この間は始めない */
  const awaitingRef = useRef(false);
  const loadingMoreRef = useRef(false);

  const apply = (next: ClipSummary[], nextTotal: number, nextMatched: number) => {
    itemsRef.current = next;
    matchedRef.current = nextMatched;
    setItems(next);
    setTotal(nextTotal);
    setMatched(nextMatched);
  };

  /**
   * 先頭から取り直す。検索語を変えたときは先頭の1ページだけ(reset)、
   * それ以外は読み込み済みの件数を保つ(スクロールした先の行が、更新のたびに消えないように)。
   */
  const refresh = useCallback(async (opts?: { reset?: boolean }) => {
    const reqId = ++reqIdRef.current;
    loadingMoreRef.current = false;
    awaitingRef.current = true;
    setStale(true);
    const limit = opts?.reset
      ? PAGE_SIZE
      : Math.min(MAX_LIMIT, Math.max(PAGE_SIZE, itemsRef.current.length));
    try {
      const page = await listClips(queryRef.current, reqId, limit, 0);
      if (page.req_id !== reqIdRef.current) return; // 古い応答
      if (mutatingRef.current > 0) return; // 変更の完了後に取り直す
      apply(page.items, page.total, page.matched);
      setNowMs(Date.now());
      setLoadError(null);
      awaitingRef.current = false;
      setStale(false);
      setLoaded(true);
    } catch (e) {
      // 取得失敗は一覧を変えず、理由を持つ(画面が「履歴がありません」と区別して出す)
      if (reqId === reqIdRef.current) {
        setLoadError(toAppError(e).message);
        awaitingRef.current = false;
        setStale(false);
        setLoaded(true);
      }
    }
  }, []);

  /** 読み込み済みの続きを取得して末尾に足す。一致件数に達していれば何もしない。 */
  const loadMore = useCallback(async () => {
    if (awaitingRef.current || loadingMoreRef.current || mutatingRef.current > 0) return;
    const offset = itemsRef.current.length;
    if (offset >= matchedRef.current) return;
    const reqId = ++reqIdRef.current;
    loadingMoreRef.current = true;
    try {
      const page = await listClips(queryRef.current, reqId, PAGE_SIZE, offset);
      if (page.req_id !== reqIdRef.current) return; // 検索語の変更や取り直しで、この続きは不要になった
      if (mutatingRef.current > 0) return;
      const known = new Set(itemsRef.current.map((c) => c.id));
      const added = page.items.filter((c) => !known.has(c.id));
      apply([...itemsRef.current, ...added], page.total, page.matched);
    } catch {
      // 続きの取得に失敗しても、表示済みの一覧はそのままにする(次のスクロールで再試行する)
    } finally {
      if (reqId === reqIdRef.current) loadingMoreRef.current = false;
    }
  }, []);

  const setQuery = useCallback(
    (q: string) => {
      queryRef.current = q;
      setQueryState(q);
      void refresh({ reset: true });
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
    total,
    matched,
    query,
    setQuery,
    loaded,
    loadError,
    stale,
    mutating,
    nowMs,
    refresh,
    loadMore,
    remove,
    togglePin,
  };
}
