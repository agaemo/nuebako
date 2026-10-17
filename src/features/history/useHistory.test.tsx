import { act, renderHook, waitFor } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ClipPage, ClipSummary } from "../../types";
import { nextSelectionAfterDelete, useHistory } from "./useHistory";

const clip = (id: number, over: Partial<ClipSummary> = {}): ClipSummary => ({
  id,
  kind: "text",
  first_line: `item-${id}`,
  copied_at_ms: 0,
  pinned: false,
  source_app_name: null,
  char_count: 6,
  thumb_png: null,
  ...over,
});

/** 解放を呼ぶまで応答しない Promise */
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("useHistory", () => {
  it("マウント時に一覧を取得して items と total を返す", async () => {
    mockIPC((cmd, args) => {
      if (cmd === "list_clips") {
        const a = args as { reqId: number };
        return { req_id: a.reqId, total: 5, items: [clip(1), clip(2)] } satisfies ClipPage;
      }
    });
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.items.length).toBe(2));
    expect(result.current.total).toBe(5);
  });

  it("古い req_id の応答は捨てて、最新の検索の結果だけを表示する", async () => {
    const slow = deferred<ClipPage>();
    mockIPC((cmd, args) => {
      if (cmd !== "list_clips") return;
      const a = args as { query: string; reqId: number };
      if (a.query === "a") return slow.promise.then((p) => ({ ...p, req_id: a.reqId }));
      return { req_id: a.reqId, total: 9, items: [clip(2, { first_line: `q:${a.query}` })] };
    });
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.loaded).toBe(true));
    act(() => result.current.setQuery("a"));
    act(() => result.current.setQuery("ab"));
    await waitFor(() => expect(result.current.items[0]?.first_line).toBe("q:ab"));
    // 先に出した検索 "a" の応答が後から届いても、上書きしない
    await act(async () => {
      slow.resolve({ req_id: 0, total: 1, items: [clip(1, { first_line: "stale" })] });
    });
    expect(result.current.items[0]?.first_line).toBe("q:ab");
  });

  it("応答の req_id が送った値と違うとき、その応答を捨てる", async () => {
    mockIPC((cmd) => {
      if (cmd === "list_clips") return { req_id: 99999, total: 1, items: [clip(1)] };
    });
    const { result } = renderHook(() => useHistory());
    await new Promise((r) => setTimeout(r, 30));
    expect(result.current.items.length).toBe(0);
  });

  it("history-changed を受けたとき一覧を取り直す", async () => {
    let n = 0;
    mockIPC(
      (cmd, args) => {
        if (cmd === "list_clips") {
          n++;
          return { req_id: (args as { reqId: number }).reqId, total: n, items: [clip(n)] };
        }
      },
      { shouldMockEvents: true },
    );
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.total).toBe(1));
    await act(async () => {
      await emit("history-changed", null);
    });
    await waitFor(() => expect(result.current.total).toBe(2));
  });

  it("変更の実行中に届いた一覧の応答では上書きせず、完了後に取り直す", async () => {
    const del = deferred<null>();
    let items = [clip(1), clip(2)];
    mockIPC(
      (cmd, args) => {
        if (cmd === "list_clips") {
          return { req_id: (args as { reqId: number }).reqId, total: items.length, items };
        }
        if (cmd === "delete_clip") {
          return del.promise.then(() => {
            items = [clip(2)];
            return null;
          });
        }
      },
      { shouldMockEvents: true },
    );
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.items.length).toBe(2));

    let done!: Promise<void>;
    act(() => {
      done = result.current.remove(1);
    });
    // 削除の実行中に history-changed が届き、古い内容(まだ2件)の応答が返る
    await act(async () => {
      await emit("history-changed", null);
      await new Promise((r) => setTimeout(r, 20));
    });
    expect(result.current.items.length).toBe(2);
    expect(result.current.mutating).toBe(true);

    await act(async () => {
      del.resolve(null);
      await done;
    });
    await waitFor(() => expect(result.current.items.map((i) => i.id)).toEqual([2]));
    expect(result.current.mutating).toBe(false);
  });

  it("変更が失敗しても mutating を戻し、一覧を取り直す", async () => {
    let lists = 0;
    mockIPC((cmd, args) => {
      if (cmd === "list_clips") {
        lists++;
        return { req_id: (args as { reqId: number }).reqId, total: 1, items: [clip(1)] };
      }
      if (cmd === "set_pinned") throw { code: "not_found", message: "x" };
    });
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.loaded).toBe(true));
    const before = lists;
    await act(async () => {
      await result.current.togglePin(1, true).catch(() => {});
    });
    expect(result.current.mutating).toBe(false);
    expect(lists).toBeGreaterThan(before);
  });

  it("検索語は await をまたいでも最新の値で取得する(ref で読む)", async () => {
    const queries: string[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "list_clips") {
        const a = args as { query: string; reqId: number };
        queries.push(a.query);
        return { req_id: a.reqId, total: 0, items: [] };
      }
    });
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.loaded).toBe(true));
    act(() => result.current.setQuery("zz"));
    await act(async () => {
      await result.current.refresh();
    });
    expect(queries[queries.length - 1]).toBe("zz");
  });

  it("応答を受けるたびに nowMs を取り直す(0時をまたいだ表示の更新)", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 2, 1, 23, 59).getTime());
    mockIPC((cmd, args) => {
      if (cmd === "list_clips") {
        return { req_id: (args as { reqId: number }).reqId, total: 0, items: [] };
      }
    });
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.loaded).toBe(true));
    expect(new Date(result.current.nowMs).getDate()).toBe(1);
    vi.setSystemTime(new Date(2026, 2, 2, 0, 0).getTime());
    await act(async () => {
      await result.current.refresh();
    });
    expect(new Date(result.current.nowMs).getDate()).toBe(2);
  });
});

describe("nextSelectionAfterDelete", () => {
  const items = [clip(1), clip(2), clip(3)];
  it("中間の行を削除したとき次の行の id を返す", () => {
    expect(nextSelectionAfterDelete(items, 2)).toBe(3);
  });
  it("最後の行を削除したとき前の行の id を返す", () => {
    expect(nextSelectionAfterDelete(items, 3)).toBe(2);
  });
  it("1件だけの行を削除したとき null(検索欄へ)を返す", () => {
    expect(nextSelectionAfterDelete([clip(1)], 1)).toBeNull();
  });
  it("一覧にない id のとき null を返す", () => {
    expect(nextSelectionAfterDelete(items, 9)).toBeNull();
  });
});
