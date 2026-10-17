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
  extra_lines: 0,
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
        return {
          req_id: a.reqId,
          total: 5,
          matched: 5,
          items: [clip(1), clip(2)],
        } satisfies ClipPage;
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
      return {
        req_id: a.reqId,
        total: 9,
        matched: 9,
        items: [clip(2, { first_line: `q:${a.query}` })],
      };
    });
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.loaded).toBe(true));
    act(() => result.current.setQuery("a"));
    act(() => result.current.setQuery("ab"));
    await waitFor(() => expect(result.current.items[0]?.first_line).toBe("q:ab"));
    // 先に出した検索 "a" の応答が後から届いても、上書きしない
    await act(async () => {
      slow.resolve({ req_id: 0, total: 1, matched: 1, items: [clip(1, { first_line: "stale" })] });
    });
    expect(result.current.items[0]?.first_line).toBe("q:ab");
  });

  it("応答の req_id が送った値と違うとき、その応答を捨てる", async () => {
    mockIPC((cmd) => {
      if (cmd === "list_clips") return { req_id: 99999, total: 1, matched: 1, items: [clip(1)] };
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
          return {
            req_id: (args as { reqId: number }).reqId,
            total: n,
            matched: n,
            items: [clip(n)],
          };
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
          return {
            req_id: (args as { reqId: number }).reqId,
            total: items.length,
            matched: items.length,
            items,
          };
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
        return {
          req_id: (args as { reqId: number }).reqId,
          total: 1,
          matched: 1,
          items: [clip(1)],
        };
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

  it("検索語を変えたあとに refresh を呼ぶと、最新の検索語で取得する", async () => {
    const queries: string[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "list_clips") {
        const a = args as { query: string; reqId: number };
        queries.push(a.query);
        return { req_id: a.reqId, total: 0, matched: 0, items: [] };
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
        return { req_id: (args as { reqId: number }).reqId, total: 0, matched: 0, items: [] };
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

describe("useHistory 続きの読み込み", () => {
  const all = Array.from({ length: 130 }, (_, i) => clip(i + 1));
  const backend = (log: Array<{ query: string; limit: number; offset: number }> = []) =>
    mockIPC((cmd, args) => {
      if (cmd !== "list_clips") return;
      const a = args as { query: string; reqId: number; limit: number; offset: number };
      log.push({ query: a.query, limit: a.limit, offset: a.offset });
      const hit = all.filter((c) => c.first_line.includes(a.query));
      return {
        req_id: a.reqId,
        total: all.length,
        matched: hit.length,
        items: hit.slice(a.offset, a.offset + a.limit),
      } satisfies ClipPage;
    });

  it("最初は limit 50, offset 0 で取得し、matched を返す", async () => {
    const log: Array<{ query: string; limit: number; offset: number }> = [];
    backend(log);
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.items.length).toBe(50));
    expect(log[0]).toEqual({ query: "", limit: 50, offset: 0 });
    expect(result.current.matched).toBe(130);
  });

  it("loadMore は読み込み済みの件数を offset にして続きを追加する", async () => {
    const log: Array<{ query: string; limit: number; offset: number }> = [];
    backend(log);
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.items.length).toBe(50));
    await act(async () => {
      await result.current.loadMore();
    });
    expect(log[1]).toEqual({ query: "", limit: 50, offset: 50 });
    expect(result.current.items.map((c) => c.id)).toEqual(
      Array.from({ length: 100 }, (_, i) => i + 1),
    );
  });

  it("全件を読み込んだあとの loadMore は何も取得しない", async () => {
    const log: Array<{ query: string; limit: number; offset: number }> = [];
    backend(log);
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.items.length).toBe(50));
    for (let i = 0; i < 3; i++)
      await act(async () => {
        await result.current.loadMore();
      });
    expect(result.current.items.length).toBe(130);
    const n = log.length;
    await act(async () => {
      await result.current.loadMore();
    });
    expect(log.length).toBe(n);
  });

  it("続きの応答を待つ間に検索語が変わったら、その応答を捨てる", async () => {
    const slow = deferred<null>();
    mockIPC((cmd, args) => {
      if (cmd !== "list_clips") return;
      const a = args as { query: string; reqId: number; limit: number; offset: number };
      const respond = () => {
        const hit = all.filter((c) => c.first_line.includes(a.query));
        return {
          req_id: a.reqId,
          total: all.length,
          matched: hit.length,
          items: hit.slice(a.offset, a.offset + a.limit),
        };
      };
      return a.offset === 50 ? slow.promise.then(respond) : respond();
    });
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.items.length).toBe(50));
    let more!: Promise<void>;
    act(() => {
      more = result.current.loadMore();
    });
    act(() => result.current.setQuery("item-12"));
    await waitFor(() => expect(result.current.matched).toBe(11));
    await act(async () => {
      slow.resolve(null);
      await more;
    });
    expect(result.current.items.length).toBe(11);
    expect(result.current.items.every((c) => c.first_line.includes("item-12"))).toBe(true);
  });

  it("最初の取得の応答待ちの間は loadMore を呼んでも取得しない", async () => {
    const slow = deferred<null>();
    const log: number[] = [];
    mockIPC((cmd, args) => {
      if (cmd !== "list_clips") return;
      const a = args as { reqId: number; limit: number; offset: number };
      log.push(a.offset);
      return slow.promise.then(() => ({
        req_id: a.reqId,
        total: 130,
        matched: 130,
        items: all.slice(0, 50),
      }));
    });
    const { result } = renderHook(() => useHistory());
    await act(async () => {
      await result.current.loadMore();
    });
    expect(log).toEqual([0]);
    await act(async () => {
      slow.resolve(null);
    });
  });

  it("読み込み済みの件数を保ったまま取り直す(history-changed でスクロール位置の先が消えない)", async () => {
    const log: Array<{ query: string; limit: number; offset: number }> = [];
    backend(log);
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.items.length).toBe(50));
    await act(async () => {
      await result.current.loadMore();
    });
    await act(async () => {
      await result.current.refresh();
    });
    expect(log[log.length - 1]).toEqual({ query: "", limit: 100, offset: 0 });
    expect(result.current.items.length).toBe(100);
  });

  it("応答待ちの間は stale が true で、最新の応答を反映すると false になる", async () => {
    const slow = deferred<null>();
    mockIPC((cmd, args) => {
      if (cmd !== "list_clips") return;
      const a = args as { query: string; reqId: number };
      const respond = () => ({ req_id: a.reqId, total: 1, matched: 1, items: [clip(1)] });
      return a.query === "x" ? slow.promise.then(respond) : respond();
    });
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.loaded).toBe(true));
    expect(result.current.stale).toBe(false);
    act(() => result.current.setQuery("x"));
    expect(result.current.stale).toBe(true);
    await act(async () => {
      slow.resolve(null);
    });
    await waitFor(() => expect(result.current.stale).toBe(false));
  });

  it("取得に失敗したとき loadError を持ち、成功したら消す", async () => {
    let fail = true;
    mockIPC((cmd, args) => {
      if (cmd !== "list_clips") return;
      if (fail) throw { code: "internal", message: "boom" };
      return { req_id: (args as { reqId: number }).reqId, total: 1, matched: 1, items: [clip(1)] };
    });
    const { result } = renderHook(() => useHistory());
    await waitFor(() => expect(result.current.loadError).not.toBeNull());
    expect(result.current.stale).toBe(false);
    fail = false;
    await act(async () => {
      await result.current.refresh();
    });
    expect(result.current.loadError).toBeNull();
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
