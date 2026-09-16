import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  DirListing,
  isTauri,
  listLocalDir,
  listRemoteDir,
  listenFileProgress,
  pullFile,
  sendFile,
} from "./lib/api";
import { loadLang, loadTheme, t, TKey } from "./lib/i18n";

function fmtSize(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function joinPath(base: string, name: string) {
  const b = (base || "").replace(/[\\/]+$/, "");
  return (b ? b + "\\" : "") + name;
}

function Pane({
  title,
  listing,
  path,
  onPath,
  onRefresh,
  onSelect,
  onEnterDir,
  selected,
  accent,
  tr,
}: {
  title: string;
  listing: DirListing | null;
  path: string;
  onPath: (p: string) => void;
  onRefresh: () => void;
  onSelect: (name: string, isDir: boolean) => void;
  onEnterDir: (name: string) => void;
  selected: string | null;
  accent: string;
  tr: (k: TKey) => string;
}) {
  return (
    <div
      style={{
        flex: 1,
        minWidth: 0,
        display: "flex",
        flexDirection: "column",
        background: "var(--panel)",
        border: "1px solid var(--border)",
        borderRadius: 10,
        overflow: "hidden",
      }}
    >
      <div
        style={{
          padding: "10px 12px",
          borderBottom: "1px solid var(--border)",
          display: "flex",
          gap: 8,
          alignItems: "center",
        }}
      >
        <strong style={{ fontSize: 13, color: accent, whiteSpace: "nowrap" }}>
          {title}
        </strong>
        <input
          value={path}
          onChange={(e) => onPath(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") onRefresh();
          }}
          style={{
            flex: 1,
            minWidth: 0,
            height: 32,
            background: "var(--input-bg)",
            border: "1px solid var(--border)",
            borderRadius: 6,
            color: "var(--text)",
            padding: "0 10px",
            fontFamily: "Consolas, monospace",
            fontSize: 12,
          }}
        />
        <button
          type="button"
          className="btn btn-ghost"
          onClick={onRefresh}
          style={{ height: 32 }}
          title={tr("loading")}
        >
          ↻
        </button>
      </div>
      <div style={{ flex: 1, overflow: "auto", minHeight: 0 }}>
        {listing?.entries?.length ? (
          listing.entries.map((en) => (
            <button
              key={en.name}
              type="button"
              onClick={() => {
                if (en.isDir) {
                  onEnterDir(en.name);
                } else {
                  onSelect(en.name, false);
                }
              }}
              style={{
                display: "flex",
                width: "100%",
                alignItems: "center",
                justifyContent: "space-between",
                padding: "8px 12px",
                background:
                  selected === en.name ? "var(--accent-soft)" : "transparent",
                border: "none",
                color: "var(--text)",
                cursor: "pointer",
                textAlign: "left",
                fontSize: 13,
              }}
            >
              <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>
                {en.isDir ? "📁 " : "📄 "}
                {en.name}
              </span>
              {!en.isDir && (
                <span style={{ color: "var(--muted)", fontSize: 11 }}>
                  {fmtSize(en.size)}
                </span>
              )}
            </button>
          ))
        ) : (
          <div style={{ padding: 16, color: "var(--muted)", fontSize: 13 }}>
            {listing ? tr("emptyOrCannot") : tr("loading")}
          </div>
        )}
      </div>
    </div>
  );
}

export default function FileTransfer() {
  const lang = loadLang();
  const tr = (k: TKey) => t(lang, k);
  const [localPath, setLocalPath] = useState("");
  const [remotePath, setRemotePath] = useState("");
  const [local, setLocal] = useState<DirListing | null>(null);
  const [remote, setRemote] = useState<DirListing | null>(null);
  const [localSel, setLocalSel] = useState<string | null>(null);
  const [remoteSel, setRemoteSel] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    document.documentElement.dataset.theme = loadTheme();
  }, []);

  const refreshLocal = useCallback(async (p?: string) => {
    if (!isTauri) return;
    try {
      const l = await listLocalDir(p ?? localPath);
      setLocal(l);
      setLocalPath(l.path);
    } catch (e) {
      setStatus(String(e));
    }
  }, [localPath]);

  const refreshRemote = useCallback(async (p?: string) => {
    if (!isTauri) return;
    try {
      const r = await listRemoteDir(p ?? remotePath);
      setRemote(r);
      setRemotePath(r.path);
    } catch (e) {
      setStatus(String(e));
    }
  }, [remotePath]);

  const enterLocalDir = useCallback(
    (name: string) => {
      const next = joinPath(localPath, name);
      void refreshLocal(next);
    },
    [localPath, refreshLocal],
  );

  const enterRemoteDir = useCallback(
    (name: string) => {
      const next = joinPath(remotePath, name);
      void refreshRemote(next);
    },
    [remotePath, refreshRemote],
  );

  useEffect(() => {
    void refreshLocal();
    void refreshRemote();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!isTauri) return;
    let un: (() => void) | null = null;
    listenFileProgress((p) => {
      const done = p.sent ?? p.received ?? 0;
      setStatus(`${p.name}: ${done}${p.total ? ` / ${p.total}` : ""} B`);
    }).then((u) => {
      un = u;
    });
    return () => un?.();
  }, []);

  async function onSend() {
    if (!localSel || !isTauri) return;
    const src = (localPath.replace(/[\\/]+$/, "") + "\\" + localSel).replace(
      /\\\\+/g,
      "\\",
    );
    setBusy(true);
    setStatus(null);
    try {
      const name = await sendFile(src, remotePath);
      setStatus(`${tr("sentTo")} → ${remotePath}\\${name}`);
      await refreshRemote();
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onReceive() {
    if (!remoteSel || !isTauri) return;
    const src = (remotePath.replace(/[\\/]+$/, "") + "\\" + remoteSel).replace(
      /\\\\+/g,
      "\\",
    );
    setBusy(true);
    setStatus(null);
    try {
      const dest = await pullFile(src, localPath);
      setStatus(`${tr("receivedTo")} → ${dest}`);
      await refreshLocal();
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      style={{
        height: "100%",
        display: "flex",
        flexDirection: "column",
        background: "var(--bg)",
        color: "var(--text)",
        padding: 12,
        gap: 10,
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <strong style={{ fontSize: 14 }}>{tr("fileTransfer")}</strong>
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => {
            if (!isTauri) return;
            void getCurrentWindow()
              .destroy()
              .catch(() => getCurrentWindow().close());
          }}
        >
          {tr("close")}
        </button>
      </div>
      <div style={{ display: "flex", gap: 10, flex: 1, minHeight: 0 }}>
        <Pane
          title={tr("thisPcSend")}
          listing={local}
          path={localPath}
          onPath={setLocalPath}
          onRefresh={() => void refreshLocal()}
          onSelect={(n) => setLocalSel(n === localSel ? null : n)}
          onEnterDir={enterLocalDir}
          selected={localSel}
          accent="var(--accent)"
          tr={tr}
        />
        <Pane
          title={tr("remotePcDest")}
          listing={remote}
          path={remotePath}
          onPath={setRemotePath}
          onRefresh={() => void refreshRemote()}
          onSelect={(n) => setRemoteSel(n === remoteSel ? null : n)}
          onEnterDir={enterRemoteDir}
          selected={remoteSel}
          accent="var(--success)"
          tr={tr}
        />
      </div>
      <div style={{ display: "flex", gap: 10, alignItems: "center" }}>
        <button
          type="button"
          className="btn btn-primary"
          disabled={busy || !localSel}
          onClick={() => void onSend()}
        >
          {tr("sendSelected")}
        </button>
        <button
          type="button"
          className="btn btn-ghost"
          disabled={busy || !remoteSel}
          onClick={() => void onReceive()}
        >
          {tr("receiveSelected")}
        </button>
        <span
          style={{ color: "var(--muted)", fontSize: 12, flex: 1, minWidth: 0 }}
        >
          {status}
        </span>
      </div>
      <p style={{ margin: 0, color: "var(--faint)", fontSize: 11.5 }}>
        {tr("fileHint")} {tr("credit")}
      </p>
    </div>
  );
}
