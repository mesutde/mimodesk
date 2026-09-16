# MimoDesk — nasıl çalıştırılır

**Tarayıcıda `localhost` açmayın.** Uygulama masaüstü penceresidir.

## Kurulu değilse

```text
C:\Users\Mesut\Desktop\remote\peerdesk\src-tauri\target\release\bundle\nsis\MimoDesk_0.1.0_x64-setup.exe
```

## Kuruluysa / hızlı açılış

Çift tıklayın:

```text
C:\Users\Mesut\Desktop\MimoDesk.exe
```

veya:

```text
C:\Users\Mesut\Desktop\remote\peerdesk\src-tauri\target\release\mimodesk.exe
```

## Güvenlik duvarı (kısa ID / LAN için)

Yönetici PowerShell:

```powershell
New-NetFirewallRule -DisplayName "MimoDesk" -Direction Inbound -Protocol UDP -LocalPort 47777 -Action Allow
```

## Bağlantı onayı

Ayarlar → **Biri bağlandığında bana sor** işaretli olmalı (varsayılan açık).
Gelende ana pencereye “Gelen bağlantı” penceresi düşer.

## Hata: ERR_CONNECTION_REFUSED (localhost)

Bu, tarayıcıdan site açmak demek. MimoDesk’i **exe** ile başlatın; `npm run dev` zorunlu değil.
