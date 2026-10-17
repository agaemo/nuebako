import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { hideWindow, openSettings } from "../../lib/ipc";
import { onHistoryWindowShown } from "../../lib/events";
import { digitPasteIndex } from "../../lib/keys";
import { HeaderBar } from "./HeaderBar";
import { HistoryItem } from "./HistoryItem";
import { KeyHintBar } from "./KeyHintBar";
import { PermissionGuide } from "./PermissionGuide";
import { PreviewPane } from "./PreviewPane";
import { unlistenAll } from "./unlisten";
import { nextSelectionAfterDelete, useHistory } from "./useHistory";
import { usePaste } from "./usePaste";
import { MAX_ROWS, windowHeight } from "./windowHeight";

const WINDOW_WIDTH = 420;

/** 鍵が使えないときに、一覧の代わりに出す理由。 */
const STORAGE_MESSAGES: Record<string, string> = {
  key_unavailable:
    "暗号鍵を取得できないため、履歴を読み込んでいません。保存済みの履歴は変更していません",
  key_mismatch:
    "暗号鍵が保存済みの履歴と一致しないため、履歴を読み込んでいません。保存済みの履歴は変更していません",
  error: "履歴を読み込めませんでした。保存済みの履歴は変更していません",
};

/** 履歴ウィンドウ(S1)と、権限の案内(S4)。 */
export function HistoryWindow() {
  const history = useHistory();
  const { items, itemsRef, total, query, setQuery, nowMs, remove, togglePin } = history;
  const paste = usePaste();

  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [rowFocused, setRowFocused] = useState(false);
  const [focusReq, setFocusReq] = useState<{ id: number | null } | null>(null);

  const inputRef = useRef<HTMLInputElement>(null);
  const rowRefs = useRef(new Map<number, HTMLDivElement>());
  const selectedIdRef = useRef<number | null>(null);
  const osRef = useRef("");
  osRef.current = paste.status?.os ?? "";

  const storageBad = paste.status !== null && paste.status.storage !== "ok";

  const visible = useMemo(() => items.slice(0, MAX_ROWS), [items]);
  const selectedIndex = Math.max(
    0,
    visible.findIndex((c) => c.id === selectedId),
  );
  const selectedClip = visible[selectedIndex] ?? null;

  const select = useCallback((id: number | null) => {
    selectedIdRef.current = id;
    setSelectedId(id);
  }, []);

  // 表示のたびに、選択を先頭に戻して検索欄にフォーカスする(一覧は useHistory が取り直す)
  useEffect(() => {
    inputRef.current?.focus();
    const un = onHistoryWindowShown(() => {
      select(null);
      setFocusReq({ id: null });
    });
    return () => unlistenAll([un]);
  }, [select]);

  // フォーカスの移動(削除後、矢印キー)。描画の後に行う
  useEffect(() => {
    if (!focusReq) return;
    if (focusReq.id === null) inputRef.current?.focus();
    else rowRefs.current.get(focusReq.id)?.focus();
  }, [focusReq]);

  // ウィンドウの高さを内容に合わせる。テストや Web 表示では失敗しても無視する
  const height = windowHeight(storageBad ? 0 : visible.length, previewOpen && !storageBad);
  useEffect(() => {
    try {
      void getCurrentWindow()
        .setSize(new LogicalSize(WINDOW_WIDTH, height))
        .catch(() => {});
    } catch {
      // ウィンドウが無い環境
    }
  }, [height]);

  const move = (delta: number) => {
    const list = itemsRef.current.slice(0, MAX_ROWS);
    if (list.length === 0) return;
    const cur = Math.max(
      0,
      list.findIndex((c) => c.id === selectedIdRef.current),
    );
    const next = list[Math.min(list.length - 1, Math.max(0, cur + delta))];
    select(next.id);
    setFocusReq({ id: next.id });
  };

  const pasteAt = (index: number) => {
    const clip = itemsRef.current.slice(0, MAX_ROWS)[index];
    if (clip && !history.mutating) void paste.paste(clip.id);
  };

  /** 削除。次の行(無ければ前の行、0件なら検索欄)へ選択とフォーカスを移す。 */
  const deleteSelected = async () => {
    const id = selectedIdRef.current ?? itemsRef.current[0]?.id;
    if (id === undefined) return;
    const nextId = nextSelectionAfterDelete(itemsRef.current, id);
    try {
      await remove(id);
    } catch {
      return;
    }
    select(nextId);
    setFocusReq({ id: nextId });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (paste.guideFor !== null) return;
    const target = e.target as HTMLElement;
    const digit = digitPasteIndex(e.nativeEvent, osRef.current);
    if (digit !== null) {
      e.preventDefault();
      pasteAt(digit);
      return;
    }
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === ",") {
      e.preventDefault();
      void openSettings();
      return;
    }
    if (e.nativeEvent.isComposing) return; // IME の変換中
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        void hideWindow();
        return;
      case "ArrowDown":
        e.preventDefault();
        move(1);
        return;
      case "ArrowUp":
        e.preventDefault();
        move(-1);
        return;
      case "Tab":
        e.preventDefault();
        setPreviewOpen((v) => !v);
        return;
      case "Enter":
        if (target.tagName === "BUTTON") return; // ボタン自身の操作に任せる
        e.preventDefault();
        pasteAt(selectedIndex);
        return;
    }
    if (target.getAttribute("role") === "option" && !mod && !e.altKey) {
      if (e.key === "Delete") {
        e.preventDefault();
        void deleteSelected();
      } else if (e.key === "p" || e.key === "P") {
        e.preventDefault();
        if (selectedClip) void togglePin(selectedClip.id, !selectedClip.pinned);
      } else if (e.key === "Backspace" || e.key.length === 1) {
        // 行にフォーカスがあるときの文字入力は、検索欄への入力に戻す
        inputRef.current?.focus();
      }
    }
  };

  const emptyMessage =
    query === ""
      ? "まだ履歴がありません。何かコピーすると、ここに並びます"
      : "一致する履歴がありません";

  return (
    <div
      className="flex h-full flex-col overflow-hidden rounded-lg border border-line bg-bg text-text"
      onKeyDown={onKeyDown}
      onFocusCapture={(e) =>
        setRowFocused((e.target as HTMLElement).getAttribute("role") === "option")
      }
    >
      <HeaderBar shown={visible.length} total={total} hideCount={storageBad} />
      {storageBad ? null : (
        <div className="shrink-0 px-3 pb-1">
          <input
            ref={inputRef}
            type="text"
            role="searchbox"
            aria-label="履歴を検索"
            placeholder="検索"
            autoFocus
            autoComplete="off"
            spellCheck={false}
            value={query}
            onChange={(e) => {
              select(null);
              setQuery(e.target.value);
            }}
            className="h-6 w-full rounded-sm border border-line bg-surface px-2 text-[12px] text-text outline-none focus:border-accent"
          />
        </div>
      )}
      <div
        className="min-h-0 flex-1 overflow-hidden px-1"
        role="listbox"
        aria-label="履歴"
        aria-busy={history.mutating || undefined}
      >
        {storageBad ? (
          <div
            role="alert"
            className="flex min-h-[72px] flex-col justify-center gap-1 px-2 text-[12px]"
          >
            <p className="text-warning">
              {STORAGE_MESSAGES[paste.status?.storage ?? "error"] ?? STORAGE_MESSAGES.error}
            </p>
            {paste.status?.storage_message ? (
              <p className="font-mono text-[10px] break-all text-text-sub">
                {paste.status.storage_message}
              </p>
            ) : null}
          </div>
        ) : visible.length === 0 ? (
          history.loaded ? (
            <p className="flex min-h-[72px] items-center px-2 text-text-sub">{emptyMessage}</p>
          ) : null
        ) : (
          visible.map((clip, i) => (
            <HistoryItem
              key={clip.id}
              ref={(el) => {
                if (el) rowRefs.current.set(clip.id, el);
                else rowRefs.current.delete(clip.id);
              }}
              clip={clip}
              index={i}
              selected={i === selectedIndex}
              nowMs={nowMs}
              previewOpen={previewOpen}
              onPaste={() => pasteAt(i)}
              onHover={() => {
                if (selectedIdRef.current !== clip.id) select(clip.id);
              }}
              onTogglePreview={() => setPreviewOpen((v) => !v)}
            />
          ))
        )}
      </div>
      {previewOpen && !storageBad ? <PreviewPane clipId={selectedClip?.id ?? null} /> : null}
      <KeyHintBar
        rowFocused={rowFocused}
        noPermissionNote={paste.noPermissionNote}
        sendFailedNote={paste.sendFailedNote}
        listHidden={storageBad}
        onOpenSettings={() => void openSettings()}
      />
      {paste.guideFor !== null && (
        <PermissionGuide
          busy={paste.busy}
          onOpenSettings={() => void paste.openSystemSettings()}
          onLater={() => void paste.guideLater()}
          onClose={() => {
            paste.closeGuide();
            inputRef.current?.focus();
          }}
        />
      )}
    </div>
  );
}
