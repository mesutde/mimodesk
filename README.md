<img width="985" height="717" alt="image" src="https://github.com/user-attachments/assets/61744413-d168-4f63-81eb-e2fd2f7749f9" />
<img width="978" height="717" alt="image" src="https://github.com/user-attachments/assets/895eb54b-d44a-4993-b310-815d522b77ad" />



# MimoDesk (PeerDesk)

**Serverless P2P remote desktop.** No VPS, no account, no cloud control plane — the PC you connect to *is* the host. Built with **Xiaomi MiMo Developers — MiMo-X Pro**.

> Kısa özet: AnyDesk/RustDesk gibi “ID ile bağlan”, ama merkezi sunucu yok. Iroh (QUIC + hole punch) ile doğrudan veya public relay üzerinden eşler arası.

---

## English — what & when

**MimoDesk** is a lightweight remote-control app: share a short ID, the other side connects, sees your screen, and drives mouse/keyboard. Video is JPEG over an encrypted Iroh session. Multi-monitor switch is live.

### When to use RustDesk / AnyDesk

| Situation | Prefer |
|-----------|--------|
| Corporate fleet, SSO, audit logs, address book | **AnyDesk / RustDesk (server)** |
| Need a public always-on rendezvous + web console | **AnyDesk / RustDesk** |
| Mobile clients (iOS/Android) required | **AnyDesk / RustDesk** |
| Billing, support desk, unattended mass deploy | **AnyDesk / RustDesk** |

### When to use MimoDesk

| Situation | Prefer |
|-----------|--------|
| You control two PCs and want **no third-party account** | **MimoDesk** |
| LAN or same-network quick help | **MimoDesk** |
| Don’t want to run/maintain `hbbs`/`hbbr` or buy a VPS | **MimoDesk** |
| Privacy-first: traffic stays P2P (relay only if hole punch fails) | **MimoDesk** |
| Simple Windows↔Windows (and Linux/macOS builds via CI) | **MimoDesk** |
| Experimenting / self-host-free P2P desktop | **MimoDesk** |

### Feature comparison

| | **MimoDesk** | **RustDesk** | **AnyDesk** |
|--|--------------|--------------|-------------|
| Central server required | No (Iroh public relay optional) | Optional (self-host or public) | Yes (vendor) |
| Account / login | No | Optional | Yes (teams) |
| Connect by | Short ID + optional password | ID + password | ID / alias |
| Protocol | Iroh QUIC (P2P) | Custom + optional relay | Proprietary |
| Multi-monitor switch | Yes (live, auto-revert) | Yes | Yes |
| File transfer | Yes (basic) | Yes | Yes |
| Clipboard | Both ways | Both ways | Both ways |
| Web browser client | No | Limited | Yes |
| Mobile apps | No | Yes | Yes |
| Open source | Yes (this repo) | Yes | No |
| Best for | Personal / LAN / no-account | Self-host & teams | Enterprise support |

---

## Türkçe — ne zaman hangisi?

**MimoDesk**, iki bilgisayar arasında hesapsız, sunucusuz uzak masaüstüdür. Kısa bir ID paylaşırsın; karşı taraf bağlanır, ekranı görür, fare/klavye kullanır.

### RustDesk / AnyDesk ne zaman daha iyi?

- Kurumsal filo, denetim kaydı, adres defteri, SSO gerekiyorsa  
- iOS/Android istemci şartsa  
- 7/24 herkesin bağlanabileceği genel sunucu + web panel istiyorsan  
- Destek masası / faturalı ticari kullanım ise  

### MimoDesk ne zaman daha iyi?

- Kendi iki bilgisayarın; **hiçbir üçüncü tarafa hesap** açmak istemiyorsan  
- Aynı ağda (LAN) hızlı yardım  
- RustDesk sunucusu (`hbbs`/`hbbr`) kurmak veya VPS kiralamak istemiyorsan  
- Mümkün olduğunca trafiğin P2P kalmasını istiyorsan  
- Basit, açık kaynak, denemelik/kişisel kullanım  

### Farklar (özet tablo)

| | **MimoDesk** | **RustDesk** | **AnyDesk** |
|--|--------------|--------------|-------------|
| Merkezi sunucu | Yok | İsteğe bağlı | Var (üretici) |
| Hesap | Gerekmez | İsteğe bağlı | Gerekir |
| Bağlantı | ID + şifre | ID + şifre | ID |
| Çoklu monitör | Var | Var | Var |
| Dosya / pano | Var | Var | Var |
| Web / mobil | Yok | Kısmen / var | Var |
| Açık kaynak | Evet | Evet | Hayır |

---

## How to use / Nasıl kullanılır

### English example

1. **Host PC** (the one you want to control): open MimoDesk → **Start host** → note the short ID (e.g. `482-910-3756`). Set a password if you want.
2. **Client PC**: open MimoDesk → enter the host ID → **Connect**.
3. Host accepts the incoming request (or enable always-allow).
4. Remote window opens: click the screen to send mouse/keyboard. Use **Monitor 1 / 2** to switch displays. **Fit** / **1:1** change scaling. Drop a file to send it.
5. **Cut** ends the session.

Same version on both sides. Both machines should stay awake; host must be running.

### Türkçe örnek

1. **Yönetilecek bilgisayar (host):** MimoDesk’i aç → **Sunucuyu başlat** → kısa ID’yi not et (ör. `482-910-3756`). İstersen şifre koy.
2. **Kontrol eden bilgisayar:** MimoDesk’i aç → host ID’sini yaz → **Bağlan**.
3. Gelen bağlantıyı host’ta **İzin ver** (veya “her zaman izin ver”).
4. Uzak masaüstü penceresi açılır: ekrana tıklayınca fare/klavye karşı tarafa gider. **Monitör 1 / 2** ile ekran değiştir. **Sığdır** / **1:1** ölçek. Dosyayı sürükleyip bırakarak gönder.
5. **Kes** ile oturumu bitir.

Her iki makinede de **aynı sürüm** olmalı; host uygulaması açık kalmalı.

---

## LAN only or the internet? / Sadece LAN mı, internet de olur mu?

**Both.** MimoDesk is not LAN-only. It uses **Iroh** (QUIC + hole punch). Direct P2P when possible; if both sides are behind strict NAT/firewall, traffic can fall back to Iroh’s **public relay**. You do **not** need your own VPS.

| Scenario | Works? |
|----------|--------|
| Same Wi‑Fi / LAN (home, office) | Yes — usually direct P2P |
| Different city / different ISPs (WAN) | **Yes** — hole punch, else public relay |
| Both behind strict NAT | Usually yes via relay |
| Fully offline LAN (no internet) | Yes (local discovery) |

```text
PC A (host) ──hole punch──► PC B (client)     // best path
     │                              ▲
     └──── Iroh public relay ───────┘          // fallback if punch fails
```

AnyDesk-style “open ID here, connect from anywhere” is supported. Host app must stay running on the controlled PC.

### Türkçe

**Sadece LAN değil — uzak bilgisayarlar da çalışır.** Iroh ile önce doğrudan P2P denenir; NAT/firewall sertse genel relay kullanılır. Kendi sunucu/VPS gerekmez.

| Durum | Çalışır mı? |
|-------|-------------|
| Aynı ev/ofis, aynı Wi‑Fi | Evet |
| Farklı şehir / farklı internet | **Evet** |
| Her ikisi de sıkı NAT arkasında | Genelde evet (relay) |
| Internetsiz yerel ağ | Evet |

---

## FAQ / SSS

**Q: Is a server or account required?**  
A: No account, no VPS. Iroh’s public relay is only a fallback for NAT traversal.

**Q: Sunucu veya hesap gerekli mi?**  
A: Hesap yok, VPS yok. Genel relay yalnızca NAT aşımında yedek yoldur.

**Q: Does it work between two PCs on the internet?**  
A: Yes — see the LAN/WAN table above.

**Q: İnternetten iki PC arasında çalışır mı?**  
A: Evet — yukarıdaki tabloya bak.

**Q: Why not a website / browser client?**  
A: Screen capture and input injection must run as native code on the host. A web client would need a different design (e.g. WebRTC agent).

**Q: Neden web sitesi yok?**  
A: Ekran yakalama ve girdi enjeksiyonu host’ta native kod ister; tarayıcı istemcisi ayrı mimari olurdu.

**Q: Who wrote this?**  
A: Built with **Xiaomi MiMo Developers — MiMo-X Pro**.

---

## Build from source

```powershell
npm install

# UI only (browser mock)
npm run dev

# Full Tauri app
npm run tauri:dev

# Release — must use tauri CLI (embeds UI; bare cargo build is not enough)
npm run tauri:build
```

Outputs:

- `src-tauri/target/release/mimodesk.exe`
- `src-tauri/target/release/bundle/nsis/`
- `src-tauri/target/release/bundle/msi/`

### Multi-platform CI

Push a version tag; GitHub Actions builds **Windows, Linux, macOS**:

```powershell
git tag v0.2.1
git push origin v0.2.1
```

Workflow: `.github/workflows/release.yml`

### Web?

Not supported as a website. Screen capture + input injection need native code on the host. A browser client would be a different architecture (e.g. WebRTC agent + web viewer).

---

## Architecture

- **Tauri 2** desktop shell (React UI)
- **Iroh** QUIC — hole punch + public relay fallback
- Capture: `xcap` + JPEG · Input: `enigo`
- Multi-monitor: live switch, auto-revert on capture failure
- ALPN: `mimodesk/control/0`

## Layout

```
src/                 # React UI
src-tauri/src/       # Rust: p2p, capture, input, clipboard
```

## Credits

Written with **Xiaomi MiMo Developers — MiMo-X Pro** model.

## License

MIT
