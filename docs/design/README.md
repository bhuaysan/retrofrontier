# Handoff: RetroFrontier — RetroArch-Bibliotheks-UI

## Project constraints

- RetroArch is downloaded and managed as an isolated runtime; it is not bundled in the RetroFrontier installer.
- Linux x86_64 is the primary implementation and validation platform. Windows x86_64 and macOS arm64/x86_64 remain V1 targets, but must not block the initial Linux implementation.
- The runtime, filesystem, process, and SQLite boundaries in `ARCHITECTURE.md` and the ADRs are authoritative. These screens are a UI/design handoff, not a replacement for those contracts.

## Overview
Frontend-UI für einen RetroArch-basierten ROM-Manager: Onboarding/Scan, Bibliothek mit Suche/Filter/Sammlungen, Spiel-Detail mit Save-States, Einstellungen inkl. Controller-Mapping, Datenprobleme (Duplikate, unbekannte Dateien, Fehlerzustände) und ein TV-/10-Fuß-Modus. Look: 16-Bit-Retro (Press Start 2P / VT323 / Space Grotesk), harte Kanten, keine Rundungen, "Pixel-Shadow"-Boxen statt weicher Schatten.

## About the Design Files
Die Dateien in `screens/` sind **HTML-Designreferenzen** (interaktive Prototypen), kein Produktionscode. Aufgabe: diese Designs im Ziel-Stack **nachbauen** — hier geplant als **Tauri + React + Vite + SQLite**, RetroArch als externer Emulator-Core-Host, ScreenScraper als Metadaten-/Cover-Quelle. Falls der Stack sich noch ändert, gilt dieselbe Regel: Designs sind die Spezifikation, nicht der Code.

## Fidelity
**Hifi.** Farben, Typografie, Abstände und Interaktionszustände sind final. Die Fokus-/Hover-Sprache ist in `screens/A6 Fokus-Zustandsblatt.dc.html` explizit dokumentiert und entschieden (V5):
1. Listenzeile → 18px-Cursor-Spalte (▶) vor dem Bauteil, Bauteil bleibt unverändert.
2. Eigenständige Fläche (Button, Toggle, Chip, Tab) → Vordergrund/Hintergrund invertiert (`background:var(--text); color:var(--bg)`), kein Farbtoken.
3. Karte mit Bild (Cover, Screenshot, Save-State) → `transform:scale(1.08)` + größerer Pixel-Schatten, keine Farbänderung.
Kein Fokus-Ring, keine eigene Fokusfarbe — das war eine verworfene Zwischenstufe.

## Design Tokens
Siehe `tokens.css` (mitgeliefert, einziger Ort für Farben/Typo/Schatten-Konventionen). Beide Themes (`dark`/`light`) über `[data-theme]`. Wichtig: `--shadow` ≠ `--border` (Schatten in Dark bewusst heller als reines Schwarz, sonst gegen `--bg` unsichtbar).

## Screens / Views

### A · Onboarding & Sitzung
- **A1 Erststart** — Ordner-Auswahl (leer/gewählt-Zustand), "Später einrichten"-Skip.
- **A2 Scan** — Scan-Fortschritt, deterministisch (Prozentbalken) vs. unbestimmt (Lauflicht, 8-step chase), Live-Zähler pro erkanntem System.
- **A3 Scan-Ergebnis** — Zusammenfassung + "Hinweise"-Liste (Duplikate/Unbekannte Prüfsumme/Ohne Cover), verlinkt auf C7/C8/C4-Flows.
- **A4 Startvorgang** — Ladebildschirm vor Core-Start (Cover, Core-Name, indeterminate Bar).
- **A5 Rückkehr** — Post-Session-Screen (Sitzungsdauer, Auto-Save-Hinweis) + Detail-Ansicht darunter (siehe B6, identischer Aufbau).
- **A6 Fokus-Zustandsblatt** — Kein Produkt-Screen, sondern das verbindliche Zustands-/Fokus-Referenzdokument (siehe oben). Für die Umsetzung nicht bauen, nur als Spec lesen.

### B · Bibliothek
- **B1 Bibliothek** — Grid-Ansicht, Mehrfachauswahl (Checkbox pro Karte, 22×22px), Sidebar mit Systemfiltern + Einstellungen-Link.
- **B2 Suche** — Inline-Ergebnisse + Vollbild-Such-Overlay mit On-Screen-Tastatur (Controller-Bedienung).
- **B3 Filter-Leiste** — Genre-/Region-Dropdowns (Pfeil-Glyph `▾`), Favoriten-/Ungespielt-Toggle-Chips, Leer-Zustand bei 0 Treffern.
- **B4 Leere Bibliothek** — Erstzustand ohne ROMs, CTA "Ordner scannen".
- **B5 Suche ohne Treffer** — Leer-Zustand mit Query-Echo + "Suche löschen".
- **B6 Detail-Ansicht** — Spiel-Metadaten, Save-States-Grid, Screenshots-Grid.
- **B7 Save-State-Aktionen** — Kontextmenü pro State (Laden/Löschen), Löschen-Bestätigungsdialog.
- **B9 Einstellungen** — 4 Tabs (Allgemein/Cores/Video/Controller), Toggles + Custom-Selects.
- **B10 Controller-Mapping** — Tastenbelegungs-Tabelle, "Taste drücken…"-Listening-State.
- **B11 Ordner-Dialog** — Datei-Browser-Modal (Breadcrumb, Ordnerliste, leerer Ordner).

### C · Datenprobleme
- **C1 Fehlerzustand** — 3 Varianten via Prop (`errorType`): Core fehlt / BIOS fehlt / ROM nicht gefunden.
- **C4 Cover-Platzhalter** — Regel für fehlendes Cover: Systemfarbe als Fläche + Titel in Press Start 2P.
- **C7 Duplikate** — Gruppierte Varianten pro Titel, Radiobutton-artige Auswahl der zu behaltenden Datei.
- **C8 Unbekannte Dateien** — Titel-Eingabe für unbekannte Dateien, "Ignorieren"/"Entfernen"-Aktionen.

### D · Verwaltung
- **D1 Spiel-Overrides** — Pro-Spiel-Override von Core/Shader/Aspect/Integer-Scaling gegenüber globalen Einstellungen.
- **D2 Sammlungen** — Übersicht (Thumbnail-Collage) + Detail (Spiele-Grid).
- **D3 Sammelaktionen** — Mehrfachauswahl + Bulk-Metadaten-Dialog (Genre/Region/Favorit für N Titel).
- **D4 Metadaten bearbeiten** — Formular (Titel/Genre/Region/Jahr/Entwickler/Beschreibung), Cover ändern, Reset-Option.
- **D5 TV-Modus** — Zweite feste Größenstufe (×1.4) für 10-Fuß-Nutzung, manueller Schalter in Einstellungen.
- **D6 Statistiken** — Summary-Karten, Meistgespielt-Liste, Spielzeit-Balken pro System.

## Interactions & Behavior
- **Theme**: Dark/Light-Umschalter im Header, global persistiert (kein Screen ohne Umschalter außer Modals/Dialogen, die das aktuelle Theme erben).
- **Controller-Footer**: A/B/X/Y/Select/Start-Hinweise unten — im echten Build an die Gamepad-API bzw. RetroArch-Hotkeys koppeln, nicht nur dekorativ.
- **Fortschrittsbalken**: 3 Varianten — deterministisch (Scan/Start), Mini-Indikator beim Rebind (Controller-Mapping), Datenbalken (Statistiken). Bewusst unterschiedliche Höhen (14px/9px/16px) je nach Kontext.
- **Sidebar/Menü-Zeilen**: immer per Tab erreichbar (siehe Fokus-Sprache Punkt 1), auch im Leer-Zustand (B4).

## State Management (Vorschlag)
- `theme`: `'dark'|'light'`, persistiert (z. B. Tauri Store/localStorage).
- `library`: Spiele-Liste inkl. Systeme, Region, Genre, Favorit, gespielt.
- `scan`: Fortschritt, Phase (bestimmt/unbestimmt), gefundene Systeme.
- `collections`: Sammlungen + Zuordnung Spiel↔Sammlung.
- `saveStates`: pro Spiel, Slot (AUTO/1/2…), Zeitstempel, Thumbnail-Pfad.
- `settings`: global (Theme, Scanline, TV-Modus, UI-Sounds, Cores pro System, Video, Controller-Hotkey).
- `overrides`: pro Spiel (Core/Shader/Aspect/Integer-Scaling).
- `controllerBindings`: global + pro Core.
- `stats`: Sitzungen (Start/Ende/Spiel), aggregiert für D6.
- `issues`: Duplikate-Gruppen, unbekannte/beschädigte Dateien (aus Scan abgeleitet).

## Backend-Anforderungen (Tauri + SQLite + RetroArch + ScreenScraper)

**1. Dateisystem-Scan (A1–A3, B11)**
- Ordnerauswahl über Tauri `dialog`-Plugin; rekursiver Scan über `fs`-Plugin (oder Rust-Backend-Command für Performance bei großen Bibliotheken).
- Erkennung: Dateiendung → System-Mapping, Checksum (CRC32/MD5) für Duplikat-Erkennung (C7) und Abgleich gegen bekannte Datenbanken (No-Intro/Redump-Hashes) für "unbekannte Prüfsumme" (C8).
- Scan als Rust-Command mit Progress-Events (`emit` an Frontend) für den Fortschrittsbalken in A2.

**2. Metadaten & Cover (A3, C4, D4)**
- ScreenScraper-API-Client (Rust-Command, da API-Key/Secret nicht im Frontend liegen sollte): Abgleich per Checksum/Dateiname, Titel/Genre/Jahr/Entwickler/Region/Cover-URL.
- Cover lokal cachen (`appDataDir`), Platzhalter-Regel aus C4 greift nur wenn kein Cover geladen werden konnte.
- Rate-Limiting/Retry für ScreenScraper einplanen (Screens zeigen "Nachladen"-Aktion für Retry, siehe A3).

**3. SQLite-Schema (Vorschlag)**
```
games(id, title, system, genre, region, year, developer, description,
      cover_path, favorite, played, added_at)
content_roots(id, path, kind, created_at)
content_units(id, game_id, unit_type, title, created_at)
content_files(id, content_unit_id, content_root_id, relative_path,
              checksum, size, modified_at)
systems(id, name, core_default)
collections(id, name)
collection_games(collection_id, game_id)
save_states(id, game_id, slot, created_at, thumbnail_path)
core_overrides(game_id, key, value)          -- D1
controller_bindings(scope, action, binding)   -- scope: 'global' | core_id, B10
settings(key, value)                          -- B9
play_sessions(id, game_id, started_at, ended_at)  -- D6
scan_issues(id, type, content_file_id, game_id_a, game_id_b) -- Duplikate/unbekannt, C7/C8
```

**4. RetroArch-Integration (A4, D1, B7)**
- Start eines Spiels: Ein Rust-Backend/Rust-`RuntimeManager` löst die aktive verwaltete Runtime und den freigegebenen Core auf und startet unter Linux den authentifizierten AppDir-Einstiegspunkt `AppRun` mit absoluten Content-/Core-Pfaden und einer expliziten RetroFrontier-Konfiguration. Es wird weder ein System-`retroarch` aus `PATH` noch eine vorhandene Host-Konfiguration verwendet. Spiel-Overrides werden nur in kontrollierte Startparameter bzw. Konfigurationsdateien übersetzt.
- Save-States: RetroArch schreibt eigene `.state`-Dateien — Backend beobachtet RetroArchs Save-Verzeichnis (per Konvention `<rom>.state1` etc.) und synchronisiert Metadaten (Zeitstempel, Slot) in `save_states`; Thumbnails über RetroArchs Screenshot-Funktion oder eigenen Hook erzeugen.
- Save-State-Einträge bewahren die Runtime-/Core-Identität, damit inkompatible Zustände später verständlich gemeldet werden können. Runtime-Dateien, Saves, States, Screenshots und Logs bleiben getrennte Datenbereiche.
- Controller-Mapping (B10): entweder in RetroArchs `.cfg` schreiben (kompatibel mit RetroArch selbst) oder eigene Bindings-Tabelle, die beim Start als `--appendconfig` injiziert wird.

**5. Statistiken (D6)**
- `play_sessions` bei Prozessstart/-ende von RetroArch schreiben (Sidecar-Prozess-Exit abfangen).
- Aggregation (Gesamtzeit, Meistgespielt, Zeit pro System) im Frontend oder als SQL-View berechnen.

**6. Bekannte Lücken / zu klären mit dem Team**
- ScreenScraper-Zugangsdaten (API-Key pro Nutzer oder App-weit?).
- Wie werden BIOS-Dateien (C1 "BIOS fehlt") erkannt/verwaltet — die Pfade werden durch den Rust-Backend-/BiosService aus den RetroFrontier-eigenen BIOS-Wurzeln aufgelöst; ein Hostpfad wie `~/.config/retroarch/system/` darf nicht vorausgesetzt werden.
- TV-Modus (D5): CSS-Variablen-Skalierung oder echte zweite Theme-Stufe im Code?

## Assets
- Cover/Screenshots im Prototyp sind CSS-Gradients (Platzhalter) — durch ScreenScraper-Boxart ersetzen.
- Icons sind handgezeichnete Pixel-SVGs (inline `<path>`, `shape-rendering:crispEdges`) — als eigenständiges Icon-Set übernehmbar oder 1:1 weiterverwenden.
- **Akzeptierte Ausnahmen von der Pixel-Regel (Stand M6.7, Produktentscheidung):** Zwei Elemente werden bewusst als glatte Vektorform gezeichnet und sind *keine* offene Fidelity-Arbeit. Sie dürfen nicht auf Pixel-Snapping zurückgesetzt werden.
  - **Richtungs-/Sidebar-Cursor-Pfeile** (`PixelArrow`, Commit `64f5bbe`): gefülltes Dreieck mit durchgehenden Diagonalen statt 5×7-`crispEdges`-Treppe. Dieselbe Glyphe trägt sowohl die kleine Sidebar-Cursor-Größe als auch die größere Section-Heading-Größe.
  - **Favorite-Stern in der Detail-Ansicht** (`PixelStar`, Commit `17a2986`): Vektor-Silhouette mit gestricheltem Umriss für den ungefüllten Zustand, damit gefüllt/ungefüllt in der kompakten Steuergröße unterscheidbar bleibt.
  - Alle übrigen Icons (`FolderIcon`, `LibraryIcon`, `ExternalLinkIcon`, `WarningIcon`, `PixelCheck`) sowie sämtliche Box-Chrome folgen weiterhin der harten Pixel-Sprache.
- Schriften: Google Fonts "Press Start 2P", "VT323", "Space Grotesk" (Lizenz: Open Font License, unkritisch für Bundling).

## Dateien in diesem Paket
- `screens/` — alle 26 Design-Referenzen (HTML, im Browser direkt öffenbar).
- `tokens.css` — Single Source of Truth für Farben/Typografie/Schatten-Konventionen.
- Dieses `README.md`.
