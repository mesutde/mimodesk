import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  disconnect as apiDisconnect,
  focusMainWindow,
  getDeskInfo,
  getRemoteMonitors,
  isTauri,
  listenDisconnected,
  listenFileProgress,
  listenPing,
  listenRemoteFrame,
  MonitorInfo,
  sendFile,
  sendInput,
  setMonitor,
  RemoteFrame,
} from "./lib/api";
import { loadLang, loadTheme, t, TKey } from "./lib/i18n";

function pingColor(ms: number) {
  if (ms <= 40) return "#3ddc84";
  if (ms <= 90) return "#f5a524";
  return "#ff5c5c";
}

function cursorCss(kind?: number): string {
  // NOTE: kind 3 ("wait") is deliberately shown as the normal arrow. The host
  // draws the real cursor into the video frame, and the old WAIT mapping used
  // to leave the viewer stuck on a loading spinner.
  switch (kind) {
    case 1:
      return "text";
    case 2:
      return "pointer";
    case 3:
      return "default";
    case 4:
      return "ns-resize";
    case 5:
      return "ew-resize";
    case 6:
      return "move";
    case 7:
      return "crosshair";
    case 8:
      return "nwse-resize";
    case 9:
      return "nesw-resize";
    default:
      return "default";
  }
}

export default function RemoteWindow() {
  const lang = loadLang();
  const tr = (k: TKey) => t(lang, k);
  const [frame, setFrame] = useState<RemoteFrame | null>(null);
  const [url, setUrl] = useState<string | null>(null);
  const [maximized, setMaximized] = useState(false);
  const [trueFs, setTrueFs] = useState(false);
  const [viewMode, setViewMode] = useState<"fit" | "actual">("fit");
  const [cursorKind, setCursorKind] = useState(0);
  const [zoomPct, setZoomPct] = useState<number | null>(null);
  const [monitors, setMonitors] = useState<MonitorInfo[] | null>(null);
  // Which remote monitor is being viewed (index in `monitors`).
  const [monitorIdx, setMonitorIdx] = useState<number | null>(null);
  // The monitor the incoming frames actually come from (host confirms it).
  const [liveMonitor, setLiveMonitor] = useState<number | null>(null);
  const liveMonitorRef = useRef<number | null>(null);
  const [switching, setSwitching] = useState(false);
  const switchingRef = useRef(false);
  const [session, setSession] = useState<number | null>(null);
  const [ping, setPing] = useState<number | null>(null);
  const [dropHint, setDropHint] = useState<string | null>(null);
  const [xfer, setXfer] = useState<{
    name: string;
    path?: string;
    sent: number;
    total: number;
  } | null>(null);
  const imgRef = useRef<HTMLImageElement | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const stageRef = useRef<HTMLDivElement | null>(null);
  const remoteSize = useRef({ w: 1, h: 1 });
  const lastMove = useRef(0);
  const ending = useRef(false);
  const fsRef = useRef(false);

  useEffect(() => {
    fsRef.current = maximized;
  }, [maximized]);

  // Keep maximize/fullscreen button labels in sync when the user exits the
  // window mode outside the buttons (e.g. ESC in exclusive fullscreen).
  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    const sync = async () => {
      try {
        const win = getCurrentWindow();
        const [fs, mx] = await Promise.all([
          win.isFullscreen().catch(() => false),
          win.isMaximized().catch(() => false),
        ]);
        if (!cancelled) {
          setTrueFs(!!fs);
          setMaximized(!!mx);
        }
      } catch {
        /* ignore */
      }
    };
    const onFsChange = () => void sync();
    document.addEventListener("fullscreenchange", onFsChange);
    window.addEventListener("resize", onFsChange);
    window.addEventListener("focus", onFsChange);
    void sync();
    const timer = setInterval(() => void sync(), 2000);
    return () => {
      cancelled = true;
      clearInterval(timer);
      document.removeEventListener("fullscreenchange", onFsChange);
      window.removeEventListener("resize", onFsChange);
      window.removeEventListener("focus", onFsChange);
    };
  }, []);

  // Zoom % so the user sees why the remote screen looks smaller than native.
  useEffect(() => {
    function updateZoom() {
      const stage = stageRef.current;
      if (!stage || viewMode !== "fit") {
        setZoomPct(null);
        return;
      }
      const rect = stage.getBoundingClientRect();
      const rw = remoteSize.current.w;
      const rh = remoteSize.current.h;
      if (rect.width < 1 || rect.height < 1 || rw < 1 || rh < 1) {
        setZoomPct(null);
        return;
      }
      const pct = Math.floor(
        Math.min(rect.width / rw, rect.height / rh) * 100,
      );
      setZoomPct(pct > 0 ? pct : null);
    }
    updateZoom();
    window.addEventListener("resize", updateZoom);
    return () => window.removeEventListener("resize", updateZoom);
  }, [frame, viewMode, maximized, trueFs]);

  // Window-level keyboard capture — more reliable than depending on element focus.
  useEffect(() => {
    function handle(e: KeyboardEvent, down: boolean) {
      if (e.key === "Escape" && !fsRef.current) return;
      const target = e.target as HTMLElement | null;
      if (
        !fsRef.current &&
        target &&
        (target.tagName === "BUTTON" || target.tagName === "INPUT")
      ) {
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      const mods = {
        ctrl: e.ctrlKey,
        shift: e.shiftKey,
        alt: e.altKey,
        meta: e.metaKey,
      };
      const hasMod = mods.ctrl || mods.alt || mods.meta;
      if (down && e.key.length === 1 && !hasMod) {
        void sendInput({ t: "text", s: e.key }).catch(() => {});
        return;
      }
      void sendInput({ t: "key", key: e.key, down, ...mods }).catch(() => {});
    }
    const kd = (e: KeyboardEvent) => handle(e, true);
    const ku = (e: KeyboardEvent) => handle(e, false);
    window.addEventListener("keydown", kd);
    window.addEventListener("keyup", ku);
    rootRef.current?.focus();
    return () => {
      window.removeEventListener("keydown", kd);
      window.removeEventListener("keyup", ku);
    };
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = loadTheme();
  }, []);

  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    listenDisconnected(() => {
      if (cancelled) return;
      void apiDisconnect().catch(() => {});
      void getCurrentWindow().close().catch(() => window.close());
    }).catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    listenPing((ms) => {
      if (!cancelled) setPing(ms);
    }).catch(() => {});
    listenFileProgress((p) => {
      if (cancelled) return;
      const done = p.sent ?? p.received ?? 0;
      setXfer({
        name: p.name,
        sent: done,
        total: p.total || done,
      });
      if (p.total && done >= p.total) {
        setTimeout(() => setXfer(null), 3500);
      }
    }).catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  // Tauri drag-drop gives real filesystem paths.
  useEffect(() => {
    if (!isTauri) return;
    let un: (() => void) | null = null;
    (async () => {
      const win = getCurrentWindow();
      un = await win.onDragDropEvent(async (event) => {
        if (event.payload.type !== "drop") {
          if (event.payload.type === "enter" || event.payload.type === "over") {
            setDropHint(tr("dropToSend"));
          }
          return;
        }
        setDropHint(null);
        const paths = event.payload.paths;
        if (!paths?.length) return;
        const path = paths[0];
        const name = path.split(/[\\/]/).pop() || path;
        const dest = window.prompt(
          `${tr("dropDestPrompt")}\n\n${name}`,
          "",
        );
        if (dest === null) return;
        setXfer({ name, sent: 0, total: 0 });
        try {
          await sendFile(path, dest.trim());
          setDropHint(
            `${tr("sentTo")}: ${name}${dest.trim() ? ` → ${dest.trim()}` : ` ${tr("destDefault")}`}`,
          );
          setTimeout(() => setDropHint(null), 5000);
        } catch (e) {
          setDropHint(String(e));
          setXfer(null);
        }
      });
    })();
    return () => {
      un?.();
    };
  }, []);

  useEffect(() => {
    if (!frame) return;
    remoteSize.current = {
      w: frame.nativeWidth || frame.width,
      h: frame.nativeHeight || frame.height,
    };
    setCursorKind(frame.cursor ?? 0);
    if (frame.monitor != null) {
      setLiveMonitor(frame.monitor);
      liveMonitorRef.current = frame.monitor;
      // Host auto-reverts after a failed switch — keep the selector honest.
      if (!switchingRef.current) {
        setMonitorIdx((cur) =>
          cur != null && cur !== frame.monitor ? frame.monitor! : cur,
        );
      }
    }
    const bin = atob(frame.jpeg);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    const blobUrl = URL.createObjectURL(
      new Blob([bytes], { type: "image/jpeg" }),
    );
    setUrl((old) => {
      if (old) URL.revokeObjectURL(old);
      return blobUrl;
    });
  }, [frame]);

  useEffect(() => {
    if (!isTauri) return;
    let un: (() => void) | null = null;
    let cancelled = false;
    // Coalesce bursts to one paint per animation frame so base64 decode +
    // object-URL churn cannot freeze the webview during monitor switches.
    let pending: RemoteFrame | null = null;
    let raf = 0;
    const flush = () => {
      raf = 0;
      const f = pending;
      pending = null;
      if (!cancelled && f) setFrame(f);
    };
    listenRemoteFrame((f) => {
      if (cancelled) return;
      pending = f;
      if (!raf) raf = requestAnimationFrame(flush);
    }).then((u) => {
      if (cancelled) u();
      else un = u;
    });
    return () => {
      cancelled = true;
      if (raf) cancelAnimationFrame(raf);
      un?.();
    };
  }, []);

  // Remote monitor list (for multi-monitor hosts). Old hosts don't know this
  // command — then the selector simply stays hidden.
  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    (async () => {
      try {
        const info = await getDeskInfo();
        if (!cancelled) setSession(info.session ?? null);
      } catch {
        /* ignore */
      }
      // Wait a beat so the session is up before asking.
      await new Promise((r) => setTimeout(r, 1200));
      for (let attempt = 0; attempt < 4; attempt++) {
        try {
          const list = await getRemoteMonitors();
          if (cancelled) return;
          if (list?.length) {
            setMonitors(list);
            const primary = list.find((m) => m.primary) ?? list[0];
            setMonitorIdx(primary.index);
            return;
          }
        } catch {
          /* host may not support it yet — retry */
        }
        await new Promise((r) => setTimeout(r, 1500));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Closed-loop monitor switch: the host confirms via frame.monitor
  // (liveMonitor). Resend once when unconfirmed, then fall back — and tell
  // the host too, otherwise it keeps capturing a dead monitor (frozen screen,
  // input remapped to the wrong origin).
  async function revertToLive() {
    const fallback = liveMonitorRef.current;
    setMonitorIdx(fallback);
    if (fallback != null) {
      try {
        await setMonitor(fallback);
      } catch {
        /* host may already be on it */
      }
    }
    setDropHint(tr("monitorSwitchFail"));
    setTimeout(() => setDropHint(null), 4000);
  }

  async function onSelectMonitor(index: number) {
    setMonitorIdx(index);
    setSwitching(true);
    switchingRef.current = true;
    try {
      await setMonitor(index);
      for (let round = 0; round < 2; round++) {
        for (let i = 0; i < 10; i++) {
          await new Promise((r) => setTimeout(r, 150));
          if (liveMonitorRef.current === index) break;
        }
        if (liveMonitorRef.current === index) break;
        // Unconfirmed — resend once before giving up.
        try {
          await setMonitor(index);
        } catch {
          break;
        }
      }
      if (liveMonitorRef.current !== index) {
        await revertToLive();
      }
    } catch {
      await revertToLive();
    } finally {
      switchingRef.current = false;
      setSwitching(false);
    }
  }

  async function toggleFullscreen() {
    if (!isTauri) return;
    // Maximize keeps the local Windows taskbar visible. The whole remote
    // screen (including its taskbar / Start menu) is still shown because the
    // image below uses fit/contain in the remaining area.
    const win = getCurrentWindow();
    try {
      if (maximized) {
        await win.unmaximize();
        setMaximized(false);
      } else {
        await win.maximize();
        setMaximized(true);
      }
    } catch {
      setMaximized((m) => !m);
    }
  }

  async function goFit() {
    // Only fit the image into the current window — never resize/maximize the
    // window itself. The whole remote screen (incl. its taskbar) is shown via
    // contain scaling in the remaining area below the toolbar.
    setViewMode("fit");
  }

  async function toggleTrueFullscreen() {
    if (!isTauri) return;
    const win = getCurrentWindow();
    try {
      const next = !trueFs;
      await win.setFullscreen(next);
      setTrueFs(next);
      // Exclusive fullscreen would otherwise keep a 1:1 scrolled view where
      // the remote taskbar stays out of sight — always fit the whole screen.
      if (next) setViewMode("fit");
    } catch {
      setTrueFs((v) => !v);
    }
  }

  async function onClose() {
    if (ending.current) return;
    ending.current = true;
    try {
      if (isTauri) {
        await apiDisconnect().catch(() => {});
        await focusMainWindow().catch(() => {});
        const win = getCurrentWindow();
        // destroy() force-closes; close() can leave a blank shell.
        try {
          await win.destroy();
        } catch {
          await win.close();
        }
      } else {
        window.close();
      }
    } catch {
      try {
        window.close();
      } catch {
        /* ignore */
      }
    }
  }

  function mapCoords(clientX: number, clientY: number) {
    const el = imgRef.current;
    if (!el) return null;
    const rect = el.getBoundingClientRect();
    if (rect.width < 1 || rect.height < 1) return null;
    const x = Math.round(
      ((clientX - rect.left) / rect.width) * remoteSize.current.w,
    );
    const y = Math.round(
      ((clientY - rect.top) / rect.height) * remoteSize.current.h,
    );
    return { x, y };
  }

  function onMove(e: React.MouseEvent) {
    const now = performance.now();
    if (now - lastMove.current < 12) return;
    lastMove.current = now;
    const c = mapCoords(e.clientX, e.clientY);
    if (c) void sendInput({ t: "move", x: c.x, y: c.y }).catch(() => {});
  }

  function onDown(e: React.MouseEvent) {
    e.preventDefault();
    rootRef.current?.focus();
    const btn = e.button === 2 ? "right" : e.button === 1 ? "middle" : "left";
    const c = mapCoords(e.clientX, e.clientY);
    if (c) {
      void sendInput({ t: "move", x: c.x, y: c.y }).catch(() => {});
      void sendInput({ t: "down", btn }).catch(() => {});
    }
  }

  function onUp(e: React.MouseEvent) {
    const btn = e.button === 2 ? "right" : e.button === 1 ? "middle" : "left";
    void sendInput({ t: "up", btn }).catch(() => {});
  }

  function onWheel(e: React.WheelEvent) {
    void sendInput({ t: "wheel", dy: e.deltaY > 0 ? -3 : 3 }).catch(() => {});
  }

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      style={{
        height: "100%",
        display: "flex",
        flexDirection: "column",
        background: "var(--remote-bg, #0a0c10)",
        outline: "none",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: maximized ? "6px 12px" : "10px 14px",
          background: "var(--panel, #1c212a)",
          borderBottom: "1px solid var(--border, #343c4a)",
          gap: 12,
          flexShrink: 0,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span
            style={{
              width: 8,
              height: 8,
              borderRadius: "50%",
              background: "var(--accent, #2b8fef)",
              boxShadow: "0 0 0 3px var(--accent-soft, rgba(43,143,239,0.14))",
            }}
          />
          <strong style={{ fontSize: 13, color: "var(--text, #e8ecf1)" }}>
            MimoDesk
          </strong>
          {ping != null && (
            <span
              style={{
                fontSize: 12,
                fontFamily: "Consolas, monospace",
                fontWeight: 700,
                color: pingColor(ping),
                background: "var(--panel-3, #2c3441)",
                border: "1px solid var(--border, #343c4a)",
                borderRadius: 999,
                padding: "2px 10px",
              }}
              title="Ping"
            >
              {ping} ms
            </span>
          )}
          {frame && (
            <span
              style={{
                color: "var(--muted, #8b949e)",
                fontSize: 12,
                fontFamily: "Consolas, monospace",
              }}
            >
              {frame.nativeWidth}×{frame.nativeHeight}
              {liveMonitor != null ? ` · M${liveMonitor + 1}` : ""}
              {viewMode === "fit" && zoomPct != null ? ` · %${zoomPct}` : ""}
              {session != null && session > 0 ? ` · #${session}` : ""}
            </span>
          )}
        </div>
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap", alignItems: "center" }}>
          {monitors && monitors.length > 1 && (
            <select
              value={monitorIdx ?? liveMonitor ?? monitors.find((m) => m.primary)?.index ?? monitors[0].index}
              onChange={(e) => void onSelectMonitor(Number(e.target.value))}
              title={switching ? tr("monitorSwitching") : tr("monitor")}
              disabled={switching}
              style={{
                background: "var(--panel-3, #2c3441)",
                color: "var(--text, #e8ecf1)",
                border: "1px solid var(--border, #343c4a)",
                borderRadius: 6,
                padding: "5px 8px",
                fontSize: 12.5,
                cursor: "pointer",
              }}
            >
              {monitors.map((m) => (
                <option key={m.index} value={m.index}>
                  {tr("monitor")} {m.index + 1} · {m.width}×{m.height}
                  {m.primary ? " ★" : ""}
                </option>
              ))}
            </select>
          )}
          <button
            type="button"
            className={viewMode === "fit" ? "btn btn-primary" : "btn btn-ghost"}
            onClick={() => void goFit()}
            title="Fit entire remote screen in window"
          >
            {tr("fitScreen")}
          </button>
          <button
            type="button"
            className={viewMode === "actual" ? "btn btn-primary" : "btn btn-ghost"}
            onClick={() => setViewMode("actual")}
            title="1:1 pixels"
          >
            1:1
          </button>
          <button type="button" className="btn btn-ghost" onClick={toggleFullscreen}>
            {maximized ? tr("exitFs") : tr("fullscreen")}
          </button>
          <button type="button" className="btn btn-ghost" onClick={toggleTrueFullscreen}>
            {trueFs ? tr("exitTrueFs") : tr("trueFs")}
          </button>
          <button type="button" className="btn btn-danger" onClick={onClose}>
            {tr("cut")}
          </button>
        </div>
      </div>
      <div
        ref={stageRef}
        style={{
          flex: 1,
          minHeight: 0,
          display: "grid",
          placeItems: "center",
          padding: viewMode === "fit" ? (maximized || trueFs ? 0 : 4) : 0,
          overflow: viewMode === "actual" ? "auto" : "hidden",
          position: "relative",
        }}
        onDragOver={(e) => e.preventDefault()}
      >
        {url ? (
          <img
            ref={imgRef}
            src={url}
            alt="Remote screen"
            draggable={false}
            onMouseMove={onMove}
            onMouseDown={onDown}
            onMouseUp={onUp}
            onWheel={onWheel}
            onContextMenu={(e) => e.preventDefault()}
            style={{
              width: viewMode === "fit" ? "100%" : undefined,
              height: viewMode === "fit" ? "100%" : undefined,
              maxWidth: viewMode === "fit" ? "100%" : "none",
              maxHeight: viewMode === "fit" ? "100%" : "none",
              objectFit: viewMode === "fit" ? "contain" : undefined,
              cursor: cursorCss(cursorKind),
              userSelect: "none",
              display: "block",
              margin: viewMode === "fit" ? "auto" : undefined,
            }}
          />
        ) : (
          <div style={{ color: "var(--muted)", textAlign: "center", lineHeight: 1.6 }}>
            <div style={{ fontWeight: 600, marginBottom: 6 }}>{tr("waiting")}</div>
            <div style={{ fontSize: 12.5 }}>{tr("waitingHelp")}</div>
          </div>
        )}
        {dropHint && (
          <div
            style={{
              position: "absolute",
              bottom: 16,
              left: "50%",
              transform: "translateX(-50%)",
              background: "var(--panel)",
              border: "1px solid var(--border)",
              color: "var(--text)",
              padding: "8px 14px",
              borderRadius: 8,
              fontSize: 12.5,
              zIndex: 5,
            }}
          >
            {dropHint}
          </div>
        )}
        {xfer && (
          <div
            style={{
              position: "absolute",
              top: 16,
              right: 16,
              background: "var(--panel)",
              border: "1px solid var(--border)",
              borderRadius: 10,
              padding: "10px 14px",
              minWidth: 220,
              zIndex: 6,
              boxShadow: "0 8px 24px rgba(0,0,0,0.35)",
            }}
          >
            <div
              style={{
                fontSize: 12,
                fontWeight: 600,
                color: "var(--text)",
                marginBottom: 6,
              }}
            >
              {xfer.name}
            </div>
            <div
              style={{
                height: 6,
                background: "var(--panel-3)",
                borderRadius: 99,
                overflow: "hidden",
              }}
            >
              <div
                style={{
                  height: "100%",
                  width:
                    xfer.total > 0
                      ? `${Math.min(100, Math.round((xfer.sent / xfer.total) * 100))}%`
                      : "8%",
                  background: "var(--accent)",
                  transition: "width 0.2s",
                }}
              />
            </div>
            <div
              style={{
                marginTop: 6,
                fontSize: 11,
                color: "var(--muted)",
                fontFamily: "Consolas, monospace",
              }}
            >
              {xfer.total > 0
                ? `${xfer.sent} / ${xfer.total} bytes`
                : tr("loading")}
            </div>
            <div
              style={{
                marginTop: 4,
                fontSize: 10.5,
                color: "var(--faint)",
                wordBreak: "break-all",
              }}
            >
              {tr("destDefault")}
            </div>
          </div>
        )}
      </div>
      <p
        style={{
          margin: 0,
          padding: "8px 16px",
          color: "var(--faint)",
          fontSize: 11.5,
          borderTop: "1px solid var(--border)",
          flexShrink: 0,
        }}
      >
        {tr("keyboardHint")} {tr("credit")}
      </p>
    </div>
  );
}
