import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NOTICE_MS, useNotice } from "./useNotice";

afterEach(() => {
  vi.useRealTimers();
});

describe("useNotice", () => {
  it("通知を出して、一定時間が過ぎたら消す", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useNotice());
    expect(result.current.notice).toBeNull();
    act(() => result.current.notify("失敗しました"));
    expect(result.current.notice).toBe("失敗しました");
    act(() => vi.advanceTimersByTime(NOTICE_MS - 1));
    expect(result.current.notice).toBe("失敗しました");
    act(() => vi.advanceTimersByTime(2));
    expect(result.current.notice).toBeNull();
  });

  it("続けて通知したとき、先の通知の時間切れで後の通知が消えない", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useNotice());
    act(() => result.current.notify("1つ目"));
    act(() => vi.advanceTimersByTime(NOTICE_MS - 100));
    act(() => result.current.notify("2つ目"));
    act(() => vi.advanceTimersByTime(200));
    expect(result.current.notice).toBe("2つ目");
    act(() => vi.advanceTimersByTime(NOTICE_MS));
    expect(result.current.notice).toBeNull();
  });
});
