import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppStatus, ClipDetail, ClipSummary, PasteOutcome, Settings } from "../../types";
import { HistoryWindow } from "./HistoryWindow";
import { windowHeight } from "./windowHeight";

const clip = (id: number, over: Partial<ClipSummary> = {}): ClipSummary => ({
  id,
  kind: "text",
  first_line: `item-${id}`,
  extra_lines: 0,
  copied_at_ms: new Date(2026, 4, 10, 9, id).getTime(),
  pinned: false,
  source_app_name: null,
  char_count: 6,
  thumb_png: null,
  ...over,
});

const baseStatus: AppStatus = {
  os: "macos",
  version: "0.1.0",
  storage: "ok",
  storage_message: null,
  capture: "ok",
  settings_recovered: false,
  paste_permission: true,
  permission_guide_dismissed: false,
};

interface Env {
  items: ClipSummary[];
  status: AppStatus;
  outcome: PasteOutcome;
  details: Record<number, ClipDetail>;
  calls: Array<{ cmd: string; args: unknown }>;
  /** コマンド名ごとに、呼ばれたら投げるエラー */
  fail: Record<string, unknown>;
  /** list_clips を差し替える(応答の遅延や順序の入れ替えを作る) */
  onList?: (a: Record<string, unknown>, fallback: () => unknown) => unknown;
  settings: Settings | null;
}

function setup(over: Partial<Env> = {}) {
  const env: Env = {
    items: [],
    status: baseStatus,
    outcome: { result: "pasted", reason: null },
    details: {},
    calls: [],
    fail: {},
    settings: null,
    ...over,
  };
  mockWindows("history");
  mockIPC(
    (cmd, args) => {
      env.calls.push({ cmd, args });
      const a = (args ?? {}) as Record<string, unknown>;
      if (cmd in env.fail) throw env.fail[cmd];
      switch (cmd) {
        case "get_settings":
          return env.settings;
        case "get_app_status":
          return env.status;
        case "list_clips": {
          const respond = () => {
            const q = String(a.query ?? "").toLowerCase();
            const hit = env.items.filter((c) => c.first_line.toLowerCase().includes(q));
            const offset = Number(a.offset ?? 0);
            const limit = Number(a.limit ?? 50);
            return {
              req_id: a.reqId,
              total: env.items.length,
              matched: hit.length,
              items: hit.slice(offset, offset + limit),
            };
          };
          return env.onList ? env.onList(a, respond) : respond();
        }
        case "get_clip_detail":
          return env.details[a.id as number];
        case "paste_clip":
          return env.outcome;
        case "delete_clip":
          env.items = env.items.filter((c) => c.id !== a.id);
          return null;
        case "set_pinned":
          env.items = env.items.map((c) =>
            c.id === a.id ? { ...c, pinned: a.pinned as boolean } : c,
          );
          return null;
        case "update_settings":
          return {};
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
  return env;
}

const calls = (env: Env, cmd: string) => env.calls.filter((c) => c.cmd === cmd);
const options = () => screen.queryAllByRole("option");

beforeEach(() => {
  document.body.innerHTML = "";
  document.documentElement.removeAttribute("data-theme");
});

/** 解放を呼ぶまで応答しない Promise */
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

const baseSettings: Settings = {
  shortcut: "CommandOrControl+Shift+V",
  max_items: 1000,
  retention_days: 30,
  theme: "dark",
  excluded_apps: [],
  record_unknown_source: true,
  permission_guide_dismissed: false,
};

describe("HistoryWindow 一覧", () => {
  it("検索結果の件数と全件数をヘッダーに表示する", async () => {
    setup({ items: [clip(1), clip(2), clip(3)] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(3));
    expect(screen.getByTestId("count").textContent).toBe("履歴 3 件 / 全 3 件");
  });

  it("番号は先頭の10行だけに付き、1〜9 と 10行目の 0 になる", async () => {
    const items = Array.from({ length: 12 }, (_, i) => clip(i + 1));
    setup({ items });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(12));
    expect(screen.getByTestId("count").textContent).toBe("履歴 12 件 / 全 12 件");
    const nums = options()
      .slice(0, 10)
      .map((o) => within(o).getByText(/^[0-9]$/).textContent);
    expect(nums).toEqual(["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]);
    for (const o of options().slice(10)) expect(within(o).queryByText(/^[0-9]$/)).toBeNull();
  });

  it("各行に種類の印(名前付き)と、本文の先頭1行を表示する", async () => {
    setup({ items: [clip(1, { kind: "url", first_line: "https://example.com" })] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    expect(screen.getByRole("img", { name: "URL" }).getAttribute("title")).toBe("URL");
    expect(screen.getByText("https://example.com")).toBeTruthy();
  });

  it("ピン留めした行は番号の代わりにピン留めの印を表示する", async () => {
    setup({ items: [clip(1, { pinned: true }), clip(2)] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    expect(within(options()[0]).getByRole("img", { name: "ピン留め" })).toBeTruthy();
    expect(within(options()[1]).getByText("2")).toBeTruthy();
  });

  it("画像の行は data URL のサムネイルを表示する", async () => {
    setup({ items: [clip(1, { kind: "image", first_line: "", thumb_png: "AAAA" })] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    const img = options()[0].querySelector("img[src^='data:image/png;base64,']");
    expect(img).not.toBeNull();
  });

  it("履歴の内容を HTML として解釈しない", async () => {
    setup({ items: [clip(1, { first_line: "<b>bold</b><img src=x>" })] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    expect(options()[0].querySelector("b")).toBeNull();
    expect(options()[0].textContent).toContain("<b>bold</b>");
  });

  it("履歴が0件のとき案内文を表示する", async () => {
    setup({ items: [] });
    render(<HistoryWindow />);
    await screen.findByText("まだ履歴がありません。何かコピーすると、ここに並びます");
  });

  it("検索で絞り込み、大文字小文字を区別せず、0件のとき別の案内文を表示する", async () => {
    setup({ items: [clip(1, { first_line: "Hello World" }), clip(2, { first_line: "other" })] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await user.type(screen.getByRole("searchbox"), "hello");
    await waitFor(() => expect(options().length).toBe(1));
    await user.clear(screen.getByRole("searchbox"));
    await user.type(screen.getByRole("searchbox"), "zzz");
    await screen.findByText("一致する履歴がありません");
  });

  it("表示時に検索欄にフォーカスする", async () => {
    setup({ items: [clip(1)] });
    render(<HistoryWindow />);
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("searchbox")));
  });
});

describe("HistoryWindow 時刻", () => {
  it("今日のコピーは HH:mm、別の日は M/D で表示する", async () => {
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 8, 5).getTime();
    const old = new Date(2020, 0, 31, 8, 5).getTime();
    setup({ items: [clip(1, { copied_at_ms: today }), clip(2, { copied_at_ms: old })] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    expect(within(options()[0]).getByText("08:05")).toBeTruthy();
    expect(within(options()[1]).getByText("1/31")).toBeTruthy();
  });
});

describe("HistoryWindow 操作", () => {
  it("Enter で選択行を paste_clip(auto) で貼り付ける", async () => {
    const env = setup({ items: [clip(1), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 1, mode: "auto" });
  });

  it("↓ キーで選択を移して Enter で貼り付ける(マウスなし)", async () => {
    const env = setup({ items: [clip(1), clip(2), clip(3)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(3));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{ArrowDown}{ArrowDown}");
    expect(options()[2].getAttribute("aria-selected")).toBe("true");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 3, mode: "auto" });
  });

  it("マウスが行の上を動いても、選択している行は変わらない", async () => {
    setup({ items: [clip(1), clip(2), clip(3)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(3));
    expect(options()[0].getAttribute("aria-selected")).toBe("true");
    await user.hover(options()[2]);
    await user.pointer({ target: options()[1] });
    expect(options()[0].getAttribute("aria-selected")).toBe("true");
    expect(options()[1].getAttribute("aria-selected")).toBe("false");
    expect(options()[2].getAttribute("aria-selected")).toBe("false");
  });

  it("行のクリックで貼り付ける", async () => {
    const env = setup({ items: [clip(1), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.click(options()[1]);
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 2, mode: "auto" });
  });

  it("macOS で Cmd+数字のとき、その番号の行を貼り付ける", async () => {
    const env = setup({ items: [clip(1), clip(2), clip(3)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(3));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Meta>}3{/Meta}");
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 3, mode: "auto" });
  });

  it("Windows で Alt+数字のとき、その番号の行を貼り付ける", async () => {
    const env = setup({ items: [clip(1), clip(2)], status: { ...baseStatus, os: "windows" } });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Alt>}2{/Alt}");
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 2, mode: "auto" });
  });

  it("修飾キーなしの数字は貼り付けずに検索欄へ入力される", async () => {
    const env = setup({ items: [clip(1, { first_line: "a1" }), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await user.keyboard("1");
    expect((screen.getByRole("searchbox") as HTMLInputElement).value).toBe("1");
    expect(calls(env, "paste_clip").length).toBe(0);
  });

  it("貼り付け中の Enter 連打は1回だけ paste_clip を呼ぶ", async () => {
    let release: (v: PasteOutcome) => void = () => {};
    const env = setup({ items: [clip(1)] });
    mockIPC(
      (cmd, args) => {
        env.calls.push({ cmd, args });
        if (cmd === "get_app_status") return env.status;
        if (cmd === "list_clips")
          return {
            req_id: (args as { reqId: number }).reqId,
            total: 1,
            matched: 1,
            items: env.items,
          };
        if (cmd === "paste_clip") return new Promise((r) => (release = r));
        return null;
      },
      { shouldMockEvents: true },
    );
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}{Enter}{Enter}");
    expect(calls(env, "paste_clip").length).toBe(1);
    release({ result: "pasted", reason: null });
  });

  it("Esc で hide_window を呼ぶ", async () => {
    const env = setup({ items: [clip(1)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await user.keyboard("{Escape}");
    expect(calls(env, "hide_window").length).toBe(1);
  });

  it("Cmd+, で設定ウィンドウを開く", async () => {
    const env = setup({ items: [clip(1)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Meta>},{/Meta}");
    expect(calls(env, "open_settings").length).toBe(1);
  });

  it("フッターの「設定」で設定ウィンドウを開く", async () => {
    const env = setup({ items: [clip(1)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await user.click(screen.getByRole("button", { name: "設定" }));
    expect(calls(env, "open_settings").length).toBe(1);
  });
});

describe("HistoryWindow プレビュー", () => {
  it("初期は閉じていて、Tab で開閉し、選択に追従する", async () => {
    const env = setup({
      items: [clip(1), clip(2)],
      details: {
        1: {
          id: 1,
          kind: "text",
          text: "全文1\n2行目",
          image_png: null,
          source_app_name: "Editor",
          copied_at_ms: 0,
          char_count: 7,
        },
        2: {
          id: 2,
          kind: "code",
          text: "const x = 1",
          image_png: null,
          source_app_name: null,
          copied_at_ms: 0,
          char_count: 11,
        },
      },
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    expect(screen.queryByRole("region", { name: "プレビュー" })).toBeNull();
    await user.keyboard("{Tab}");
    const pane = await screen.findByRole("region", { name: "プレビュー" });
    await waitFor(() => expect(pane.textContent).toContain("全文1"));
    expect(pane.textContent).toContain("テキスト");
    expect(pane.textContent).toContain("Editor");
    expect(pane.textContent).toContain("7 文字");
    await user.keyboard("{ArrowDown}");
    await waitFor(() => expect(pane.textContent).toContain("const x = 1"));
    expect(pane.textContent).toContain("コード");
    expect(calls(env, "get_clip_detail").length).toBeGreaterThan(1);
    await user.keyboard("{Tab}");
    expect(screen.queryByRole("region", { name: "プレビュー" })).toBeNull();
  });

  it("画像は data URL の img で拡大表示する", async () => {
    setup({
      items: [clip(1, { kind: "image", first_line: "", thumb_png: "AAAA" })],
      details: {
        1: {
          id: 1,
          kind: "image",
          text: null,
          image_png: "BBBB",
          source_app_name: null,
          copied_at_ms: 0,
          char_count: 0,
        },
      },
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await user.keyboard("{Tab}");
    const img = await screen.findByAltText("画像のプレビュー");
    expect(img.getAttribute("src")).toBe("data:image/png;base64,BBBB");
  });
});

describe("HistoryWindow 削除とピン留め", () => {
  const focusedText = () => document.activeElement?.textContent ?? "";

  it("中間の行を削除すると、フォーカスが次の行に移る", async () => {
    const env = setup({ items: [clip(1), clip(2), clip(3)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(3));
    await user.keyboard("{ArrowDown}");
    expect(focusedText()).toContain("item-2");
    await user.keyboard("{Delete}");
    await waitFor(() => expect(options().length).toBe(2));
    expect(calls(env, "delete_clip")[0].args).toEqual({ id: 2 });
    await waitFor(() => expect(focusedText()).toContain("item-3"));
  });

  it("最後の行を削除すると、フォーカスが前の行に移る", async () => {
    setup({ items: [clip(1), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await user.keyboard("{ArrowDown}{Delete}");
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(focusedText()).toContain("item-1"));
  });

  it("最後の1件を削除すると、フォーカスが検索欄に移る", async () => {
    setup({ items: [clip(1)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await user.keyboard("{ArrowDown}{Delete}");
    await waitFor(() => expect(options().length).toBe(0));
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("searchbox")));
  });

  it("行にフォーカスがあるときだけ、ピン留めと削除のキー案内を表示する", async () => {
    setup({ items: [clip(1), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    expect(screen.queryByText("ピン留め", { selector: "span" })).toBeNull();
    expect(screen.queryByText("削除")).toBeNull();
    await user.keyboard("{ArrowDown}");
    expect(await screen.findByText("削除")).toBeTruthy();
    expect(screen.getByText("ピン留め", { selector: "span:not([role])" })).toBeTruthy();
  });

  it("行にフォーカスして P キーを押すと、その行のピン留めを切り替える", async () => {
    const env = setup({ items: [clip(1), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await user.keyboard("{ArrowDown}p");
    await waitFor(() => expect(calls(env, "set_pinned").length).toBe(1));
    expect(calls(env, "set_pinned")[0].args).toEqual({ id: 2, pinned: true });
  });
});

describe("HistoryWindow 権限の案内", () => {
  const noPerm: AppStatus = { ...baseStatus, paste_permission: false };

  it("権限なしで案内が未表示のとき、Enter で貼り付けずに案内を出す", async () => {
    const env = setup({ items: [clip(1)], status: noPerm });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("アクセシビリティの権限");
    expect(calls(env, "paste_clip").length).toBe(0);
  });

  it("「システム設定を開く」で open_accessibility_settings を呼ぶ", async () => {
    const env = setup({ items: [clip(1)], status: noPerm });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await user.click(await screen.findByRole("button", { name: "システム設定を開く" }));
    expect(calls(env, "open_accessibility_settings").length).toBe(1);
  });

  it("権限がないとき「あとで」で非表示を保存し、copy_only で貼り付ける", async () => {
    const env = setup({
      items: [clip(1)],
      status: noPerm,
      outcome: { result: "copied_only", reason: "no_permission" },
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await user.click(await screen.findByRole("button", { name: "あとで(再コピーのみ)" }));
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "update_settings")[0].args).toEqual({
      patch: { permission_guide_dismissed: true },
    });
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 1, mode: "copy_only" });
    await screen.findByText("自動貼り付けには権限が必要です(再コピーのみ)");
  });

  it("案内の表示中に Esc を押すと、案内だけを閉じてウィンドウは閉じない", async () => {
    const env = setup({ items: [clip(1)], status: noPerm });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls(env, "hide_window").length).toBe(0);
    expect(calls(env, "paste_clip").length).toBe(0);
  });

  it("案内を非表示にした後は、権限なしのときフッターに案内を表示する", async () => {
    setup({ items: [clip(1)], status: { ...noPerm, permission_guide_dismissed: true } });
    render(<HistoryWindow />);
    await screen.findByText("自動貼り付けには権限が必要です(再コピーのみ)");
  });

  it("結果の reason が send_failed のとき、送信失敗の案内を表示する", async () => {
    const env = setup({
      items: [clip(1)],
      outcome: { result: "copied_only", reason: "send_failed" },
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await screen.findByText("貼り付けの送信に失敗しました(内容はコピー済みです)");
  });
});

describe("HistoryWindow 鍵が使えない状態", () => {
  it("storage が key_unavailable のとき、一覧の代わりに理由を表示する", async () => {
    setup({
      items: [clip(1)],
      status: { ...baseStatus, storage: "key_unavailable", storage_message: "keychain locked" },
    });
    render(<HistoryWindow />);
    const band = await screen.findByRole("status", { name: "状態" });
    expect(band.textContent).toContain(
      "暗号鍵を取得できないため、履歴を読み込んでいません。保存済みの履歴は変更していません",
    );
    expect(options().length).toBe(0);
    expect(screen.queryByRole("searchbox")).toBeNull();
  });

  it("storage が key_mismatch のとき、別の理由を表示する", async () => {
    setup({ status: { ...baseStatus, storage: "key_mismatch" } });
    render(<HistoryWindow />);
    const band = await screen.findByRole("status", { name: "状態" });
    expect(band.textContent).toContain("一致しない");
  });
});

describe("HistoryWindow ウィンドウの高さ", () => {
  it("内容に合わせて set_size を呼び、プレビューを開くと 96px 高くなる", async () => {
    const env = setup({ items: [clip(1), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    type Sz = { Logical: { width: number; height: number } };
    // Size インスタンスは JSON にすると { Logical: { width, height } } になる
    const sizes = () =>
      env.calls
        .filter((c) => c.cmd === "plugin:window|set_size")
        .map((c) => JSON.parse(JSON.stringify((c.args as { value: unknown }).value)) as Sz);
    await waitFor(() => expect(sizes().length).toBeGreaterThan(0));
    const last = () => sizes()[sizes().length - 1];
    const h = (v: Sz) => v.Logical.height;
    const before = h(last());
    expect(last().Logical.width).toBe(420);
    expect(before).toBeLessThanOrEqual(330);
    await user.keyboard("{Tab}");
    await waitFor(() => expect(h(last())).toBe(before + 96));
  });
});

describe("HistoryWindow テーマ", () => {
  it("起動時に、設定のテーマを履歴ウィンドウに反映する", async () => {
    setup({ items: [clip(1)], settings: { ...baseSettings, theme: "dark" } });
    render(<HistoryWindow />);
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBe("dark"));
  });

  it("settings-changed を受けたら、テーマを切り替える", async () => {
    setup({ items: [clip(1)], settings: { ...baseSettings, theme: "dark" } });
    render(<HistoryWindow />);
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBe("dark"));
    await act(async () => {
      await emit("settings-changed", { ...baseSettings, theme: "light" });
    });
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBe("light"));
    await act(async () => {
      await emit("settings-changed", { ...baseSettings, theme: "system" });
    });
    await waitFor(() => expect(document.documentElement.hasAttribute("data-theme")).toBe(false));
  });
});

describe("HistoryWindow 失敗の表示", () => {
  const err = { code: "internal", message: "boom" };

  it("貼り付けに失敗したら、フッターにエラーを表示し、次の操作もできる", async () => {
    const env = setup({ items: [clip(1)], fail: { paste_clip: err } });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await screen.findByText("貼り付けに失敗しました");
    expect(screen.getByRole("status", { name: "操作の結果" }).textContent).toContain(
      "貼り付けに失敗しました",
    );
    delete env.fail.paste_clip;
    await user.keyboard("{Enter}");
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(2));
    await waitFor(() => expect(screen.queryByText("貼り付けに失敗しました")).toBeNull());
  });

  it("削除に失敗したら、フッターにエラーを表示する", async () => {
    setup({ items: [clip(1), clip(2)], fail: { delete_clip: err } });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await user.keyboard("{ArrowDown}{Delete}");
    await screen.findByText("削除に失敗しました");
    expect(options().length).toBe(2);
  });

  it("ピン留めの変更に失敗したら、フッターにエラーを表示する", async () => {
    setup({ items: [clip(1), clip(2)], fail: { set_pinned: err } });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await user.keyboard("{ArrowDown}p");
    await screen.findByText("ピン留めの変更に失敗しました");
  });

  it("一覧の取得に失敗したら、「履歴がありません」ではなくエラーを表示する", async () => {
    setup({ items: [], fail: { list_clips: err } });
    render(<HistoryWindow />);
    await screen.findByText(/履歴を取得できませんでした/);
    expect(screen.queryByText(/まだ履歴がありません/)).toBeNull();
  });
});

describe("HistoryWindow 状態の帯", () => {
  const band = () => screen.findByRole("status", { name: "状態" });

  it("記録が止まっているとき、その旨を表示する", async () => {
    setup({ items: [clip(1)], status: { ...baseStatus, capture: "stopped" } });
    render(<HistoryWindow />);
    expect((await band()).textContent).toContain("クリップボードの記録を停止しています");
  });

  it("設定ファイルが壊れて既定で起動したとき、その旨を表示する", async () => {
    setup({ items: [clip(1)], status: { ...baseStatus, settings_recovered: true } });
    render(<HistoryWindow />);
    expect((await band()).textContent).toContain("既定の設定で起動しました");
  });

  it("保存の状態が error のとき、理由を表示する(一覧は出さない)", async () => {
    setup({
      items: [clip(1)],
      status: { ...baseStatus, storage: "error", storage_message: "lock poisoned" },
    });
    render(<HistoryWindow />);
    const b = await band();
    expect(b.textContent).toContain("履歴を読み込めませんでした");
    expect(b.textContent).toContain("lock poisoned");
    expect(options().length).toBe(0);
  });

  it("自動貼り付けの権限がないとき、その旨を1か所にだけ表示する", async () => {
    setup({
      items: [clip(1)],
      status: { ...baseStatus, paste_permission: false, permission_guide_dismissed: true },
    });
    render(<HistoryWindow />);
    await band();
    expect(screen.getAllByText("自動貼り付けには権限が必要です(再コピーのみ)").length).toBe(1);
  });

  it("鍵が使えないときの文は二重に出ない", async () => {
    setup({ status: { ...baseStatus, storage: "key_unavailable" } });
    render(<HistoryWindow />);
    await band();
    expect(screen.getAllByText(/暗号鍵を取得できないため/).length).toBe(1);
  });

  it("問題がないときは帯を出さない", async () => {
    setup({ items: [clip(1)] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    expect(screen.queryByRole("status", { name: "状態" })).toBeNull();
  });
});

describe("HistoryWindow 日本語入力", () => {
  const search = () => screen.getByRole("searchbox");
  const ready = async (env: ReturnType<typeof setup>) => {
    await waitFor(() => expect(options().length).toBeGreaterThan(0));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
  };

  it("変換中(compositionstart から compositionend まで)の Enter は貼り付けない", async () => {
    const env = setup({ items: [clip(1)] });
    render(<HistoryWindow />);
    await ready(env);
    fireEvent.compositionStart(search());
    fireEvent.keyDown(search(), { key: "Enter" });
    expect(calls(env, "paste_clip").length).toBe(0);
  });

  it("isComposing が true の Enter は貼り付けない", async () => {
    const env = setup({ items: [clip(1)] });
    render(<HistoryWindow />);
    await ready(env);
    fireEvent.keyDown(search(), { key: "Enter", isComposing: true });
    expect(calls(env, "paste_clip").length).toBe(0);
  });

  it("確定の Enter(keyCode 229)は貼り付けない", async () => {
    const env = setup({ items: [clip(1)] });
    render(<HistoryWindow />);
    await ready(env);
    fireEvent.keyDown(search(), { key: "Enter", keyCode: 229 });
    expect(calls(env, "paste_clip").length).toBe(0);
  });

  it("key が Process の Enter は貼り付けない", async () => {
    const env = setup({ items: [clip(1)] });
    render(<HistoryWindow />);
    await ready(env);
    fireEvent.keyDown(search(), { key: "Process", code: "Enter" });
    expect(calls(env, "paste_clip").length).toBe(0);
  });

  it("compositionend の直後の Enter は貼り付けず、50ms 過ぎたら貼り付ける", async () => {
    const env = setup({ items: [clip(1)] });
    render(<HistoryWindow />);
    await ready(env);
    fireEvent.compositionStart(search());
    fireEvent.compositionEnd(search());
    fireEvent.keyDown(search(), { key: "Enter" });
    expect(calls(env, "paste_clip").length).toBe(0);
    await act(async () => {
      await wait(70);
    });
    fireEvent.keyDown(search(), { key: "Enter" });
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
  });

  it("変換中の矢印キーと Esc は、選択もウィンドウの非表示もしない", async () => {
    const env = setup({ items: [clip(1), clip(2)] });
    render(<HistoryWindow />);
    await ready(env);
    fireEvent.compositionStart(search());
    fireEvent.keyDown(search(), { key: "ArrowDown", keyCode: 229 });
    fireEvent.keyDown(search(), { key: "Escape", keyCode: 229 });
    expect(options()[0].getAttribute("aria-selected")).toBe("true");
    expect(calls(env, "hide_window").length).toBe(0);
  });
});

describe("HistoryWindow スクロール", () => {
  const many = (n: number) => Array.from({ length: n }, (_, i) => clip(i + 1));
  const listCalls = (env: Env) =>
    calls(env, "list_clips").map((c) => c.args as { query: string; limit: number; offset: number });
  const scrollList = () => fireEvent.scroll(screen.getByRole("listbox"));

  it("50件ずつ取得し、一覧は縦にスクロールできる", async () => {
    const env = setup({ items: many(120) });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(50));
    expect(listCalls(env)[0]).toMatchObject({ query: "", limit: 50, offset: 0 });
    const list = screen.getByRole("listbox");
    expect(list.className).toContain("overflow-y-auto");
    expect(list.getAttribute("style")).toBeNull();
    expect(screen.getByTestId("count").textContent).toBe("履歴 120 件 / 全 120 件");
  });

  it("末尾に近づいたら offset を進めて続きを追加し、一致件数を超えては読まない", async () => {
    const env = setup({ items: many(120) });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(50));
    scrollList();
    await waitFor(() => expect(options().length).toBe(100));
    expect(listCalls(env)[1]).toMatchObject({ limit: 50, offset: 50 });
    scrollList();
    await waitFor(() => expect(options().length).toBe(120));
    expect(listCalls(env)[2]).toMatchObject({ offset: 100 });
    const n = listCalls(env).length;
    scrollList();
    await act(async () => {
      await wait(30);
    });
    expect(listCalls(env).length).toBe(n);
    expect(options().length).toBe(120);
  });

  it("読み込み中の追加の要求は重ねない", async () => {
    const slow = deferred<unknown>();
    let first = true;
    const env = setup({
      items: many(120),
      onList: (a, fallback) => {
        if (a.offset === 50) return slow.promise.then(() => fallback());
        first = false;
        return fallback();
      },
    });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(50));
    expect(first).toBe(false);
    scrollList();
    scrollList();
    scrollList();
    expect(listCalls(env).filter((c) => c.offset === 50).length).toBe(1);
    await act(async () => {
      slow.resolve(null);
    });
    await waitFor(() => expect(options().length).toBe(100));
  });

  it("検索語が変わったら offset を 0 に戻し、読み込み中だった続きの応答は捨てる", async () => {
    const slow = deferred<unknown>();
    const env = setup({
      items: many(120),
      onList: (a, fallback) => (a.offset === 50 ? slow.promise.then(() => fallback()) : fallback()),
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(50));
    scrollList();
    await user.type(screen.getByRole("searchbox"), "item-1");
    await waitFor(() => expect(screen.getByTestId("count").textContent).toContain("全 120 件"));
    const last = listCalls(env)[listCalls(env).length - 1];
    expect(last).toMatchObject({ query: "item-1", offset: 0 });
    const before = options().map((o) => o.textContent);
    await act(async () => {
      slow.resolve(null);
      await wait(20);
    });
    // 古い検索の続き(offset 50, query "")は混ざらない
    expect(options().map((o) => o.textContent)).toEqual(before);
    expect(new Set(before).size).toBe(before.length);
    for (const o of options()) expect(o.textContent).toContain("item-1");
  });

  it("矢印キーで11行目以降に進み、選択した行を見える位置にスクロールする", async () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    setup({ items: many(30) });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(30));
    for (let i = 0; i < 10; i++)
      fireEvent.keyDown(screen.getByRole("searchbox"), { key: "ArrowDown" });
    await waitFor(() => expect(options()[10].getAttribute("aria-selected")).toBe("true"));
    await waitFor(() =>
      expect(scrollIntoView.mock.contexts[scrollIntoView.mock.contexts.length - 1]).toBe(
        options()[10],
      ),
    );
    expect(scrollIntoView.mock.calls[scrollIntoView.mock.calls.length - 1]?.[0]).toMatchObject({
      block: "nearest",
    });
  });

  it("矢印キーで読み込み済みの末尾に近づいたら、続きを読み込む", async () => {
    Element.prototype.scrollIntoView = vi.fn();
    const env = setup({ items: many(80) });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(50));
    for (let i = 0; i < 46; i++)
      fireEvent.keyDown(screen.getByRole("searchbox"), { key: "ArrowDown" });
    await waitFor(() => expect(options().length).toBe(80));
    expect(listCalls(env).some((c) => c.offset === 50)).toBe(true);
  });

  it("11行目以降の行も、Enter で貼り付けられる", async () => {
    Element.prototype.scrollIntoView = vi.fn();
    const env = setup({ items: many(30) });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(30));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    for (let i = 0; i < 12; i++)
      fireEvent.keyDown(screen.getByRole("searchbox"), { key: "ArrowDown" });
    fireEvent.keyDown(screen.getByRole("searchbox"), { key: "Enter" });
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 13, mode: "auto" });
  });

  it("番号キーは先頭の10行だけを貼り付ける", async () => {
    const env = setup({ items: many(30) });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(30));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Meta>}0{/Meta}");
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 10, mode: "auto" });
  });
});

describe("HistoryWindow 検索の応答待ち", () => {
  it("最新の検索の応答が届くまでは、Enter で貼り付けない", async () => {
    const gate = deferred<unknown>();
    const env = setup({
      items: [clip(1, { first_line: "apple" }), clip(2, { first_line: "banana" })],
      onList: (a, fallback) => (a.query === "b" ? gate.promise.then(() => fallback()) : fallback()),
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.type(screen.getByRole("searchbox"), "b");
    await user.keyboard("{Enter}");
    expect(calls(env, "paste_clip").length).toBe(0);
    await act(async () => {
      gate.resolve(null);
    });
    await waitFor(() => expect(options().length).toBe(1));
    await user.keyboard("{Enter}");
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 2, mode: "auto" });
  });

  it("最新の応答が届くまでは、行のクリックでも貼り付けない", async () => {
    const gate = deferred<unknown>();
    const env = setup({
      items: [clip(1, { first_line: "apple" }), clip(2, { first_line: "banana" })],
      onList: (a, fallback) => (a.query === "a" ? gate.promise.then(() => fallback()) : fallback()),
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.type(screen.getByRole("searchbox"), "a");
    await user.click(options()[1]);
    expect(calls(env, "paste_clip").length).toBe(0);
    await act(async () => {
      gate.resolve(null);
    });
  });
});

describe("HistoryWindow 権限の案内の状態", () => {
  const noPerm: AppStatus = { ...baseStatus, paste_permission: false };

  it("履歴ウィンドウを再表示したとき、開いていた案内を閉じて権限を判定し直す", async () => {
    const env = setup({ items: [clip(1)], status: noPerm });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await screen.findByRole("dialog");
    const before = calls(env, "get_app_status").length;
    await act(async () => {
      await emit("history-window-shown", null);
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls(env, "get_app_status").length).toBeGreaterThan(before);
  });

  it("「あとで」を押したとき権限が既にあれば、非表示を保存しない", async () => {
    const env = setup({
      items: [clip(1)],
      status: noPerm,
      outcome: { result: "pasted", reason: null },
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    const later = await screen.findByRole("button", { name: "あとで(再コピーのみ)" });
    env.status = { ...baseStatus, paste_permission: true };
    await user.click(later);
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "update_settings").length).toBe(0);
  });

  it("貼り付け先を確認できなかったとき、コピーのみになった旨を表示する", async () => {
    const env = setup({
      items: [clip(1)],
      outcome: { result: "copied_only", reason: "focus_lost" },
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await screen.findByText(/貼り付け先を確認できなかったため/);
  });
});

describe("HistoryWindow 選択の導出", () => {
  it("選択中の行が一覧から消えたら、選択は先頭の行になり、削除も先頭の行に対して行う", async () => {
    const env = setup({ items: [clip(1), clip(2), clip(3)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(3));
    await user.keyboard("{ArrowDown}");
    expect(options()[1].getAttribute("aria-selected")).toBe("true");
    env.items = [clip(1), clip(3)];
    await act(async () => {
      await emit("history-changed", null);
    });
    await waitFor(() => expect(options().length).toBe(2));
    expect(options()[0].getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(options()[0], { key: "Delete" });
    await waitFor(() => expect(calls(env, "delete_clip").length).toBe(1));
    expect(calls(env, "delete_clip")[0].args).toEqual({ id: 1 });
  });

  it("ウィンドウの高さの設定は、古い計算が後から完了しても最新の高さが残る", async () => {
    const innerSize = deferred<unknown>();
    let sizeCalls = 0;
    const env = setup({ items: [clip(1)] });
    mockIPC(
      (cmd, args) => {
        env.calls.push({ cmd, args });
        const a = (args ?? {}) as Record<string, unknown>;
        if (cmd === "plugin:window|inner_size") {
          sizeCalls++;
          return sizeCalls === 1 ? innerSize.promise : { width: 840, height: 400 };
        }
        if (cmd === "plugin:window|scale_factor") return 2;
        if (cmd === "get_app_status") return env.status;
        if (cmd === "list_clips") {
          const items = env.items;
          return {
            req_id: a.reqId,
            total: items.length,
            matched: items.length,
            items,
          };
        }
        return null;
      },
      { shouldMockEvents: true },
    );
    render(<HistoryWindow />);
    await waitFor(() => expect(sizeCalls).toBe(1));
    env.items = [clip(1), clip(2), clip(3), clip(4)];
    await act(async () => {
      await emit("history-changed", null);
    });
    await waitFor(() => expect(sizeCalls).toBeGreaterThanOrEqual(2));
    await act(async () => {
      innerSize.resolve({ width: 840, height: 400 });
      await wait(30);
    });
    const heights = env.calls
      .filter((c) => c.cmd === "plugin:window|set_size")
      .map(
        (c) =>
          JSON.parse(JSON.stringify((c.args as { value: unknown }).value)) as {
            Logical: { height: number };
          },
      )
      .map((v) => v.Logical.height);
    // 幅の取得が遅れた古い計算(1行)が、新しい高さ(4行)を上書きしない
    expect(heights[heights.length - 1]).toBe(windowHeight(4, false));
  });
});

describe("HistoryWindow 行の表示", () => {
  it("複数行のとき、行末に +N行 を表示する", async () => {
    setup({ items: [clip(1, { extra_lines: 3 }), clip(2)] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    expect(within(options()[0]).getByText("+3行")).toBeTruthy();
    expect(within(options()[1]).queryByText(/^\+\d+行$/)).toBeNull();
  });

  it("画像の行は first_line ではなく種類から「(画像)」と表示する", async () => {
    setup({
      items: [clip(1, { kind: "image", first_line: "should-not-show", thumb_png: "AAAA" })],
    });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    expect(within(options()[0]).getByText("(画像)")).toBeTruthy();
    expect(options()[0].textContent).not.toContain("should-not-show");
  });
});

describe("HistoryWindow 削除のキー", () => {
  const rowFocus = async (os: string) => {
    const env = setup({ items: [clip(1), clip(2)], status: { ...baseStatus, os } });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await waitFor(() => expect(options().length).toBe(2));
    fireEvent.keyDown(screen.getByRole("searchbox"), { key: "ArrowDown" });
    await waitFor(() => expect(document.activeElement).toBe(options()[1]));
    return env;
  };

  it("macOS では行で Cmd+Backspace を押すと削除する", async () => {
    const env = await rowFocus("macos");
    fireEvent.keyDown(options()[1], { key: "Backspace", metaKey: true });
    await waitFor(() => expect(calls(env, "delete_clip").length).toBe(1));
    expect(calls(env, "delete_clip")[0].args).toEqual({ id: 2 });
  });

  it("Windows では行で Ctrl+Backspace を押すと削除する", async () => {
    const env = await rowFocus("windows");
    fireEvent.keyDown(options()[1], { key: "Backspace", ctrlKey: true });
    await waitFor(() => expect(calls(env, "delete_clip").length).toBe(1));
  });

  it("修飾キーなしの Backspace は削除せず、検索欄へ戻す", async () => {
    const env = await rowFocus("macos");
    fireEvent.keyDown(options()[1], { key: "Backspace" });
    expect(calls(env, "delete_clip").length).toBe(0);
    expect(document.activeElement).toBe(screen.getByRole("searchbox"));
  });

  it("行にフォーカスがあるとき、Delete に加えて OS ごとの削除のキーを案内する", async () => {
    await rowFocus("macos");
    expect(await screen.findByText(/⌘⌫/)).toBeTruthy();
  });
});
