import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export type SessionStatus =
  | "offline"
  | "listening"
  | "connecting"
  | "connected"
  | "error";

export type QualityLevel = "low" | "medium" | "high";

export interface LinkInfo {
  mode: string;
  rttMs: number | null;
  suggestedQuality: string;
}

export interface DeskInfo {
  address: string;
  rawAddress: string;
  alias: string;
  status: SessionStatus;
  relay: string;
  listening: boolean;
  lastError: string | null;
  quality: string;
  link: LinkInfo | null;
  session: number;
  remoteVersion: string | null;
}

export type InputEvent =
  | { t: "move"; x: number; y: number }
  | { t: "down"; btn: string }
  | { t: "up"; btn: string }
  | { t: "wheel"; dy: number }
  | {
      t: "key";
      key: string;
      down: boolean;
      ctrl?: boolean;
      shift?: boolean;
      alt?: boolean;
      meta?: boolean;
    }
  | { t: "text"; s: string };

export interface RemoteFrame {
  width: number;
  height: number;
  nativeWidth: number;
  nativeHeight: number;
  cursor?: number;
  monitor?: number;
  jpeg: string;
}

export interface MonitorInfo {
  index: number;
  id: string;
  name: string;
  width: number;
  height: number;
  primary: boolean;
}

export async function getRemoteMonitors(): Promise<MonitorInfo[]> {
  return invoke<MonitorInfo[]>("get_remote_monitors");
}

export async function setMonitor(index: number): Promise<void> {
  await invoke("set_monitor", { index });
}

export async function getDeskInfo(): Promise<DeskInfo> {
  return invoke<DeskInfo>("get_desk_info");
}

export async function startHost(): Promise<DeskInfo> {
  return invoke<DeskInfo>("start_host");
}

export async function regenerateId(): Promise<DeskInfo> {
  return invoke<DeskInfo>("regenerate_id");
}

export async function connectRemote(
  address: string,
  password?: string,
  viewOnly?: boolean,
): Promise<DeskInfo> {
  return invoke<DeskInfo>("connect_remote", {
    address,
    password: password ?? null,
    viewOnly: viewOnly ?? false,
  });
}

export async function disconnect(): Promise<void> {
  await invoke("disconnect");
}

export async function setAlias(alias: string): Promise<void> {
  await invoke("set_alias", { alias });
}

export async function sendInput(payload: InputEvent): Promise<void> {
  await invoke("send_input", { payload });
}

export async function setQuality(level: QualityLevel): Promise<void> {
  await invoke("set_quality", { level });
}

export function listenRemoteFrame(cb: (f: RemoteFrame) => void): Promise<UnlistenFn> {
  return listen<RemoteFrame>("remote-frame", (e) => cb(e.payload));
}

export async function openRemoteWindow() {
  const existing = await WebviewWindow.getByLabel("remote-session");
  if (existing) {
    await existing.setFocus();
    return;
  }
  const win = new WebviewWindow("remote-session", {
    url: "index.html?view=remote",
    title: "Remote desktop — MimoDesk",
    width: 1280,
    height: 800,
    minWidth: 640,
    minHeight: 400,
    center: true,
    resizable: true,
  });
  win.once("tauri://error", (e) => {
    console.error("remote window error", e);
  });
}

export async function sendFile(path: string, destDir?: string): Promise<string> {
  return invoke<string>("send_file", { path, destDir: destDir ?? null });
}

export function listenPing(cb: (rtt: number) => void): Promise<UnlistenFn> {
  return listen<{ rtt: number }>("ping", (e) => cb(e.payload.rtt));
}

export function listenFileReceived(
  cb: (p: { name: string; path: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ name: string; path: string }>("file-received", (e) =>
    cb(e.payload),
  );
}

export function listenFileProgress(
  cb: (p: { sent?: number; received?: number; total: number; name: string }) => void,
): Promise<UnlistenFn> {
  return Promise.all([
    listen<{ sent: number; total: number; name: string }>("file-progress", (e) =>
      cb(e.payload),
    ),
    listen<{ received: number; total: number; name: string }>(
      "file-received-progress",
      (e) => cb(e.payload),
    ),
  ]).then(([a, b]) => {
    return () => {
      a();
      b();
    };
  });
}

export async function closeRemoteWindow() {
  try {
    const w = await WebviewWindow.getByLabel("remote-session");
    if (w) {
      try {
        await w.destroy();
      } catch {
        await w.close();
      }
    }
  } catch {
    /* ignore */
  }
}

export function listenDisconnected(cb: () => void): Promise<UnlistenFn> {
  return listen<{ disconnected?: boolean }>("remote-status", (e) => {
    if (e.payload?.disconnected) cb();
  });
}

export interface DirListing {
  path: string;
  entries: { name: string; isDir: boolean; size: number }[];
}

export async function listLocalDir(path: string): Promise<DirListing> {
  return invoke<DirListing>("list_local_dir", { path });
}

export async function listRemoteDir(path: string): Promise<DirListing> {
  return invoke<DirListing>("list_remote_dir", { path });
}

export async function pullFile(
  remotePath: string,
  localDest: string,
): Promise<string> {
  return invoke<string>("pull_file", { remotePath, localDest });
}

export async function openFileTransferWindow() {
  const existing = await WebviewWindow.getByLabel("file-transfer");
  if (existing) {
    await existing.setFocus();
    return;
  }
  new WebviewWindow("file-transfer", {
    url: "index.html?view=files",
    title: "File transfer — MimoDesk",
    width: 920,
    height: 600,
    minWidth: 720,
    minHeight: 480,
    center: true,
    resizable: true,
  });
}

export async function focusMainWindow() {
  try {
    const w = await WebviewWindow.getByLabel("main");
    if (w) await w.setFocus();
  } catch {
    /* ignore */
  }
}

export async function answerConnection(
  peer: string,
  allow: boolean,
  always = false,
): Promise<void> {
  await invoke("answer_connection", { peer, allow, always });
}

export async function setAccessPassword(password: string | null): Promise<void> {
  await invoke("set_access_password", { password });
}

export async function setRequireApproval(require: boolean): Promise<void> {
  await invoke("set_require_approval", { require });
}

export interface AccessSettings {
  password: string | null;
  requireApproval: boolean;
  alwaysAllow: string[];
}

export async function getAccessSettings(): Promise<AccessSettings> {
  return invoke<AccessSettings>("get_access_settings");
}

export function listenIncomingConnection(
  cb: (p: { peer: string; alias: string; viewOnly: boolean }) => void,
): Promise<UnlistenFn> {
  return listen<{ peer: string; alias: string; viewOnly: boolean }>(
    "incoming-connection",
    (e) => cb(e.payload),
  );
}

export const mockDesk: DeskInfo = {
  address: "4829103756",
  rawAddress: "",
  alias: "WORKSTATION",
  status: "listening",
  relay: "Iroh · VPS yok",
  listening: true,
  lastError: null,
  quality: "High",
  link: null,
  session: 0,
  remoteVersion: null,
};

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/// Frontend/app version. Kept in sync with package.json + backend
/// CARGO_PKG_VERSION; compared against the peer's reported version so
/// mismatched exes warn instead of misbehaving silently.
export const APP_VERSION = "0.2.0";
