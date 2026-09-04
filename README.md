# RecoverKit — USB-C Mobile Forensics Toolkit

A desktop forensic toolkit for Android phones. Plug in your phone over USB-C, acquire evidence
read-only across every app, and recover data — including **deleted WhatsApp messages** carved
directly from SQLite freeblocks, freelist pages and WAL frames.

**Supported sources (no root needed):**
- 🖼 **Photos & videos from any app** — WhatsApp media, the whole gallery (DCIM/Pictures/Movies), downloads
- 💬 **WhatsApp chats** — via the app's built-in Export chat (.txt)
- 📨 **SMS / MMS** — via "SMS Backup & Restore" (.xml)
- 📇 **Contacts** — full address book via .vcf export
- 🔒 **Encrypted WhatsApp backups** — crypt12/14/15 with a device key (root required for the key)

Deep carving of deleted records works on any recovered SQLite database.

Built as a **Rust engine + Tauri 2 desktop UI** (React/TypeScript, light saffron theme).

```
┌─────────────┐  USB-C / ADB   ┌──────────────────┐
│ Android     │ ──read-only──▶ │ recovery-core    │  Rust engine:
│ phone       │   (pull only)  │  (core/)         │  scan · decrypt · parse · carve
└─────────────┘                └────────┬─────────┘
                                        │ Tauri commands (JSON)
                               ┌────────▼─────────┐
                               │ RecoverKit UI    │  React + Tauri 2 desktop app
                               │  (desktop/)      │  Connect · Scan · Recover · Export
                               └──────────────────┘
```

## Features

- **Device detection** via ADB (serial, model, Android version, state)
- **Read-only acquisition** — strict ADB allowlist (`devices` / `pull` / `exec-out` only); `push`, `shell`, `install` etc. are rejected before they can run. The tool never writes to your phone.
- **Artifact scan** on device (`/sdcard/Android/media/com.whatsapp/...` + legacy paths) or any local backup folder
- **crypt12/14/15 decryption** (AES-256-GCM) using the WhatsApp key file
- **Live parsing** of modern (`message`/`chat`/`jid`) and legacy (`messages`) msgstore schemas
- **Deleted-message carving** from SQLite page freeblocks, the freelist trunk chain and WAL frames, with damaged-record reconstruction
- **Export** to JSON, CSV and a styled HTML report (deleted rows highlighted)

## Project layout

```
recovery-tool/
├── core/                    # recovery-core: Rust engine (library + CLI)
│   ├── src/adb.rs           #   read-only ADB client (command allowlist)
│   ├── src/scanner.rs       #   artifact discovery (device + folder)
│   ├── src/crypto.rs        #   crypt12/14/15 AES-256-GCM decryption
│   ├── src/sqlite/          #   msgstore parser + deleted-record carving
│   ├── src/main.rs          #   CLI (recovery-core)
│   └── tests/               #   integration tests (14 total with unit tests)
└── desktop/                 # RecoverKit desktop app (Tauri 2 + React-TS)
    ├── src/                 #   UI (App.tsx = 4-step wizard)
    └── src-tauri/           #   Rust shell, Tauri commands over recovery-core
```

## Prerequisites

| Tool | Version used | Why |
|---|---|---|
| Rust (cargo) | 1.96+ | builds the engine + app shell |
| Node.js + npm | 20.x | builds the UI |
| ADB (Android platform-tools) | any recent | phone communication — must be on `PATH` (or set the `ADB_PATH` env var) |
| WebView2 Runtime | preinstalled on Win 10/11 | renders the desktop UI |

On the phone: **Settings → Developer options → USB debugging** enabled, then allow the PC when prompted.

## Quick start — run the app

```powershell
# from the repo root
npm run ui:build          # builds the release app (exe + NSIS installer)
```

Outputs:

- Portable exe: `desktop\src-tauri\target\release\recoverkit-desktop.exe`
- Installer: `desktop\src-tauri\target\release\bundle\nsis\RecoverKit_0.1.0_x64-setup.exe`

Just run the exe (or the installer). For live development with hot reload:

```powershell
npm run ui:dev            # starts Vite + Tauri dev window
```

## Using the app (4 steps)

### ① Connect
Plug the phone in via USB-C, allow the USB-debugging prompt, click **⟳ Refresh devices**.
The connected phone appears with model/Android version and a green `device` state pill.

### ② Scan
- **🔍 Scan device** — lists WhatsApp databases/backups/media on the phone (nothing is copied yet)
- **⬇ Scan & pull** — also copies the found artifacts to the app's local workspace (media only with the *include media* tick)
- **📂 Import folder…** — scan an already-copied backup folder instead

### ③ Recover
Pick the `msgstore.db` (auto-filled after scan/pull), keep **deep recovery** ticked, press **▶ Run recovery**.
Deleted records appear with an orange **deleted** badge; use the search box or **deleted only** filter.

Encrypted backups (`.crypt12/14/15`) need WhatsApp's **key file**:
- **Rooted phone** → click **📥 Fetch key (root)** (reads `/data/data/com.whatsapp/files/key` via `su`)
- **Otherwise** → browse a previously extracted key with 🗝
Then click **Decrypt** — the decrypted database is recovered automatically.

> ⚠️ On non-rooted phones Android blocks access to WhatsApp's private storage
> (`/data/data/com.whatsapp/`), so the key file cannot be fetched over ADB.

### ④ Export
Save the result as **JSON**, **CSV**, or an **HTML report**, and open the folder with *Show in Explorer*.

## Working with real data

The tool processes **only real WhatsApp data** — it never shows, generates, or falls
back to sample/mock data. Point it at one of these:

| Source | How |
|---|---|
| Live phone (USB) | ① Connect → ② Scan & pull → ③ Recover |
| Encrypted backups (`.crypt12/14/15`) | needs the WhatsApp key file — root only (`📥 Fetch key`) or a previously extracted key (`🗝 Browse key…`) |
| A `msgstore.db` you already have | ③ Recover → 📄… to browse it |

The UI labels every result: 🟡 **DEMO** vs 🟢 **REAL DATA** — if you ever see the
yellow banner, the data came from a folder you imported, never from the tool itself.

## Command-line usage (no UI)

The engine also ships as a CLI (`recovery-core`):

```powershell
npm run core:cli -- devices                                  # list phones
npm run core:cli -- scan-device <serial>                     # scan phone (read-only)
npm run core:cli -- scan-device <serial> --pull --include-media
npm run core:cli -- scan-folder "C:\path\to\backup"         # scan local folder
npm run core:cli -- decrypt msgstore.db.crypt12 key out.db   # decrypt backup
npm run core:cli -- parse  out.db -o parsed.json             # live rows only
npm run core:cli -- recover out.db -o recovered.json         # live + deleted carving
```

## Tests

```powershell
npm run core:test         # 14 tests: unit + integration
```

Covers: modern/legacy schema parsing, deleted-record carving from freeblocks,
WAL-frame carving, crypt round-trip decryption, and ADB command-allowlist safety.

## npm scripts (repo root)

| Script | What it does |
|---|---|
| `npm run ui:dev` | Tauri dev mode (hot reload) |
| `npm run ui:build` | Release build → exe + installer |
| `npm run core:build` | Build the Rust engine (release) |
| `npm run core:test` | Run engine test suite |
| `npm run core:cli -- <cmd>` | Run the recovery CLI |

## Safety model

- **Read-only by default**: ADB is restricted to `devices`, `pull` and `exec-out`; anything else is rejected before the process is spawned.
- Phone data is only ever copied *off* the device; nothing is pushed, installed or deleted.
- Original pulled/decrypted files are kept; decryption writes a separate output file.

## Known limitations

- Key-file access requires **root** (Android app-private storage); no root = bring your own key file.
- crypt header offsets are discovered by candidate scanning validated against known layouts — unusual vendor variants may need the offset tables in `core/src/crypto.rs` extended.
- Media files are pulled/listed but not yet carved for thumbnails or parsed content.
- Google Drive / iCloud backups are out of scope (device + local backup acquisition only).
