import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { SettingsWindow } from "./SettingsWindow";
import type { AppRef, LimitPreview, Settings } from "../../types";

const baseSettings: Settings = {
  shortcut: "CommandOrControl+Shift+V",
  max_items: 1000,
  retention_days: 30,
  theme: "system",
  excluded_apps: [
    { id: "com.1password.1password", name: "1Password" },
    { id: "com.bitwarden.desktop", name: "Bitwarden" },
  ],
  record_unknown_source: true,
  permission_guide_dismissed: false,
};

interface Backend {
  calls: { cmd: string; args: unknown }[];
  settings: Settings;
  preview: (maxItems: string, days: string) => LimitPreview;
  total: number;
  running: AppRef[];
  setShortcutError?: { code: string; message: string };
  /** 設定した間、最初の update_settings の応答を止める */
  holdUpdate?: Promise<unknown> | null;
}

/** IPC の偽のバックエンドを作る。呼ばれたコマンドを calls に記録する。 */
function setup(overrides: Partial<Backend> = {}) {
  const backend: Backend = {
    calls: [],
    settings: { ...baseSettings },
    preview: () => ({ valid: true, error: null, will_delete: 0 }),
    total: 5,
    running: [{ id: "com.example.app", name: "Example" }],
    ...overrides,
  };
  mockWindows("settings");
  mockIPC(
    (cmd, args) => {
      backend.calls.push({ cmd, args });
      const a = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "get_settings":
          return backend.settings;
        case "get_app_status":
          return { version: "0.1.0" };
        case "preview_limits":
          return backend.preview(a.maxItems as string, a.retentionDays as string);
        case "update_settings": {
          const patch = a.patch as Record<string, unknown>;
          if (backend.holdUpdate) {
            const hold = backend.holdUpdate;
            backend.holdUpdate = null;
            return hold.then(() => {
              backend.settings = { ...backend.settings, ...patch } as Settings;
              return backend.settings;
            });
          }
          backend.settings = {
            ...backend.settings,
            ...patch,
            ...(patch.max_items ? { max_items: Number(patch.max_items) } : {}),
            ...(patch.retention_days ? { retention_days: Number(patch.retention_days) } : {}),
          } as Settings;
          return backend.settings;
        }
        case "set_shortcut":
          if (backend.setShortcutError) throw backend.setShortcutError;
          backend.settings = { ...backend.settings, shortcut: a.accelerator as string };
          return backend.settings;
        case "list_clips":
          return { req_id: a.reqId, total: backend.total, matched: 0, items: [] };
        case "clear_history":
          return backend.total;
        case "list_running_apps":
          return backend.running;
      }
      return undefined;
    },
    { shouldMockEvents: true },
  );
  return backend;
}

const callsOf = (b: Backend, cmd: string) => b.calls.filter((c) => c.cmd === cmd);

async function open(section: string | RegExp) {
  await userEvent.click(await screen.findByRole("button", { name: section }));
}

describe("SettingsWindow", () => {
  it("初期状態では「一般」だけが開いている", async () => {
    setup();
    render(<SettingsWindow />);
    const general = await screen.findByRole("button", { name: /一般/ });
    expect(general.getAttribute("aria-expanded")).toBe("true");
    for (const name of ["履歴", /除外アプリ/, /このアプリについて/]) {
      const b = screen.getByRole("button", { name });
      expect(b.getAttribute("aria-expanded")).toBe("false");
    }
    expect(screen.getByRole("button", { name: "呼び出しショートカット" })).toBeTruthy();
  });

  it("見出しをクリックすると開閉する", async () => {
    setup();
    render(<SettingsWindow />);
    await open("除外アプリ");
    expect(screen.getByRole("list", { name: "除外アプリ" })).toBeTruthy();
  });

  it("テーマを変えると update_settings に theme が渡り、画面に反映される", async () => {
    const b = setup();
    render(<SettingsWindow />);
    await userEvent.selectOptions(await screen.findByLabelText("テーマ"), "dark");
    await waitFor(() => expect(callsOf(b, "update_settings")).toHaveLength(1));
    expect(callsOf(b, "update_settings")[0].args).toEqual({ patch: { theme: "dark" } });
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBe("dark"));
    document.documentElement.removeAttribute("data-theme");
  });

  describe("ショートカット", () => {
    it("修飾キー付きのキーを押すと set_shortcut が呼ばれ、表示が変わる", async () => {
      const b = setup();
      render(<SettingsWindow />);
      await userEvent.click(await screen.findByRole("button", { name: "呼び出しショートカット" }));
      await userEvent.keyboard("{Control>}{Alt>}k{/Alt}{/Control}");
      await waitFor(() => expect(callsOf(b, "set_shortcut")).toHaveLength(1));
      expect(callsOf(b, "set_shortcut")[0].args).toEqual({ accelerator: "Control+Alt+K" });
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "呼び出しショートカット" }).textContent).toBe(
          "Control + Alt + K",
        ),
      );
    });

    it("競合で失敗したとき、メッセージを表示し、元の表示を維持する", async () => {
      const b = setup({
        setShortcutError: {
          code: "shortcut_conflict",
          message: "ショートカットを登録できません: 使用中",
        },
      });
      render(<SettingsWindow />);
      await userEvent.click(await screen.findByRole("button", { name: "呼び出しショートカット" }));
      await userEvent.keyboard("{Control>}{Alt>}k{/Alt}{/Control}");
      const alert = await screen.findByRole("alert");
      expect(alert.textContent).toContain("登録できません");
      expect(alert.textContent).toContain("元の設定を維持");
      expect(screen.getByRole("button", { name: "呼び出しショートカット" }).textContent).toBe(
        "Cmd/Ctrl + Shift + V",
      );
      expect(b.settings.shortcut).toBe("CommandOrControl+Shift+V");
    });

    it("修飾キーなしのキーでは登録しない", async () => {
      const b = setup();
      render(<SettingsWindow />);
      await userEvent.click(await screen.findByRole("button", { name: "呼び出しショートカット" }));
      await userEvent.keyboard("k");
      expect(callsOf(b, "set_shortcut")).toHaveLength(0);
    });

    // 入力待ちの間は、今のショートカットを解除する(押したキーが、グローバルショートカットに奪われないように)
    it("クリックして入力待ちになったとき、いまのショートカットを一時解除する", async () => {
      const b = setup();
      render(<SettingsWindow />);
      await userEvent.click(await screen.findByRole("button", { name: "呼び出しショートカット" }));
      await waitFor(() => expect(callsOf(b, "suspend_shortcut")).toHaveLength(1));
      expect(callsOf(b, "resume_shortcut")).toHaveLength(0);
    });

    it("Esc で取り消したとき、一時解除したショートカットを戻す", async () => {
      const b = setup();
      render(<SettingsWindow />);
      await userEvent.click(await screen.findByRole("button", { name: "呼び出しショートカット" }));
      await userEvent.keyboard("{Escape}");
      await waitFor(() => expect(callsOf(b, "resume_shortcut")).toHaveLength(1));
    });

    it("フォーカスが外れたとき、一時解除したショートカットを戻す", async () => {
      const b = setup();
      render(<SettingsWindow />);
      await userEvent.click(await screen.findByRole("button", { name: "呼び出しショートカット" }));
      // 入力待ちの間は Tab も入力として扱うため、他の場所をクリックしてフォーカスを外す
      await userEvent.click(document.body);
      await waitFor(() => expect(callsOf(b, "resume_shortcut")).toHaveLength(1));
    });

    it("キーを確定するとき、先にショートカットを戻してから set_shortcut を呼ぶ", async () => {
      const b = setup();
      render(<SettingsWindow />);
      await userEvent.click(await screen.findByRole("button", { name: "呼び出しショートカット" }));
      await userEvent.keyboard("{Control>}{Alt>}k{/Alt}{/Control}");
      await waitFor(() => expect(callsOf(b, "set_shortcut")).toHaveLength(1));
      const order = b.calls
        .map((c) => c.cmd)
        .filter((c) => ["suspend_shortcut", "resume_shortcut", "set_shortcut"].includes(c));
      expect(order).toEqual(["suspend_shortcut", "resume_shortcut", "set_shortcut"]);
    });

    it("入力待ちの Esc は取り消しで、ウィンドウは閉じない", async () => {
      const b = setup();
      render(<SettingsWindow />);
      await userEvent.click(await screen.findByRole("button", { name: "呼び出しショートカット" }));
      await userEvent.keyboard("{Escape}");
      expect(screen.getByRole("button", { name: "呼び出しショートカット" }).textContent).toBe(
        "Cmd/Ctrl + Shift + V",
      );
      expect(callsOf(b, "plugin:window|hide")).toHaveLength(0);
    });
  });

  describe("履歴の上限", () => {
    it("0 を入力すると、エラーを表示し、設定を変更しない", async () => {
      const b = setup({
        preview: () => ({
          valid: false,
          error: {
            field: "max_items",
            message: "最大件数は 1 以上 100000 以下の半角整数で入力してください",
          },
          will_delete: 0,
        }),
      });
      render(<SettingsWindow />);
      await open("履歴");
      const input = screen.getByLabelText("最大件数");
      await userEvent.clear(input);
      await userEvent.type(input, "0");
      await userEvent.click(screen.getByRole("button", { name: "保存" }));
      expect((await screen.findByRole("alert")).textContent).toContain("最大件数");
      expect(callsOf(b, "update_settings")).toHaveLength(0);
      expect(screen.queryByRole("dialog")).toBeNull();
    });

    it("削除される履歴が無いとき、確認なしで保存する", async () => {
      const b = setup();
      render(<SettingsWindow />);
      await open("履歴");
      const input = screen.getByLabelText("最大件数");
      await userEvent.clear(input);
      await userEvent.type(input, "2000");
      await userEvent.click(screen.getByRole("button", { name: "保存" }));
      await waitFor(() => expect(callsOf(b, "update_settings")).toHaveLength(1));
      expect(callsOf(b, "update_settings")[0].args).toEqual({
        patch: { max_items: "2000", retention_days: "30" },
      });
      expect(screen.queryByRole("dialog")).toBeNull();
      expect((await screen.findByRole("status")).textContent).toBe("保存しました");
    });

    it("上限を下げて古い履歴が削除されるとき、確認を出し、キャンセルすると保存しない", async () => {
      const b = setup({ preview: () => ({ valid: true, error: null, will_delete: 1234 }) });
      render(<SettingsWindow />);
      await open("履歴");
      const input = screen.getByLabelText("最大件数");
      await userEvent.clear(input);
      await userEvent.type(input, "10");
      await userEvent.click(screen.getByRole("button", { name: "保存" }));
      const dialog = await screen.findByRole("dialog");
      expect(dialog.textContent).toContain("1,234");
      await userEvent.click(within(dialog).getByRole("button", { name: "キャンセル" }));
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(callsOf(b, "update_settings")).toHaveLength(0);
    });

    it("確認で「削除して保存」を押すと update_settings が呼ばれる", async () => {
      const b = setup({ preview: () => ({ valid: true, error: null, will_delete: 3 }) });
      render(<SettingsWindow />);
      await open("履歴");
      const input = screen.getByLabelText("最大件数");
      await userEvent.clear(input);
      await userEvent.type(input, "10");
      await userEvent.click(screen.getByRole("button", { name: "保存" }));
      const dialog = await screen.findByRole("dialog");
      await userEvent.click(within(dialog).getByRole("button", { name: "削除して保存" }));
      await waitFor(() => expect(callsOf(b, "update_settings")).toHaveLength(1));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    });

    it("全履歴を削除は、件数入りの確認を出し、実行すると clear_history が呼ばれる", async () => {
      const b = setup({ total: 1000 });
      render(<SettingsWindow />);
      await open("履歴");
      await userEvent.click(screen.getByRole("button", { name: "全履歴を削除" }));
      const dialog = await screen.findByRole("dialog");
      expect(dialog.textContent).toContain("1,000件の履歴をすべて削除します");
      expect(callsOf(b, "clear_history")).toHaveLength(0);
      await userEvent.click(within(dialog).getByRole("button", { name: "削除" }));
      await waitFor(() => expect(callsOf(b, "clear_history")).toHaveLength(1));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    });

    it("全履歴の件数は、limit 1 の list_clips の total で調べる", async () => {
      const b = setup({ total: 1000 });
      render(<SettingsWindow />);
      await open("履歴");
      await userEvent.click(screen.getByRole("button", { name: "全履歴を削除" }));
      await screen.findByRole("dialog");
      const args = callsOf(b, "list_clips")[0].args as Record<string, unknown>;
      expect(args).toMatchObject({ query: "", limit: 1, offset: 0 });
      expect(typeof args.reqId).toBe("number");
    });

    it("全履歴の確認でキャンセルすると削除しない", async () => {
      const b = setup();
      render(<SettingsWindow />);
      await open("履歴");
      await userEvent.click(screen.getByRole("button", { name: "全履歴を削除" }));
      await userEvent.click(
        within(await screen.findByRole("dialog")).getByRole("button", { name: "キャンセル" }),
      );
      expect(callsOf(b, "clear_history")).toHaveLength(0);
    });
  });

  describe("除外アプリ", () => {
    it("初期の除外アプリが一覧に表示される", async () => {
      setup();
      render(<SettingsWindow />);
      await open("除外アプリ");
      const list = screen.getByRole("list", { name: "除外アプリ" });
      expect(
        within(list)
          .getAllByRole("listitem")
          .map((li) => li.textContent),
      ).toEqual(["1Password削除", "Bitwarden削除"]);
    });

    it("削除すると、そのアプリを除いた一覧で update_settings が呼ばれる", async () => {
      const b = setup();
      render(<SettingsWindow />);
      await open("除外アプリ");
      await userEvent.click(screen.getByRole("button", { name: "1Passwordを除外から外す" }));
      await waitFor(() => expect(callsOf(b, "update_settings")).toHaveLength(1));
      expect(callsOf(b, "update_settings")[0].args).toEqual({
        patch: { excluded_apps: [{ id: "com.bitwarden.desktop", name: "Bitwarden" }] },
      });
      await waitFor(() => expect(screen.queryByText("1Password")).toBeNull());
    });

    it("実行中のアプリから選んで追加できる(除外済みは候補に出ない)", async () => {
      const b = setup({
        running: [
          { id: "com.bitwarden.desktop", name: "Bitwarden" },
          { id: "com.example.app", name: "Example" },
        ],
      });
      render(<SettingsWindow />);
      await open("除外アプリ");
      await userEvent.click(screen.getByRole("button", { name: "実行中のアプリから追加" }));
      const picker = await screen.findByRole("list", { name: "実行中のアプリ" });
      expect(
        within(picker)
          .getAllByRole("button")
          .map((x) => x.textContent),
      ).toEqual(["Example"]);
      await userEvent.click(within(picker).getByRole("button", { name: "Example" }));
      await waitFor(() => expect(callsOf(b, "update_settings")).toHaveLength(1));
      expect(
        (callsOf(b, "update_settings")[0].args as { patch: { excluded_apps: AppRef[] } }).patch
          .excluded_apps,
      ).toEqual([...baseSettings.excluded_apps, { id: "com.example.app", name: "Example" }]);
    });

    it("設定の保存が処理中で追加がスキップされたとき、候補を一覧から消さない", async () => {
      let release!: (v: unknown) => void;
      const b = setup({ holdUpdate: new Promise((r) => (release = r)) });
      render(<SettingsWindow />);
      await userEvent.selectOptions(await screen.findByLabelText("テーマ"), "dark");
      await waitFor(() => expect(callsOf(b, "update_settings")).toHaveLength(1));
      await open("除外アプリ");
      await userEvent.click(screen.getByRole("button", { name: "実行中のアプリから追加" }));
      const picker = await screen.findByRole("list", { name: "実行中のアプリ" });
      await userEvent.click(within(picker).getByRole("button", { name: "Example" }));
      expect(callsOf(b, "update_settings")).toHaveLength(1);
      expect(screen.getByRole("button", { name: "Example" })).toBeTruthy();
      release(null);
    });

    it("コピー元が不明なときの記録を切り替えられる", async () => {
      const b = setup();
      render(<SettingsWindow />);
      await open("除外アプリ");
      const toggle = screen.getByRole("switch", { name: "コピー元が不明なときも記録する" });
      expect(toggle.getAttribute("aria-checked")).toBe("true");
      await userEvent.click(toggle);
      await waitFor(() => expect(callsOf(b, "update_settings")).toHaveLength(1));
      expect(callsOf(b, "update_settings")[0].args).toEqual({
        patch: { record_unknown_source: false },
      });
      await waitFor(() => expect(toggle.getAttribute("aria-checked")).toBe("false"));
    });
  });

  it("このアプリについてにバージョンとライセンスを表示する", async () => {
    setup();
    render(<SettingsWindow />);
    await open("このアプリについて");
    await waitFor(() => expect(screen.getByText("バージョン: 0.1.0")).toBeTruthy());
    expect(screen.getByText("ライセンス: MIT")).toBeTruthy();
  });

  it("Esc でウィンドウを隠す", async () => {
    const b = setup();
    render(<SettingsWindow />);
    await screen.findByRole("button", { name: /一般/ });
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(callsOf(b, "plugin:window|hide")).toHaveLength(1));
  });

  it("閉じるボタンでウィンドウを隠す", async () => {
    const b = setup();
    render(<SettingsWindow />);
    await userEvent.click(await screen.findByRole("button", { name: "閉じる" }));
    await waitFor(() => expect(callsOf(b, "plugin:window|hide")).toHaveLength(1));
  });
});
