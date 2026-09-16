# MimoDesk (PeerDesk)

Sunucusuz (VPS’siz) P2P uzak masaüstü. **Bağlanılan bilgisayar host sunucudur**; merkezi app sunucusu yoktur.

## İndir

Windows build’leri **[Releases](https://github.com/mesutde/mimodesk/releases)** sayfasında:

| Asset | Açıklama |
|-------|----------|
| `mimodesk.exe` | Taşınabilir — kurulumsuz çalışır |
| `MimoDesk_*_x64-setup.exe` | NSIS kurulum |
| `MimoDesk_*_x64_en-US.msi` | MSI kurulum |

Host ve client aynı sürümü kullanmalı.

## Mimari

- **Tauri 2** masaüstü kabuğu (React UI)
- **Iroh** QUIC + hole punch + public relay fallback
- Eşleşme: kısa ID (AnyDesk gibi)
- Ekran yakalama: xcap + JPEG
- Girdi: enigo (SendInput)
- Çoklu monitör: canlı geçiş, otomatik geri dönüş

## Geliştirme

```powershell
npm install

# sadece UI (browser mock)
npm run dev

# tam Tauri + Iroh
npm run tauri:dev

# release build (custom-protocol ile — bare cargo build YETMEZ)
npm run tauri:build
```

Release çıktısı:

- `src-tauri/target/release/mimodesk.exe`
- `src-tauri/target/release/bundle/nsis/`
- `src-tauri/target/release/bundle/msi/`

## Dizin yapısı

```
peerdesk/
  src/                 # React UI
  src-tauri/src/       # Rust: p2p, capture, input, clipboard
  dist/                # vite build (git’te yok)
```

## Notlar

- Çoklu monitör: uzak pencere araç çubuğundan **Monitör 1 / 2** seçin.
- Geçiş başarısızsa host otomatik son başarılı ekrana döner.
- `cargo build --release` tek başına UI’ı gömmez; `npm run tauri:build` kullanın.
