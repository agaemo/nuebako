import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { hideWindow, openSettings } from "../../lib/ipc";
import { onHistoryWindowShown } from "../../lib/events";
import { digitPasteIndex } from "../../lib/keys";
import { useTheme } from "../../lib/useTheme";
import { HeaderBar } from "./HeaderBar";
import { HistoryItem } from "./HistoryItem";
import { KeyHintBar, type FooterNote } from "./KeyHintBar";
import { PermissionGuide } from "./PermissionGuide";
import { PreviewPane } from "./PreviewPane";
import { StatusBand } from "./StatusBand";
import { isStorageBad, statusMessages } from "./statusMessages";
import { unlistenAll } from "./unlisten";
import { nextSelectionAfterDelete, useHistory } from "./useHistory";
import { useNotice } from "./useNotice";
import { usePaste } from "./usePaste";
import { ROW_HEIGHT, windowHeight } from "./windowHeight";

const WINDOW_WIDTH = 420;

/**
 * 変換確定の Enter を貼り付けと取り違えないための猶予(ms)。
 * WebKit では compositionend の後に確定の keydown が届き、その時点の isComposing は false になる。
 */
const IME_GRACE_MS = 50;

/** 末尾までこの行数以内に近づいたら、続きを読み込む。 */
const LOAD_AHEAD_ROWS = 5;

/** 履歴ウィンドウと、権限の案内。 */
export function HistoryWindow() {
  useTheme();
  const { notice, notify, clear: clearNotice } = useNotice();
  const history = useHistory();
  const { items, matched, total, query, setQuery, nowMs, remove, togglePin, loadMore } = history;
  const paste = usePaste({ onError: notify });

  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [rowFocused, setRowFocused] = useState(false);
  const [focusReq, setFocusReq] = useState<{ id: number | null } | null>(null);
  const [bandHeight, setBandHeight] = useState(0);

  const inputRef = useRef<HTMLInputElement>(null);
  const bandRef = useRef<HTMLDivElement>(null);
  const rowRefs = useRef(new Map<number, HTMLDivElement>());
  const composingRef = useRef(false);
  const compositionEndedAtRef = useRef(0);
  const os = paste.status?.os ?? "";

  const storageBad = isStorageBad(paste.status);

  // 選択は選択中の id だけを持ち、位置は一覧から導く(その行が消えたら先頭になる)
  const selectedIndex = Math.max(
    0,
    items.findIndex((c) => c.id === selectedId),
  );
  const selectedClip = items[selectedIndex] ?? null;

  const bandMessages = useMemo(
    () =>
      statusMessages({
        status: paste.status,
        noPermission: paste.noPermissionNote,
        listStale: history.loadError !== null && items.length > 0,
      }),
    [paste.status, paste.noPermissionNote, history.loadError, items.length],
  );
  const bandKey = bandMessages.map((m) => m.key).join("|");

  // 表示のたびに、選択を先頭に戻して検索欄にフォーカスする(一覧は useHistory が取り直す)
  useEffect(() => {
    inputRef.current?.focus();
    const un = onHistoryWindowShown(() => {
      setSelectedId(null);
      setFocusReq({ id: null });
    });
    return () => unlistenAll([un]);
  }, []);

  // フォーカスの移動(削除後、矢印キー)。描画の後に行い、10行を超える一覧でも選択行が見えるようにする
  useEffect(() => {
    if (!focusReq) return;
    if (focusReq.id === null) {
      inputRef.current?.focus();
      return;
    }
    const row = rowRefs.current.get(focusReq.id);
    row?.focus();
    row?.scrollIntoView?.({ block: "nearest" });
  }, [focusReq]);

  // 選択が読み込み済みの末尾に近づいたら、続きを読み込む(スクロールのイベントが出ない操作のため)
  useEffect(() => {
    if (items.length - selectedIndex <= LOAD_AHEAD_ROWS) void loadMore();
  }, [items.length, selectedIndex, loadMore]);

  // 状態の帯の高さは、文の長さで変わるので、描画後に測ってウィンドウの高さに足す
  useLayoutEffect(() => {
    setBandHeight(storageBad ? 0 : (bandRef.current?.offsetHeight ?? 0));
  }, [bandKey, storageBad]);

  const previewKind = selectedClip?.kind;
  const height = windowHeight(
    storageBad ? 0 : items.length,
    previewOpen && !storageBad,
    previewKind,
    bandHeight,
  );
  // 幅は、利用者が広げたものを保つ(高さだけを内容に合わせる)。
  // 幅の取得を待つ間に次の高さが決まることがあるので、古い計算は破棄して、最後の高さだけを反映する
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const win = getCurrentWindow();
      let width = WINDOW_WIDTH;
      try {
        const [size, scale] = await Promise.all([win.innerSize(), win.scaleFactor()]);
        if (size.width > 0 && scale > 0) width = Math.max(WINDOW_WIDTH, size.width / scale);
      } catch {
        // 取得できないときは、既定の幅にする
      }
      if (cancelled) return;
      await win.setSize(new LogicalSize(width, height));
    })().catch(() => {
      // テストや Web 表示など、ウィンドウが無い環境では無視する
    });
    return () => {
      cancelled = true;
    };
  }, [height]);

  const select = useCallback((id: number | null) => setSelectedId(id), []);

  const move = (delta: number) => {
    if (items.length === 0) return;
    const next = items[Math.min(items.length - 1, Math.max(0, selectedIndex + delta))];
    select(next.id);
    setFocusReq({ id: next.id });
  };

  /** 旧一覧への操作を避けるため、取り直しや変更の応答待ちの間は貼り付けも削除もしない。 */
  const busyWithList = history.stale || history.mutating;

  const pasteAt = (index: number) => {
    const clip = items[index];
    if (!clip || busyWithList) return;
    clearNotice();
    void paste.paste(clip.id);
  };

  /** 削除。次の行(無ければ前の行、0件なら検索欄)へ選択とフォーカスを移す。 */
  const deleteSelected = async () => {
    if (!selectedClip || busyWithList) return;
    const id = selectedClip.id;
    const nextId = nextSelectionAfterDelete(items, id);
    clearNotice();
    try {
      await remove(id);
    } catch {
      notify("削除に失敗しました");
      return;
    }
    select(nextId);
    setFocusReq({ id: nextId });
  };

  const togglePinSelected = () => {
    if (!selectedClip || busyWithList) return;
    clearNotice();
    togglePin(selectedClip.id, !selectedClip.pinned).catch(() =>
      notify("ピン留めの変更に失敗しました"),
    );
  };

  /** 変換中のキーか。変換の確定や候補選択の Enter・矢印・Esc を、操作として扱わないために見分ける。 */
  const isImeKey = (e: React.KeyboardEvent) =>
    composingRef.current ||
    e.nativeEvent.isComposing ||
    e.nativeEvent.keyCode === 229 ||
    e.key === "Process" ||
    Date.now() - compositionEndedAtRef.current < IME_GRACE_MS;

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (paste.guideFor !== null) return;
    if (isImeKey(e)) return;
    const target = e.target as HTMLElement;
    const digit = digitPasteIndex(e.nativeEvent, os);
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
    if (target.getAttribute("role") === "option") {
      // Mac のノートはキーボードに Delete が無いので、OS の流儀の削除(Cmd/Ctrl+Backspace)も受ける
      const modifiedBackspace =
        e.key === "Backspace" &&
        !e.altKey &&
        !e.shiftKey &&
        (os === "macos" ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey);
      if (e.key === "Delete" || modifiedBackspace) {
        e.preventDefault();
        void deleteSelected();
      } else if (!mod && !e.altKey) {
        if (e.key === "p" || e.key === "P") {
          e.preventDefault();
          togglePinSelected();
        } else if (e.key === "Backspace" || e.key.length === 1) {
          // 行にフォーカスがあるときの文字入力は、検索欄への入力に戻す
          inputRef.current?.focus();
        }
      }
    }
  };

  const onListScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    if (el.scrollHeight - el.scrollTop - el.clientHeight <= ROW_HEIGHT * LOAD_AHEAD_ROWS) {
      void loadMore();
    }
  };

  const emptyMessage =
    query === ""
      ? "まだ履歴がありません。何かコピーすると、ここに並びます"
      : "一致する履歴がありません";

  const footerNote: FooterNote | null = notice
    ? { text: notice, tone: "error" }
    : paste.focusLostNote
      ? { text: "貼り付け先を確認できなかったため、コピーのみ行いました", tone: "warning" }
      : paste.sendFailedNote
        ? { text: "貼り付けの送信に失敗しました(内容はコピー済みです)", tone: "warning" }
        : null;

  const band =
    bandMessages.length > 0 ? <StatusBand ref={bandRef} messages={bandMessages} /> : null;

  return (
    <div
      className="flex h-full flex-col overflow-hidden rounded-lg border border-line bg-bg text-text"
      onKeyDown={onKeyDown}
      onCompositionStart={() => {
        composingRef.current = true;
      }}
      onCompositionEnd={() => {
        composingRef.current = false;
        compositionEndedAtRef.current = Date.now();
      }}
      onFocusCapture={(e) =>
        setRowFocused((e.target as HTMLElement).getAttribute("role") === "option")
      }
    >
      <HeaderBar matched={matched} total={total} hideCount={storageBad} />
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
      {storageBad ? null : band}
      {storageBad ? (
        <div className="min-h-[72px] flex-1">{band}</div>
      ) : (
        <div
          className="max-h-60 min-h-0 flex-1 overflow-y-auto px-1"
          role="listbox"
          aria-label="履歴"
          aria-busy={history.mutating || undefined}
          onScroll={onListScroll}
        >
          {items.length === 0 ? (
            history.loaded ? (
              history.loadError !== null ? (
                <p role="alert" className="flex min-h-[72px] items-center px-2 text-error">
                  履歴を取得できませんでした。もう一度開いてください
                </p>
              ) : (
                <p className="flex min-h-[72px] items-center px-2 text-text-sub">{emptyMessage}</p>
              )
            ) : null
          ) : (
            items.map((clip, i) => (
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
                onTogglePreview={() => setPreviewOpen((v) => !v)}
              />
            ))
          )}
        </div>
      )}
      {previewOpen && !storageBad ? (
        <PreviewPane clipId={selectedClip?.id ?? null} tall={previewKind === "image"} />
      ) : null}
      <KeyHintBar
        rowFocused={rowFocused}
        os={os}
        note={footerNote}
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
