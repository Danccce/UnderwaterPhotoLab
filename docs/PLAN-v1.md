# UnderwaterPhotoLab – Plan für v1

Eigenständiges Programm (Rust + Slint) zur Farbkorrektur von Unterwasser-JPGs. Die korrigierten
Bilder werden anschließend in PhotoCraft, LightCraft oder RapidRaw geöffnet und dort bei Bedarf
nachbearbeitet. Schärfen, Entrauschen und HSL bleiben bewusst diesen Programmen überlassen.

Stand vor Beginn: Commit `5847ad3` (Kern-Algorithmus, PhotoCraft-Plug-ins, `uwcc-analyze`,
`unterwasser-analyse.exe`).

## Getroffene Entscheidungen

| Thema | Entscheidung |
|---|---|
| Originale | werden nie verändert, verschoben oder überschrieben |
| Export-Ziel | Standard: Unterordner `uw-korrigiert\` mit gleichem Dateinamen; optional im selben Ordner mit Endung `_uw` |
| Erneuter Export | überschreibt das vorherige Ergebnis ohne Nachfrage (nur Dateien in `uw-korrigiert\` bzw. `*_uw.jpg`, nie ein Original) |
| Dateinamen | keine eckigen Klammern (PowerShell behandelt `[ ]` als Platzhalter) |
| Einlesen | Ordner nicht rekursiv; `uw-korrigiert\` und Dateien mit `_uw` am Namensende werden ignoriert |
| Einstellungen | pro Bild in `uw-korrigiert\.uwcc.json`, damit ein Ordner später wieder bearbeitbar ist |
| Metadaten | EXIF (inkl. Orientation-Tag) und ICC-Profil werden unverändert übernommen, Pixel nicht gedreht |
| Ansicht | Normal: 100 % bearbeitet. Leertaste oder „Vorher“-Knopf halten: 100 % Original. Split-Ansicht nur als zuschaltbarer Modus |
| Mehrere Bilder | zwei Modi: „gleiche Werte übernehmen“ und „Auto pro Bild + meine Abweichung“ |
| KI | kein lernendes Modell; „Lernen“ = gespeicherte Abweichung vom Auto-Wert (Presets in v2) |
| Weiterbearbeiten | Knopf „In [Programm] öffnen“ statt „In PhotoCraft anwenden“: exportiert bei Bedarf und startet das eingestellte Programm mit der Ergebnisdatei |
| Bestehendes | Kern, Plug-ins und `uwcc-analyze` bleiben; `unterwasser-analyse.exe` wird nach v1 ersetzt |

## Architektur

```text
crates/
  uwcc-core      (no_std)  Analyse, Farbmatrix, neu: Einstellungsmodell und Pixel-Pipeline
  uwcc-photo     (neu)     Laden mit EXIF/ICC/Ausrichtung, EXIF-Vorschaubild, Vorschau, Export, Ordner-Scan
  uwcc-lab       (neu)     die Slint-App „UnderwaterPhotoLab“
  uwcc-plugin, uwcc-plugin-red, uwcc-analyze, uwcc-gui, uwcc-plugin-tests   unverändert
```

### Einstellungsmodell

```text
Settings {
    auto: bool                 // Basis aus der Analyse des jeweiligen Bildes
    delta: Abweichung vom Auto-Wert (hueShift, Levels je Kanal)
    strength                   // 0–100 %, Überblendung mit dem Original
    temperature, tint          // Weißabgleich, Pipette setzt beides
    vibrance
    red_limit                  // Obergrenze der Rotverstärkung gegen verstärktes Rauschen
}
```

Wirksame Werte = Auto-Analyse des Bildes + `delta`. Ein Regler zeigt immer den wirksamen Wert;
Bewegen ändert `delta`. Dadurch funktioniert „Auto + Abweichung“ für beliebig viele Bilder.

### Reihenfolge der Pixel-Pipeline

1. Farbmatrix aus Hue-Shift und Levels (GitHub-Algorithmus), mit `red_limit`
2. Weißabgleich (Kanalverstärkung aus Temperatur/Tint)
3. Dynamik (Vibrance)
4. Überblendung mit dem Original (`strength`)

Dieselbe Funktion rechnet Vorschau (max. 1280 px) und Export (volle Auflösung).

## Schritte

Jede Phase endet mit grünen Tests und einem eigenen Commit.

### Phase 0 – Kern erweitern (`uwcc-core`)

- [x] 0.1 `Settings` und `Delta` wie oben anlegen, inkl. Standardwerten (= reine Auto-Korrektur, 100 %)
- [x] 0.2 `Settings::resolve(auto: Correction) -> Correction` (Auto + Abweichung, Grenzen einhalten)
- [x] 0.3 Rotverstärkung begrenzen (`red_limit`: höchstens Faktor 4 Spreizung im Rotkanal, einstellbar)
- [x] 0.4 Weißabgleich: Temperatur/Tint → Kanalfaktoren (Grau bleibt gleich hell); Pipette `Settings::white_balance_from_sample`
- [x] 0.5 Vibrance (schwach gesättigte Farben stärker anheben als gesättigte)
- [x] 0.6 `process_rgba8(pixels, &Settings, &Correction)` als einzige Pipeline-Funktion
- [x] 0.7 Tests: Standardwerte = bisheriges Ergebnis; Stärke 0 = Original; Pipette auf Grau ergibt Grau; Alpha bleibt

Abnahme: bisherige Tests weiterhin grün, Abgleich mit dem Original-`index.js` unverändert exakt.
Erledigt: 10 Kern-Tests und 7 Plug-in-Tests grün, Abgleich auf den drei Beispielbildern weiterhin exakt (Abweichung 0).

### Phase 1 – Bilddateien (`uwcc-photo`)

- [x] 1.1 `Photo::open`: Pixel, EXIF, ICC-Profil und Ausrichtung lesen
- [x] 1.2 `Photo::preview(max)`: verkleinert und richtig gedreht
- [x] 1.3 EXIF-Vorschaubild auslesen (JPEG im IFD1); Rückfall auf Dekodieren, wenn keins vorhanden ist
- [x] 1.4 `scan_folder`: unterstützte Bilder sortiert, ohne `uw-korrigiert\` und `*_uw.*`
- [x] 1.5 Export: JPG (Qualität 95) mit EXIF und ICC, erst in Temp-Datei schreiben, dann umbenennen; ein vorhandenes Ergebnis wird ersetzt, ein Original nie (Prüfung: Zielpfad ≠ Quellpfad)
- [x] 1.6 Ausgabepfad je nach Modus (`uw-korrigiert\name.jpg` oder `name_uw.jpg`)
- [x] 1.7 Tests: EXIF/Orientation bleiben erhalten; Scan ignoriert Ergebnisse; Export überschreibt kein Original

Erledigt: 6 Tests mit erzeugten Bildern grün. Mit `uw-fotos` (296 Bilder, Olympus E-M10 II mit 16 MP
und XZ-1 mit 10 MP; Upload der E-M1-Bilder lief noch):
alle EXIF-Vorschaubilder in 0,28 s (≈ 1 ms pro Bild), die 3 Hochformate werden richtig gedreht;
ein Bild öffnen ≈ 0,4 s, Export in voller Auflösung ≈ 1,8 s (für Phase 6: Stapel-Export parallel).

### Phase 2 – App-Gerüst (`uwcc-lab`)

- [ ] 2.1 Neues Crate mit Slint (`~1.18`), Fenster „UnderwaterPhotoLab“
- [ ] 2.2 Layout: große Vorschau (Mitte), Reglerleiste (rechts, ca. 300 px), Bildleiste (unten, ca. 130 px)
- [ ] 2.3 „Ordner öffnen“ (Dialog) und Ordner per Drag-and-drop; Startargument = Ordner oder Bild
- [ ] 2.4 Bildleiste: waagerecht scrollbar (`ScrollView`/`Flickable`), Platzhalter sofort, Vorschaubilder im Hintergrund nachladen (EXIF-Vorschaubild zuerst)
- [ ] 2.5 Begrenzte Hintergrund-Threads (z. B. 4), damit der Speicher bei 24-MP-Bildern im Rahmen bleibt

Abnahme: Ordner mit ca. 200 JPGs zeigt die Leiste nach < 2 s, das Fenster bleibt bedienbar.

### Phase 3 – Vorschau und Vergleich

- [ ] 3.1 Klick in der Leiste lädt das Bild, analysiert es im Hintergrund und zeigt die Vorschau
- [ ] 3.2 Vorschau neu rechnen bei jeder Regleränderung (auf max. 1280 px, flüssig beim Ziehen)
- [ ] 3.3 Leertaste und Knopf „Vorher“ (gedrückt halten) zeigen 100 % Original
- [ ] 3.4 Split-Modus (Schalter): Trennlinie per Maus ziehen, links Original, rechts bearbeitet
- [ ] 3.5 Pfeiltasten links/rechts wechseln das Bild

### Phase 4 – Regler

- [ ] 4.1 Gruppe „Auto“: Auto ein/aus, „Zurücksetzen“, Stärke
- [ ] 4.2 Gruppe „Farbe“: Rot wiederherstellen (Hue-Shift), Temperatur, Grün/Magenta, Dynamik, Rot-Obergrenze
- [ ] 4.3 Pipette: Klick auf neutrale Stelle (Sand, Flosse, Graukarte) setzt Temperatur/Tint
- [ ] 4.4 Einklappbare Gruppe „Experte“: Levels je Kanal (Low/High)
- [ ] 4.5 Regler zeigen wirksame Werte; geänderte Bilder bekommen in der Leiste eine Markierung

### Phase 5 – Mehrfachauswahl

- [ ] 5.1 Strg+Klick und Umschalt+Klick in der Leiste, „Alle auswählen“
- [ ] 5.2 „Gleiche Werte übernehmen“: feste Werte des aktuellen Bildes auf die Auswahl
- [ ] 5.3 „Auto + Abweichung übernehmen“: nur `delta` und die übrigen Regler auf die Auswahl, jedes Bild behält seine eigene Analyse
- [ ] 5.4 Rückgängig für die letzte Übernahme

### Phase 6 – Export und Einstellungen speichern

- [ ] 6.1 Export-Optionen in der Reglerleiste: Ziel (Unterordner / selber Ordner), JPG-Qualität
- [ ] 6.2 „Aktuelles Bild exportieren“ und „Auswahl exportieren“ mit Fortschritt und Abbrechen
- [ ] 6.3 `.uwcc.json` schreiben (Einstellungen pro Datei) und beim Öffnen des Ordners wieder laden
- [ ] 6.4 Bereits exportierte Bilder in der Leiste kennzeichnen

### Phase 6b – „In [Programm] öffnen“

- [ ] 6.5 Konfiguration der Zielprogramme in `%APPDATA%\UnderwaterPhotoLab\config.json`: Name, Pfad zur `.exe`, Argumente mit Platzhalter (Standard `{files}`)
- [ ] 6.6 Vorbelegung durch Suche an den üblichen Installationsorten (PhotoCraft, Photoshop, Lightroom Classic, LightCraft, RapidRaw); eigene Einträge per Dateiauswahl
- [ ] 6.7 Immer verfügbar: „Windows-Standardprogramm“ und „Öffnen mit…“ (Windows-Auswahldialog)
- [ ] 6.8 Knopf „In {Programm} öffnen“ mit Auswahlpfeil zum schnellen Wechseln; exportiert vorher, falls die Datei fehlt oder die Einstellungen geändert wurden
- [ ] 6.9 Mehrfachauswahl: alle Ergebnisdateien in einem Aufruf übergeben (Photoshop/PhotoCraft öffnen dann alle)
- [ ] 6.10 Verhalten je Programm prüfen und im Konfigurationsdialog vermerken: Lightroom Classic importiert in den Katalog statt einfach zu öffnen; bei Katalog-Programmen ggf. den Ordner statt einzelner Dateien übergeben

### Phase 7 – Abschluss

- [ ] 7.1 Gesamttest mit einem echten Tauchgang-Ordner (Ladezeit, Reglerreaktion, Export, Metadaten in PhotoCraft/RapidRaw prüfen)
- [ ] 7.2 Screenshots, kurze Bedienhinweise im Fenster („?“)
- [ ] 7.3 `unterwasser-analyse.exe` (`uwcc-gui`) samt PhotoCraft-Steuerschnittstelle entfernen; die Plug-ins bleiben
- [ ] 7.4 Release-Build nach `dist\`

## v2 (nicht in v1)

- Dehaze in der Unterwasser-Variante (Dark Channel nur aus Grün und Blau, braucht das ganze Bild)
- Presets pro Wasserart („Rotes Meer“, „See grün“, „mit Blitz“) als gespeicherte Abweichung vom Auto-Wert
- Klarheit (lokaler Kontrast), Lichter/Schatten
- „Im Programm öffnen“: exportiertes Bild direkt an PhotoCraft/RapidRaw übergeben

## Offene Fragen

- Welche Programme sollen vorbelegt werden, und welches ist das Standard-Ziel?
