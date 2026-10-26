# UnderwaterPhotoLab – Plan für v2

**Ziel:** Für die meisten Bilder eines Tauchgangs entsteht die fertige Version direkt im
UnderwaterPhotoLab – ohne PhotoCraft, RapidRaw o. Ä. „In … öffnen“ bleibt für Sonderfälle
(Retusche, Montagen, RAW).

Stand vor Beginn: v1 abgeschlossen (`docs/PLAN-v1.md`, letzter Commit `c449cbe`).

## Leitlinien

1. **Unterwasser zuerst.** Vorrang haben Werkzeuge, die bei Unterwasser-JPGs fast immer gebraucht werden;
   allgemeine Foto-Werkzeuge nur so weit, wie sie für ein fertiges Bild nötig sind.
2. **Viele Bilder.** Alles muss sich übernehmen, als Preset speichern und im Stapel exportieren lassen.
3. **Vorschau = Export.** Filter mit Radius (Dehaze, Klarheit, Schärfen, Rauschen) rechnen mit Radien relativ
   zur Bildgröße, damit die 1280-px-Vorschau dem Export entspricht. Für Schärfe und Rauschen gibt es eine
   100-%-Ansicht.
4. **Nicht-destruktiv bleibt.** Alles steht als Einstellung in `.uwcc.json`; Originale werden nie verändert.

## Antworten auf die drei Fragen

### Welche Filter sind noch sinnvoll?

| Filter | Nutzen unter Wasser | Aufwand | Einordnung |
|---|---|---|---|
| **Belichtung, Kontrast** | immer nötig | gering | Muss |
| **Lichter / Schatten** | Gegenlicht zur Oberfläche, dunkler Vordergrund | mittel | Muss |
| **Dehaze (Unterwasser-Variante)** | Schleier durch Schwebeteilchen ist das zweitgrößte Problem nach der Farbe | mittel–hoch | Muss |
| **Klarheit** (lokaler Kontrast) | Korallen, Schuppen, Strukturen | mittel | Muss |
| **Schärfen** (mit Schwelle) | jedes fertige Bild braucht etwas, die Schwelle schont glattes Wasser | gering | Muss |
| **Farbrauschen reduzieren** | entsteht durch das starke Anheben von Rot, besonders in dunklen Bereichen | mittel | Muss |
| Luminanzrauschen reduzieren | bei hohen ISO (z. B. ISO 1600 in `uw-fotos`) | hoch, wenn gut | Soll (einfach) |
| Vignette (entfernen/hinzufügen) | Blitz-/Domeport-Abschattung, Bildwirkung | gering | Soll |
| Farbmischer, vereinfacht: Wasserblau, Cyan, Orange/Gelb | Wasserfarbe einstellen, Fische/Korallen betonen | mittel | Soll |
| Partikel (Backscatter) entfernen | häufiges Ärgernis bei Blitzfotos | hoch | Kann (Phase 13) |
| Lokale Masken, Pinsel, Ebenen | – | sehr hoch | **Nicht** (bleibt Sache von PhotoCraft) |

Die Dehaze-Variante: Der klassische „Dark Channel Prior“ schätzt den Dunst aus dem dunkelsten Kanal; unter
Wasser ist das immer Rot, das Verfahren überschätzt den Dunst. Die Unterwasser-Variante (UDCP) nimmt nur Grün und
Blau. Das „Hintergrundlicht“ (Wasserfarbe) wird aus dem ganzen Bild geschätzt – das kann nur das eigenständige
Programm, nicht das PhotoCraft-Plug-in.

### Zuschneiden?

**Ja, unbedingt.** Ohne Zuschneiden und Geraderichten ist kaum ein Bild „fertig“. Umfang:

- Zuschnittrahmen auf der Vorschau ziehen, Drittel-Raster während des Ziehens
- Seitenverhältnisse: frei, wie Original, 3:2, 4:3, 1:1, 16:9, 4:5 (Instagram), Hoch/Quer tauschen
- Geraderichten: Regler ±45° und „Linie ziehen“ (Horizont, Riffkante)
- Drehen 90° links/rechts, Spiegeln
- Beim Export werden die Pixel aufrecht geschrieben und das EXIF-Orientation-Tag auf 1 gesetzt
  (sonst würde doppelt gedreht); das eingebettete Vorschaubild zeigt den Zuschnitt.
- Zuschnitt wird beim Übernehmen auf mehrere Bilder standardmäßig **nicht** mit übertragen (siehe unten).

### Histogramm?

**Ja.** Für Belichtung, Kontrast und Lichter/Schatten ist es das wichtigste Hilfsmittel, und es ist billig:

- RGB-Histogramm (Rot, Grün, Blau überlagert) des korrigierten Bildes, oben in der Reglerleiste
- Es zeigt sofort, ob Rot nach der Korrektur wirklich da ist – unter Wasser besonders aussagekräftig
- Beschnitt-Warnung: Dreiecke am Histogramm; bei Klick (oder Taste J) werden ausgefressene Lichter rot,
  abgesoffene Schatten blau im Bild markiert
- Gerechnet auf der Vorschau (1,2 MP), wenige Millisekunden, in einem Durchgang mit der Vorschau

## Was für „fertig ohne PhotoCraft“ außerdem nötig ist

Diese Punkte sind keine Filter, entscheiden aber, ob man 50–400 Bilder bequem fertig bekommt:

- **100-%-Ansicht / Lupe:** Doppelklick oder Taste Z zoomt auf 100 %, Ziehen verschiebt. Ohne sie lassen sich
  Schärfe, Rauschen und Partikel nicht beurteilen.
- **Rückgängig / Wiederholen** (Strg+Z / Strg+Y) je Bild.
- **Selektives Übernehmen:** beim Übertragen wählen, welche Gruppen mitkommen (Farbe, Ton, Details, Zuschnitt).
- **Presets pro Wasserart** (schon in v1 vorgesehen): „Rotes Meer“, „See grün“, „mit Blitz“ – gespeichert als
  Abweichung vom Auto-Wert plus Ton/Details.
- Zurückgestellt: Bewertung/Auswahl der Guten und ein Export-Profil „Web“ (siehe Entscheidungen).

## Architektur-Änderungen

- **Einstellungen v2:** `Settings` bekommt Gruppen `tone`, `detail`, `geometry`, `effects`; `.uwcc.json`
  Version 2 liest v1-Dateien weiter (fehlende Gruppen = neutral).
- **Pipeline** (Reihenfolge):
  1. Geometrie (Drehen/Spiegeln, Geraderichten, Zuschnitt)
  2. Farbmatrix (Auto + Abweichung, Rot-Obergrenze)
  3. Dehaze (Unterwasser)
  4. Weißabgleich, Dynamik, Farbmischer
  5. Ton: Belichtung, Kontrast, Lichter/Schatten
  6. Klarheit
  7. Rauschreduzierung (Farbe, Luminanz)
  8. Schärfen
  9. Vignette
  10. Stärke (Überblendung mit dem Original)
- **Analyse** bleibt auf dem ganzen, ungeschnittenen Bild (stabil beim Zuschneiden); optional später
  „Auto auf Zuschnitt“.
- **Nachbarschaftsfilter** mit trennbaren Box-/Gauß-Filtern bzw. Guided Filter (O(n), auf Kerne verteilt);
  Radien in Prozent der Bildbreite.
- **Kern bleibt `no_std`, wo möglich**; Nachbarschaftsfilter brauchen Puffer und kommen in ein neues Crate
  `uwcc-filters` (std). Das PhotoCraft-Plug-in bleibt bei der reinen Farbkorrektur.
- **Leistungsziel:** Vorschau nach Regleränderung ≤ 50 ms auch mit allen Filtern; Export ≤ 2 s pro 16-MP-Bild
  (4 Threads).

## Phasen

Jede Phase endet mit grünen Tests, Messung gegen `uw-fotos` und eigenem Commit.

### Phase 8 – Grundlagen

- [ ] 8.1 `Settings` v2 mit Gruppen, `.uwcc.json` Version 2 (liest v1), Tests
- [ ] 8.2 Crate `uwcc-filters`: Puffer-Pipeline, trennbarer Box-/Gauß-Filter, Guided Filter, relative Radien
- [ ] 8.3 Histogramm (RGB) + Beschnitt-Warnung (Dreiecke, Taste J, farbige Markierung im Bild)
- [ ] 8.4 100-%-Ansicht: Doppelklick/Taste Z, Verschieben per Ziehen; Ausschnitt in voller Auflösung rechnen
- [ ] 8.5 Rückgängig/Wiederholen je Bild (Strg+Z/Strg+Y)

### Phase 9 – Ton und Dehaze

- [ ] 9.1 Belichtung, Kontrast
- [ ] 9.2 Lichter, Schatten (weiche Luminanzmaske über Guided Filter, ohne Halos)
- [ ] 9.3 Dehaze Unterwasser (UDCP aus Grün/Blau, Hintergrundlicht aus dem ganzen Bild, Regler 0–100)
- [ ] 9.4 Abgleich Vorschau ↔ Export (gleiche Wirkung trotz unterschiedlicher Auflösung, Test)

### Phase 10 – Details

- [ ] 10.1 Klarheit (großer Radius, nur Luminanz)
- [ ] 10.2 Farbrauschen reduzieren (Chroma-Glättung kantenerhaltend)
- [ ] 10.3 Luminanzrauschen, einfache Stufe
- [ ] 10.4 Schärfen: Stärke, Radius, Schwelle (Maske ab Schwelle, damit Wasser glatt bleibt)

### Phase 11 – Zuschneiden und Ausrichten

- [ ] 11.1 Zuschnitt-Modus (Taste C): Rahmen, Griffe, Drittel-Raster, Seitenverhältnisse
- [ ] 11.2 Geraderichten (Regler, Linie ziehen), Drehen 90°, Spiegeln
- [ ] 11.3 Export: Pixel aufrecht, Orientation = 1, Vorschaubild aus dem Zuschnitt; Leiste zeigt Zuschnitt
- [ ] 11.4 Tests: Drehungen/Orientierungen 1–8, Zuschnitt in voller Auflösung exakt

### Phase 12 – Viele Bilder

- [ ] 12.1 Selektives Übernehmen (Gruppen wählen; Zuschnitt standardmäßig aus)
- [ ] 12.2 Presets: speichern, anwenden, auf Auswahl anwenden; mitgelieferte für Blauwasser, Grünwasser, Blitz
- [ ] 12.3 Leiste filtern (alle / unbearbeitet / nicht exportiert)

Zurückgestellt (Entscheidung): Bewertung/Auswahl der Guten (Markieren, Sterne) und ein Export-Profil „Web“.

### Phase 13 – Feinschliff (optional)

- [ ] 13.1 Vignette
- [ ] 13.2 Farbmischer vereinfacht (Wasserblau, Cyan, Orange/Gelb: Farbton, Sättigung, Helligkeit)
- [ ] 13.3 Partikel entfernen: Klick-Werkzeug (Fleck aus der Umgebung füllen); später evtl. automatisch
- [ ] 13.4 Programmsymbol (Fenster und .exe)

## Nicht in v2

- Lokale Masken, Pinsel, Ebenen, Montagen, Text
- RAW-Entwicklung (bleibt JPG; RAW weiter über „In … öffnen“)
- Objektivkorrektur, Panorama, HDR, KI-Funktionen

## Entscheidungen

| Frage | Entscheidung |
|---|---|
| Bewertung/Auswahl der Guten | später, nicht in v2 |
| Export-Profil „Web“ | nein, nur volle Größe |
| RAW (z. B. ORF) | nein, es bleibt bei JPG |
| Reihenfolge | Phase 8 Grundlagen, dann Ton + Dehaze (9), Details (10), Zuschneiden (11), Viele Bilder (12) |
