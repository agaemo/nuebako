import { act, renderHook, waitFor } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it } from "vitest";
import type { AppStatus, PasteOutcome } from "../../types";
import { usePaste } from "./usePaste";

const status = (over: Partial<AppStatus> = {}): AppStatus => ({
  os: "macos",
  version: "0.1.0",
  storage: "ok",
  storage_message: null,
  paste_permission: true,
  permission_guide_dismissed: false,
  ...over,
});

interface Calls {
  paste: Array<{ id: number; mode: string }>;
  settings: unknown[];
  openedAccessibility: number;
}

function setup(s: AppStatus, outcome: PasteOutcome = { result: "pasted", reason: null }) {
  const calls: Calls = { paste: [], settings: [], openedAccessibility: 0 };
  let release: (() => void) | null = null;
  let hold = false;
  mockIPC((cmd, args) => {
    switch (cmd) {
      case "get_app_status":
        return s;
      case "paste_clip": {
        calls.paste.push(args as { id: number; mode: string });
        if (hold) return new Promise((r) => (release = () => r(outcome)));
        return outcome;
      }
      case "update_settings":
        calls.settings.push(args);
        return {};
      case "open_accessibility_settings":
        calls.openedAccessibility++;
        return null;
    }
  });
  return {
    calls,
    holdPaste: () => (hold = true),
    releasePaste: () => release?.(),
  };
}

describe("usePaste", () => {
  it("権限があるとき paste_clip(id, auto) を呼ぶ", async () => {
    const t = setup(status());
    const { result } = renderHook(() => usePaste());
    await waitFor(() => expect(result.current.statusLoaded).toBe(true));
    await act(async () => {
      await result.current.paste(7);
    });
    expect(t.calls.paste).toEqual([{ id: 7, mode: "auto" }]);
    expect(result.current.guideFor).toBeNull();
    expect(result.current.noPermissionNote).toBe(false);
  });

  it("権限なしで案内が未表示のとき、貼り付けずに案内(S4)を出す", async () => {
    const t = setup(status({ paste_permission: false }));
    const { result } = renderHook(() => usePaste());
    await waitFor(() => expect(result.current.statusLoaded).toBe(true));
    await act(async () => {
      await result.current.paste(7);
    });
    expect(t.calls.paste).toEqual([]);
    expect(result.current.guideFor).toBe(7);
  });

  it("案内の「あとで」のとき、非表示を保存して copy_only で貼り付ける", async () => {
    const t = setup(status({ paste_permission: false }), {
      result: "copied_only",
      reason: "no_permission",
    });
    const { result } = renderHook(() => usePaste());
    await waitFor(() => expect(result.current.statusLoaded).toBe(true));
    await act(async () => {
      await result.current.paste(7);
    });
    await act(async () => {
      await result.current.guideLater();
    });
    expect(t.calls.settings).toEqual([{ patch: { permission_guide_dismissed: true } }]);
    expect(t.calls.paste).toEqual([{ id: 7, mode: "copy_only" }]);
    expect(result.current.guideFor).toBeNull();
  });

  it("案内を非表示にした後、権限なしのときは案内を出さず auto で呼び、フッターの案内を出す", async () => {
    const t = setup(status({ paste_permission: false, permission_guide_dismissed: true }), {
      result: "copied_only",
      reason: "no_permission",
    });
    const { result } = renderHook(() => usePaste());
    await waitFor(() => expect(result.current.noPermissionNote).toBe(true));
    await act(async () => {
      await result.current.paste(3);
    });
    expect(result.current.guideFor).toBeNull();
    expect(t.calls.paste).toEqual([{ id: 3, mode: "auto" }]);
  });

  it("結果の reason が no_permission のとき、フッターの案内を出す", async () => {
    setup(status(), { result: "copied_only", reason: "no_permission" });
    const { result } = renderHook(() => usePaste());
    await waitFor(() => expect(result.current.statusLoaded).toBe(true));
    expect(result.current.noPermissionNote).toBe(false);
    await act(async () => {
      await result.current.paste(1);
    });
    expect(result.current.noPermissionNote).toBe(true);
    expect(result.current.sendFailedNote).toBe(false);
  });

  it("結果の reason が send_failed のとき、別の案内を出す", async () => {
    setup(status(), { result: "copied_only", reason: "send_failed" });
    const { result } = renderHook(() => usePaste());
    await waitFor(() => expect(result.current.statusLoaded).toBe(true));
    await act(async () => {
      await result.current.paste(1);
    });
    expect(result.current.sendFailedNote).toBe(true);
    expect(result.current.noPermissionNote).toBe(false);
  });

  it("貼り付け中の2回目の呼び出し(Enter 連打)は無視する", async () => {
    const t = setup(status());
    t.holdPaste();
    const { result } = renderHook(() => usePaste());
    await waitFor(() => expect(result.current.statusLoaded).toBe(true));
    let first!: Promise<void>;
    act(() => {
      first = result.current.paste(1);
    });
    await waitFor(() => expect(result.current.busy).toBe(true));
    await act(async () => {
      await result.current.paste(1);
      await result.current.paste(1);
    });
    expect(t.calls.paste.length).toBe(1);
    await act(async () => {
      t.releasePaste();
      await first;
    });
    expect(result.current.busy).toBe(false);
  });

  it("案内の「システム設定を開く」のとき open_accessibility_settings を呼び、案内は閉じない", async () => {
    const t = setup(status({ paste_permission: false }));
    const { result } = renderHook(() => usePaste());
    await waitFor(() => expect(result.current.statusLoaded).toBe(true));
    await act(async () => {
      await result.current.paste(7);
    });
    await act(async () => {
      await result.current.openSystemSettings();
    });
    expect(t.calls.openedAccessibility).toBe(1);
    expect(result.current.guideFor).toBe(7);
  });

  it("案内を閉じる(Esc)とき貼り付けず、案内だけ閉じる", async () => {
    const t = setup(status({ paste_permission: false }));
    const { result } = renderHook(() => usePaste());
    await waitFor(() => expect(result.current.statusLoaded).toBe(true));
    await act(async () => {
      await result.current.paste(7);
    });
    act(() => result.current.closeGuide());
    expect(result.current.guideFor).toBeNull();
    expect(t.calls.paste).toEqual([]);
  });
});
