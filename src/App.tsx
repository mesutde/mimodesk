import { useCallback, useEffect, useMemo, useState } from "react";
import {
  answerConnection,
  APP_VERSION,
  closeRemoteWindow,
  connectRemote,
  DeskInfo,
  disconnect as apiDisconnect,
  focusMainWindow,
  getAccessSettings,
  getDeskInfo,
  isTauri,
  listenIncomingConnection,
  mockDesk,
  openFileTransferWindow,
  openRemoteWindow,
  QualityLevel,
  regenerateId,
  setAccessPassword,
  setAlias,
  setQuality,
  setRequireApproval,
} from "./lib/api";
import {
  detectLang,
  formatId,
  Lang,
  loadLang,
  loadTheme,
  saveLang,
  saveTheme,
  t,
  TKey,
} from "./lib/i18n";
import {
  IconCopy,
  IconDashboard,
  IconPower,
  IconSessions,
  IconSettings,
} from "./components/Icons";

type Page = "desk" | "sessions" | "settings";

export default function App() {
  const [lang, setLangState] = useState<Lang>(() => loadLang());
  const [theme, setTheme] = useState<"dark" | "light">(() => loadTheme());
  const [page, setPage] = useState<Page>("desk");
  const [desk, setDesk] = useState<DeskInfo>(mockDesk);
  const [remoteId, setRemoteId] = useState("");
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [aliasDraft, setAliasDraft] = useState(desk.alias);
  const [quality, setQ] = useState<QualityLevel>("high");
  const [showFull, setShowFull] = useState(false);
  const [accessPass, setAccessPass] = useState("");
  const [viewOnly, setViewOnly] = useState(false);
  const [hostPass, setHostPass] = useState("");
  const [requireApproval, setReqApproval] = useState(false);
  const [incoming, setIncoming] = useState<{
    peer: string;
    alias: string;
    viewOnly: boolean;
  } | null>(null);

  const tr = useCallback((key: TKey) => t(lang, key), [lang]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    saveTheme(theme);
  }, [theme]);

  const refresh = useCallback(async () => {
    if (!isTauri) return;
    try {
      const info = await getDeskInfo();
      setDesk(info);
      setAliasDraft(info.alias);
      if (info.quality === "Düşük" || info.quality === "Low") setQ("low");
      else if (info.quality === "Orta" || info.quality === "Medium")
        setQ("medium");
      else if (info.quality === "Yüksek" || info.quality === "High")
        setQ("high");
    } catch (e) {
      console.error(e);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 3000);
    return () => clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    if (!isTauri) return;
    void getAccessSettings()
      .then((s) => {
        setHostPass(s.password ?? "");
        setReqApproval(s.requireApproval);
      })
      .catch(() => {});
    let un: (() => void) | null = null;
    let cancelled = false;
    listenIncomingConnection((p) => {
      if (!cancelled) {
        setIncoming(p);
        // Bring host UI to front so Allow is not missed.
        void focusMainWindow().catch(() => {});
      }
    }).then((u) => {
      if (cancelled) u();
      else un = u;
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, []);

  const displayId = useMemo(() => formatId(desk.address.replace(/\D/g, "")), [
    desk.address,
  ]);
  const idPairs = useMemo(
    () => displayId.split("-").filter(Boolean),
    [displayId],
  );

  function setLang(next: Lang) {
    setLangState(next);
    saveLang(next);
  }

  async function onConnect() {
    // Already connected → just focus the remote window. Reconnecting on top
    // of a live session used to run two video pipelines side by side
    // (flickering monitors); use Kes first for a clean reconnect.
    if (desk.status === "connected") {
      if (isTauri) await openRemoteWindow();
      return;
    }
    const trimmed = remoteId.trim();
    if (!trimmed) return;
    setBusy(true);
    setToast(null);
    try {
      const info = isTauri
        ? await connectRemote(
            trimmed,
            accessPass.trim() || undefined,
            viewOnly,
          )
        : mockDesk;
      if (!isTauri) await new Promise((r) => setTimeout(r, 500));
      setDesk(info);
      const rtt = info.link?.rttMs;
      let msg =
        rtt != null
          ? `${tr("connected")} · ${rtt} ms · ${info.link?.mode}`
          : tr("connected");
      if (info.remoteVersion && info.remoteVersion !== APP_VERSION) {
        msg += ` · ${tr("versionMismatch")}: ${info.remoteVersion}`;
      }
      setToast(msg);
      await openRemoteWindow();
    } catch (e) {
      setToast(String(e) || tr("statusError"));
      if (isTauri) await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function onDisconnect() {
    setBusy(true);
    try {
      if (isTauri) {
        await apiDisconnect();
        await closeRemoteWindow();
        await refresh();
      }
      setToast(tr("sessionClosed"));
    } finally {
      setBusy(false);
    }
  }

  async function onSendFile() {
    if (!isTauri) return;
    setBusy(true);
    setToast(null);
    try {
      if (desk.status !== "connected") {
        const trimmed = remoteId.trim();
        if (!trimmed) {
          setToast(tr("target"));
          return;
        }
        const info = await connectRemote(trimmed, accessPass.trim() || undefined, viewOnly);
        setDesk(info);
        await openRemoteWindow();
      }
      await openFileTransferWindow();
    } catch (e) {
      setToast(String(e));
      if (isTauri) await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function onCopyId() {
    const digits = desk.address.replace(/\D/g, "");
    try {
      await navigator.clipboard.writeText(digits);
      setToast(tr("copiedId"));
    } catch {
      setToast("—");
    }
  }

  async function onCopyFull() {
    try {
      await navigator.clipboard.writeText(desk.rawAddress);
      setToast(tr("copiedFull"));
    } catch {
      setToast("—");
    }
  }

  async function onRestartHost() {
    setBusy(true);
    try {
      if (isTauri) setDesk(await regenerateId());
      setToast(tr("hostRestarted"));
    } catch (e) {
      setToast(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onQuality(level: QualityLevel) {
    setQ(level);
    if (!isTauri) return;
    try {
      await setQuality(level);
      setToast(
        level === "high"
          ? tr("qualityHighHint")
          : level === "medium"
            ? tr("qualityMedHint")
            : tr("qualityLowHint"),
      );
    } catch (e) {
      setToast(String(e));
    }
  }

  async function onSaveAlias() {
    if (!isTauri) return;
    try {
      await setAlias(aliasDraft);
    } catch {
      /* ignore */
    }
  }

  const statusLabel: Record<string, string> = {
    offline: tr("statusOffline"),
    listening: tr("statusListening"),
    connecting: tr("statusConnecting"),
    connected: tr("statusConnected"),
    error: tr("statusError"),
  };

  return (
    <div className="app">
      {incoming && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            zIndex: 100,
            background: "rgba(0,0,0,0.45)",
            display: "grid",
            placeItems: "center",
          }}
        >
          <div
            className="card"
            style={{ width: 360, maxWidth: "90vw", margin: 16 }}
          >
            <h3 style={{ margin: "0 0 8px", fontSize: 16 }}>
              {tr("incomingTitle")}
            </h3>
            <p className="card-sub" style={{ marginBottom: 16 }}>
              {incoming.alias || incoming.peer}
              {incoming.viewOnly ? ` · ${tr("viewOnly")}` : ""}
            </p>
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => {
                  void answerConnection(incoming.peer, true, false);
                  setIncoming(null);
                }}
              >
                {tr("allow")}
              </button>
              <button
                type="button"
                className="btn btn-ghost"
                onClick={() => {
                  void answerConnection(incoming.peer, true, true);
                  setIncoming(null);
                }}
              >
                {tr("allowAlways")}
              </button>
              <button
                type="button"
                className="btn btn-danger"
                onClick={() => {
                  void answerConnection(incoming.peer, false, false);
                  setIncoming(null);
                }}
              >
                {tr("deny")}
              </button>
            </div>
          </div>
        </div>
      )}
      <aside className="sidebar" aria-label="nav">
        <div className="logo" title="MimoDesk">
          MD
        </div>
        <button
          type="button"
          className={`nav-btn ${page === "desk" ? "active" : ""}`}
          onClick={() => setPage("desk")}
          title="Desk"
        >
          <IconDashboard />
        </button>
        <button
          type="button"
          className={`nav-btn ${page === "sessions" ? "active" : ""}`}
          onClick={() => setPage("sessions")}
          title={tr("sessions")}
        >
          <IconSessions />
        </button>
        <div className="nav-spacer" />
        <button
          type="button"
          className={`nav-btn ${page === "settings" ? "active" : ""}`}
          onClick={() => setPage("settings")}
          title={tr("settings")}
        >
          <IconSettings />
        </button>
      </aside>

      <main className="main">
        <header className="topbar">
          <div className="brand-block">
            <h1>{tr("brand")}</h1>
            <p>{tr("tagline")}</p>
          </div>
          <div style={{ display: "flex", gap: 10, alignItems: "center" }}>
            <div className={`status-pill ${desk.status}`}>
              <span className="dot" />
              {statusLabel[desk.status] ?? desk.status}
              {desk.link?.rttMs != null && (
                <>
                  <span style={{ opacity: 0.55 }}>·</span>
                  <span style={{ opacity: 0.9 }}>{desk.link.rttMs} ms</span>
                </>
              )}
            </div>
          </div>
        </header>

        <div className="content">
          {page === "desk" && (
            <div className="desk-grid">
              <section className="card">
                <div className="card-head">
                  <h2 className="card-title">{tr("thisId")}</h2>
                </div>
                <p className="card-sub">{tr("thisIdHelp")}</p>
                <div className="address-row" aria-label="ID">
                  {(idPairs.length ? idPairs : ["00", "00", "00", "00", "00"]).map(
                    (pair, i) => (
                      <span key={i} style={{ display: "inline-flex", gap: 6 }}>
                        <span className="address-tile">{pair}</span>
                        {i < 4 && <span className="address-dash">-</span>}
                      </span>
                    ),
                  )}
                </div>
                {showFull && desk.rawAddress && (
                  <div className="address-full">{desk.rawAddress}</div>
                )}
                <div className="card-actions">
                  <button type="button" className="btn btn-ghost" onClick={onCopyId}>
                    <IconCopy /> {tr("copyId")}
                  </button>
                  {desk.rawAddress && (
                    <button
                      type="button"
                      className="btn btn-ghost"
                      onClick={onCopyFull}
                    >
                      {tr("copyFull")}
                    </button>
                  )}
                  <button
                    type="button"
                    className="btn btn-ghost"
                    onClick={() => setShowFull((v) => !v)}
                  >
                    {showFull ? tr("showId") : tr("details")}
                  </button>
                  <button
                    type="button"
                    className="btn btn-ghost"
                    onClick={onRestartHost}
                    disabled={busy}
                  >
                    <IconPower /> {tr("newId")}
                  </button>
                </div>
                <div className="meta-row">
                  <span>
                    {tr("network")}: <strong>{desk.relay}</strong>
                  </span>
                  <span>
                    {tr("listening")}:{" "}
                    <strong>{desk.listening ? tr("on") : tr("off")}</strong>
                  </span>
                </div>
              </section>

              <section className="card">
                <div className="card-head">
                  <h2 className="card-title">{tr("remoteDesk")}</h2>
                </div>
                <p className="card-sub">{tr("remoteHelp")}</p>
                <div className="field">
                  <label htmlFor="remote-id">{tr("target")}</label>
                  <input
                    id="remote-id"
                    value={remoteId}
                    onChange={(e) => setRemoteId(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void onConnect();
                    }}
                    placeholder={tr("targetPh")}
                    autoComplete="off"
                    spellCheck={false}
                    disabled={busy}
                  />
                </div>
                <div className="field">
                  <label htmlFor="access-pass">{tr("accessPassword")}</label>
                  <input
                    id="access-pass"
                    type="password"
                    value={accessPass}
                    onChange={(e) => setAccessPass(e.target.value)}
                    placeholder={tr("accessPasswordPh")}
                    autoComplete="off"
                    disabled={busy}
                    style={{ fontFamily: "var(--font)", letterSpacing: 0 }}
                  />
                </div>
                <label
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                    fontSize: 13,
                    color: "var(--muted)",
                    margin: "4px 0 12px",
                    cursor: "pointer",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={viewOnly}
                    onChange={(e) => setViewOnly(e.target.checked)}
                    disabled={busy}
                  />
                  {tr("viewOnly")}
                </label>

                <div style={{ marginBottom: 12 }}>
                  <div
                    style={{
                      fontSize: 12,
                      color: "var(--muted)",
                      fontWeight: 600,
                      marginBottom: 8,
                    }}
                  >
                    {tr("quality")}
                  </div>
                  <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                    {(
                      [
                        ["low", tr("low")],
                        ["medium", tr("medium")],
                        ["high", tr("high")],
                      ] as const
                    ).map(([k, label]) => (
                      <button
                        key={k}
                        type="button"
                        className={
                          quality === k ? "btn btn-primary" : "btn btn-ghost"
                        }
                        onClick={() => void onQuality(k)}
                        disabled={busy}
                      >
                        {label}
                      </button>
                    ))}
                  </div>
                  {desk.link && (
                    <p className="hint" style={{ marginTop: 10 }}>
                      {tr("link")}: {desk.link.mode}
                      {desk.link.rttMs != null ? ` · ${desk.link.rttMs} ms` : ""}
                      {` · ${desk.link.suggestedQuality}`}
                    </p>
                  )}
                </div>

                <div className="card-actions">
                  <button
                    type="button"
                    className="btn btn-primary"
                    onClick={onConnect}
                    disabled={busy || !remoteId.trim()}
                  >
                    {busy ? tr("connecting") : tr("connect")}
                  </button>
                  <button
                    type="button"
                    className="btn btn-ghost"
                    onClick={onSendFile}
                    disabled={busy}
                  >
                    {busy ? "…" : tr("sendFile")}
                  </button>
                  {desk.status === "connected" && (
                    <button
                      type="button"
                      className="btn btn-danger"
                      onClick={onDisconnect}
                      disabled={busy}
                    >
                      {tr("disconnect")}
                    </button>
                  )}
                </div>
                {toast && <div className="alert ok">{toast}</div>}
                {desk.lastError && (
                  <div className="alert error" style={{ marginTop: 10 }}>
                    {desk.lastError}
                  </div>
                )}
                <p className="hint" style={{ marginTop: 14 }}>
                  {tr("hint")}
                </p>
              </section>
            </div>
          )}

          {page === "sessions" && (
            <section className="card list-card">
              <div className="card-head">
                <h2 className="card-title">{tr("sessions")}</h2>
              </div>
              <p className="card-sub">{tr("sessionsHelp")}</p>
              <p className="hint">—</p>
            </section>
          )}

          {page === "settings" && (
            <section className="card settings-block">
              <div className="card-head">
                <h2 className="card-title">{tr("settings")}</h2>
              </div>
              <p className="card-sub">{tr("settingsHelp")}</p>

              <div className="field">
                <label htmlFor="host-pass">{tr("hostPassword")}</label>
                <input
                  id="host-pass"
                  type="password"
                  value={hostPass}
                  onChange={(e) => setHostPass(e.target.value)}
                  placeholder={tr("hostPasswordPh")}
                  style={{ fontFamily: "var(--font)", letterSpacing: 0 }}
                  onBlur={() => {
                    if (!isTauri) return;
                    void setAccessPassword(hostPass.trim() || null);
                  }}
                />
              </div>
              <label
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  fontSize: 13,
                  color: "var(--muted)",
                  margin: "0 0 16px",
                  cursor: "pointer",
                }}
              >
                <input
                  type="checkbox"
                  checked={requireApproval}
                  onChange={(e) => {
                    setReqApproval(e.target.checked);
                    if (isTauri) void setRequireApproval(e.target.checked);
                  }}
                />
                {tr("requireApproval")}
              </label>

              <div className="field">
                <label>{tr("language")}</label>
                <div style={{ display: "flex", gap: 8 }}>
                  <button
                    type="button"
                    className={lang === "en" ? "btn btn-primary" : "btn btn-ghost"}
                    onClick={() => setLang("en")}
                  >
                    English
                  </button>
                  <button
                    type="button"
                    className={lang === "tr" ? "btn btn-primary" : "btn btn-ghost"}
                    onClick={() => setLang("tr")}
                  >
                    Türkçe
                  </button>
                </div>
              </div>

              <div className="field">
                <label>{tr("theme")}</label>
                <div style={{ display: "flex", gap: 8 }}>
                  <button
                    type="button"
                    className={
                      theme === "dark" ? "btn btn-primary" : "btn btn-ghost"
                    }
                    onClick={() => setTheme("dark")}
                  >
                    {tr("dark")}
                  </button>
                  <button
                    type="button"
                    className={
                      theme === "light" ? "btn btn-primary" : "btn btn-ghost"
                    }
                    onClick={() => setTheme("light")}
                  >
                    {tr("light")}
                  </button>
                </div>
              </div>

              <div className="field">
                <label htmlFor="alias">{tr("deviceName")}</label>
                <input
                  id="alias"
                  value={aliasDraft}
                  onChange={(e) => setAliasDraft(e.target.value)}
                  style={{ fontFamily: "var(--font)", letterSpacing: 0 }}
                  onBlur={() => void onSaveAlias()}
                />
              </div>

              <dl className="kv">
                <dt>{tr("shortId")}</dt>
                <dd style={{ letterSpacing: 2, fontSize: 15 }}>{displayId}</dd>
                <dt>{tr("fullAddr")}</dt>
                <dd>{desk.rawAddress || "—"}</dd>
                <dt>{tr("rtt")}</dt>
                <dd>{desk.link?.rttMs != null ? `${desk.link.rttMs} ms` : "—"}</dd>
              </dl>
              <div className="footer-note">{tr("lanNote")}</div>
              <p className="hint" style={{ marginTop: 12 }}>
                {tr("credit")}
              </p>
            </section>
          )}

          <p
            className="hint"
            style={{ marginTop: 8, textAlign: "right", opacity: 0.7 }}
          >
            {tr("credit")}
          </p>
        </div>
      </main>
    </div>
  );
}

// keep detectLang referenced for default language bootstrap
void detectLang;
