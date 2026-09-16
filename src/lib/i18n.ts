export type Lang = "en" | "tr";

export const dict = {
  en: {
    tagline: "No server · short ID · this PC is the host",
    thisId: "This computer's ID",
    thisIdHelp:
      "10-digit ID. Same LAN works immediately. Internet: paste full address once, then short ID is remembered.",
    remoteDesk: "Remote desktop",
    remoteHelp:
      "On the same network a 10-digit ID is enough. For internet use the full address. Choose image quality.",
    target: "Target ID or full address",
    targetPh: "61-79-62-45-67 or full endpoint…",
    quality: "Image quality",
    low: "Low",
    medium: "Medium",
    high: "High",
    connect: "Connect",
    connecting: "Connecting…",
    connected: "Connected",
    disconnect: "End session",
    openWindow: "Open remote window",
    copyId: "Copy ID",
    copyFull: "Copy full address",
    details: "Details",
    showId: "Show ID",
    newId: "New ID",
    network: "Network",
    listening: "Listening",
    on: "On",
    off: "Off",
    sessions: "Recent sessions",
    sessionsHelp: "Local history — never uploaded.",
    settings: "Settings",
    settingsHelp: "Identity is per-session. No account, no VPS.",
    deviceName: "Device name",
    shortId: "Short ID",
    fullAddr: "Full address",
    link: "Link",
    rtt: "Ping",
    qualityLbl: "Quality",
    language: "Language",
    theme: "Theme",
    dark: "Dark",
    light: "Light",
    sendFile: "Send file",
    fileSent: "File sent",
    fileReceived: "File received",
    remoteTitle: "Remote desktop",
    fullscreen: "Maximize",
    exitFs: "Restore window",
    cut: "End",
    waiting: "Waiting for screen…",
    waitingHelp: "Host may be sending a high-quality frame — wait a second.",
    hint:
      "Remote opens in a separate window. Fit mode always shows the whole remote screen including its taskbar. If text is blurry set quality to High.",
    lanNote:
      "Short ID is permanent on this PC. First time internet connect: paste the other side's full address once (or same LAN). After that only the short ID is needed — it is remembered in peers.json. There is no global ID server without a VPS.",
    statusOffline: "Offline",
    statusListening: "Listening",
    statusConnecting: "Connecting…",
    statusConnected: "Connected",
    statusError: "Error",
    qualityLowHint: "Quality: Low (fast, blurry text)",
    qualityMedHint: "Quality: Medium",
    qualityHighHint: "Quality: High (sharp, more bandwidth)",
    copiedId: "ID copied",
    copiedFull: "Full endpoint copied — send it to the remote PC",
    hostRestarted: "Host restarted — new ID issued",
    sessionClosed: "Session closed",
    versionMismatch: "peer version differs, update both sides",
    pingGood: "Good",
    pingOk: "OK",
    pingBad: "High",
    sendFileOnly: "Send file only",
    credit: "Built with MiMo-X-Pro-Preview",
    brand: "MimoDesk",
    close: "Close",
    fileTransfer: "File transfer",
    thisPcSend: "This PC (send from)",
    remotePcDest: "Remote PC (receive / dest)",
    emptyOrCannot: "Empty or cannot list",
    loading: "Loading…",
    sendSelected: "Send selected → remote folder",
    receiveSelected: "Receive selected ← remote",
    fileHint:
      "Click a folder to open it. Active folder is the path box.",
    dropToSend: "Drop to send",
    dropDestPrompt:
      "Remote destination folder (leave empty for Downloads\\MimoDesk):",
    destDefault: "→ Downloads\\MimoDesk",
    remoteTitleShort: "Remote desktop",
    keyboardHint:
      "Clipboard syncs both ways. Click the screen — keyboard & mouse are sent to the remote PC.",
    sentTo: "Sent",
    receivedTo: "Received",
    accessPassword: "Access password (if host set one)",
    accessPasswordPh: "Optional",
    viewOnly: "View only (no mouse/keyboard)",
    hostPassword: "Require password to connect to this PC",
    hostPasswordPh: "Empty = no password",
    requireApproval: "Ask me when someone connects",
    incomingTitle: "Incoming connection",
    allow: "Allow",
    allowAlways: "Always allow",
    deny: "Deny",
    fitScreen: "Fit screen",
    trueFs: "Full screen",
    exitTrueFs: "Exit full screen",
    monitor: "Monitor",
    monitorSwitching: "Switching monitor…",
    monitorSwitchFail: "Monitor switch not confirmed — reselected the live one.",
  },
  tr: {
    tagline: "Sunucu yok · kısa ID · bu bilgisayar host",
    thisId: "Bu bilgisayarın ID’si",
    thisIdHelp:
      "10 haneli ID. Aynı LAN’da hemen çalışır. İnternet: tam adresi bir kez yapıştırın; sonra kısa ID hatırlanır.",
    remoteDesk: "Uzak masaüstü",
    remoteHelp:
      "Aynı ağda 10 haneli ID yeterli. İnternet için tam adres. Görüntü kalitesini seçin.",
    target: "Hedef ID veya tam adres",
    targetPh: "61-79-62-45-67 veya tam endpoint…",
    quality: "Görüntü kalitesi",
    low: "Düşük",
    medium: "Orta",
    high: "Yüksek",
    connect: "Bağlan",
    connecting: "Bağlanıyor…",
    connected: "Bağlı",
    disconnect: "Oturumu kes",
    openWindow: "Uzak pencereyi aç",
    copyId: "ID’yi kopyala",
    copyFull: "Tam adresi kopyala",
    details: "Teknik detay",
    showId: "ID’yi göster",
    newId: "Yeni ID",
    network: "Ağ",
    listening: "Dinleme",
    on: "Açık",
    off: "Kapalı",
    sessions: "Son oturumlar",
    sessionsHelp: "Yerel geçmiş — sunucuya yüklenmez.",
    settings: "Ayarlar",
    settingsHelp: "Kimlik oturumluk. Hesap veya VPS yok.",
    deviceName: "Cihaz adı",
    shortId: "Kısa ID",
    fullAddr: "Tam adres",
    link: "Bağlantı",
    rtt: "Ping",
    qualityLbl: "Kalite",
    language: "Dil",
    theme: "Tema",
    dark: "Koyu",
    light: "Açık",
    sendFile: "Dosya gönder",
    fileSent: "Dosya gönderildi",
    fileReceived: "Dosya alındı",
    remoteTitle: "Uzak masaüstü",
    fullscreen: "Büyüt",
    exitFs: "Pencereye dön",
    cut: "Kes",
    waiting: "Ekran bekleniyor…",
    waitingHelp: "Host yüksek kalite kare gönderiyor olabilir — bir saniye bekleyin.",
    hint:
      "Bağlanınca uzak ekran ayrı pencerede açılır. Sığdır modu uzak görev çubuğu dahil ekranın tamamını gösterir. Yazı bulanıksa kaliteyi Yüksek yapın.",
    lanNote:
      "Kısa ID bu PC’de kalıcıdır. İlk internet bağlantısında karşı tarafın tam adresini bir kez yapıştırın (veya aynı LAN). Sonrada sadece kısa ID yeter — peers.json’a yazılır. VPS’siz genel ID sunucusu yoktur.",
    statusOffline: "Çevrimdışı",
    statusListening: "Dinliyor",
    statusConnecting: "Bağlanıyor…",
    statusConnected: "Bağlı",
    statusError: "Hata",
    qualityLowHint: "Kalite: Düşük (hızlı, yazılar bulanık)",
    qualityMedHint: "Kalite: Orta",
    qualityHighHint: "Kalite: Yüksek (net, daha çok bant)",
    copiedId: "ID kopyalandı",
    copiedFull: "Tam endpoint kopyalandı — karşı tarafa verin",
    hostRestarted: "Host yeniden başladı — yeni ID",
    sessionClosed: "Oturum kapatıldı",
    versionMismatch: "karşı sürüm farklı, iki tarafı da güncelleyin",
    pingGood: "İyi",
    pingOk: "Orta",
    pingBad: "Yüksek",
    sendFileOnly: "Sadece dosya gönder",
    credit: "MiMo-X-Pro-Preview ile yazıldı",
    brand: "MimoDesk",
    close: "Kapat",
    fileTransfer: "Dosya aktarımı",
    thisPcSend: "Bu bilgisayar (gönder)",
    remotePcDest: "Uzak bilgisayar (al / hedef)",
    emptyOrCannot: "Boş veya listelenemedi",
    loading: "Yükleniyor…",
    sendSelected: "Seçileni gönder → uzak klasör",
    receiveSelected: "Seçileni al ← uzak",
    fileHint:
      "Klasöre tıklayın, açılsın. Aktif klasör üstteki yol kutusudur.",
    dropToSend: "Göndermek için bırakın",
    dropDestPrompt:
      "Uzak hedef klasör (boş bırakın: Downloads\\MimoDesk):",
    destDefault: "→ Downloads\\MimoDesk",
    remoteTitleShort: "Uzak masaüstü",
    keyboardHint:
      "Pano iki yönlü eşitlenir. Ekrana tıklayın — klavye ve fare uzak PC’ye gider.",
    sentTo: "Gönderildi",
    receivedTo: "Alındı",
    accessPassword: "Erişim şifresi (host ayarladıysa)",
    accessPasswordPh: "İsteğe bağlı",
    viewOnly: "Sadece izle (fare/klavye yok)",
    hostPassword: "Bu bilgisayara bağlanmak için şifre iste",
    hostPasswordPh: "Boş = şifre yok",
    requireApproval: "Biri bağlandığında bana sor",
    incomingTitle: "Gelen bağlantı",
    allow: "İzin ver",
    allowAlways: "Her zaman izin ver",
    deny: "Reddet",
    fitScreen: "Ekrana sığdır",
    trueFs: "Tam ekran",
    exitTrueFs: "Tam ekrandan çık",
    monitor: "Monitör",
    monitorSwitching: "Monitör geçiriliyor…",
    monitorSwitchFail: "Monitör geçişi onaylanmadı — canlı olana dönüldü.",
  },
} as const;

export type TKey = keyof (typeof dict)["en"];

export function detectLang(): Lang {
  const nav = (navigator.language || "en").toLowerCase();
  return nav.startsWith("tr") ? "tr" : "en";
}

export function loadLang(): Lang {
  const saved = localStorage.getItem("mimodesk-lang");
  if (saved === "tr" || saved === "en") return saved;
  return detectLang();
}

export function saveLang(lang: Lang) {
  localStorage.setItem("mimodesk-lang", lang);
}

export function loadTheme(): "dark" | "light" {
  const saved = localStorage.getItem("mimodesk-theme");
  return saved === "light" ? "light" : "dark";
}

export function saveTheme(t: "dark" | "light") {
  localStorage.setItem("mimodesk-theme", t);
}

export function t(lang: Lang, key: TKey): string {
  return dict[lang][key];
}

/** 6179624567 → 61-79-62-45-67 */
export function formatId(digits: string): string {
  return (digits || "")
    .replace(/\D/g, "")
    .slice(0, 10)
    .replace(/(\d{2})(?=\d)/g, "$1-");
}
