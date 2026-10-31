# UnderwaterPhotoLab – Plan für v2

**Ziel:** Für die meisten Bilder eines Tauchgangs entsteht die fertige Version direkt im
UnderwaterPhotoLab – ohne PhotoCraft, RapidRaw o. Ä. „In … öffnen“ bleibt für Sonderfälle
(Retusche, Montagen, RAW).

Stand vor Beginn: v1 abgeschlossen (`docs/PLAN-v1.md`, letzter Commit `caa0804`).

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
  Radien in Promille der langen Bildkante.
- **Kern bleibt `no_std`, wo möglich**; Nachbarschaftsfilter brauchen Puffer und kommen in ein neues Crate
  `uwcc-filters` (std). Das PhotoCraft-Plug-in bleibt bei der reinen Farbkorrektur.
- **Leistungsziel:** Vorschau nach Regleränderung ≤ 50 ms auch mit allen Filtern; Export ≤ 2 s pro 16-MP-Bild
  (4 Threads).

## Phasen

Jede Phase endet mit grünen Tests, Messung gegen `uw-fotos` und eigenem Commit.

### Phase 8 – Grundlagen

- [x] 8.1 `Settings` v2 mit Gruppen, `.uwcc.json` Version 2 (liest v1), Tests (`5d2f5d3`)
- [x] 8.2 Crate `uwcc-filters`: Puffer-Pipeline, trennbarer Box-/Gauß-Filter, Guided Filter, relative Radien (`5d2f5d3`)
- [x] 8.3 Histogramm (RGB) + Beschnitt-Warnung (Dreiecke, Taste J, farbige Markierung im Bild) (`a78cc83`)
- [x] 8.4 100-%-Ansicht: Doppelklick/Taste Z, Verschieben per Ziehen; Ausschnitt in voller Auflösung rechnen (`11158ce`)
- [x] 8.4b Stufenloser Zoom bis 400 %: Mausrad und Pinch (Windows meldet ihn als Strg+Rad) zoomen um den
  Mauszeiger, zwei Finger auf dem Touchpad verschieben (Wunsch während Phase 8) (`d110575`, Touchbildschirm
  `1aa1ac1`)
- [x] 8.5 Rückgängig/Wiederholen je Bild (Strg+Z/Strg+Y) (`362505d`)

Umgesetzt – Abweichungen und Details:

- **8.1:** Gruppen `tone` (Belichtung, Kontrast, Lichter, Schatten, Dehaze), `detail` (Klarheit, Schärfen,
  Farb- und Luminanzrauschen) und `geometry` (Vierteldrehungen, Spiegeln, Geraderichten, Zuschnitt als Anteil
  des Bildes). Die Gruppe `effects` (Vignette, Farbmischer) kommt erst mit Phase 13. `.uwcc.json` liest auch
  Dateien mit UTF-8-BOM.
- **8.2:** `uwcc_filters::render()` ist die einzige Pipeline für Vorschau, 100-%-Ansicht und Export; sie
  verteilt auf bis zu 8 Kerne. Radien werden in **Promille der langen Kante** des ganzen Bildes angegeben
  (`Context::radius_px`); die 100-%-Ansicht rechnet ihren Ausschnitt mit den Maßen des ganzen Bildes.
  Noch offen für Phase 9/10: Ausschnitte brauchen einen Rand, sobald Nachbarschaftsfilter wirken.
- **8.3:** Histogramm fest über der Reglerleiste, Kanäle halbtransparent überlagert, nur für die Anzeige
  geglättet (die Tonwertspreizung erzeugt sonst einen Kamm). Ein Dreieck leuchtet ab 0,1 % beschnittener Pixel
  in der Mischfarbe der betroffenen Kanäle (Rot, Gelb, Weiß …); kleinere Schwellen leuchteten bei fast jedem
  Bild, weil die Auto-Korrektur absichtlich etwas beschneidet. Markierung: ein Kanal bei 255 → rot,
  ein Kanal bei 0 → blau. Klick auf ein Dreieck oder J schaltet um.
- **8.4:** Ein Bildpixel pro Bildschirmpixel. Das volle Bild bleibt nach dem Laden gedreht im Speicher;
  der sichtbare Ausschnitt wird im Hintergrund gerechnet (höchstens eine Berechnung, danach eine mit dem
  neuesten Stand), beim Ziehen wandert das letzte Ergebnis sofort mit. Doppelklick zoomt auf die Stelle,
  Z auf die Mitte; Esc beendet. Split-Ansicht, Leertaste, Beschnitt-Warnung und Pipette wirken auch im Zoom;
  die Split-Linie greift man im Zoom direkt an der Linie. Der Zoom bleibt beim Bildwechsel erhalten.
- **8.4b:** Maßstab = Bildschirmpixel je Bildpixel. Unter 100 % wird der Ausschnitt erst verkleinert und dann
  gerechnet (Radien passen sich an), ab 100 % in voller Auflösung und über 100 % pixelgenau vergrößert.
  Mausrad und Touchpad unterscheidet das Programm an der Art der Scroll-Meldung (Rasten bzw. Pixel), die es vor
  Slint abfängt. Eine Raste ≈ 15 %; zurück bis zum eingepassten Bild beendet den Zoom.
  Touchbildschirm: Slints `ScaleRotateGestureHandler` liefert die Zwei-Finger-Geste (Abstand → Zoom,
  Mittelpunkt → Verschieben); ein Finger bleibt Ziehen. Ein eigenes Abfangen der rohen Berührungen brachte
  Slints Fingerzählung durcheinander (danach ging Verschieben nur noch mit zwei Fingern) und wurde verworfen.
  Der Mauszeiger wird erst beim Ziehen zum Vier-Wege-Pfeil.
- **8.5:** Verlauf je Bild (`history.rs`), nur für die laufende Sitzung, höchstens 100 Schritte. Änderungen
  desselben Reglers im Abstand unter 1,5 s bilden einen Schritt; Auto, Zurücksetzen, Doppelklick, Pipette und
  Übernahme sind je ein eigener Schritt. Die Übernahme trägt sich in den Verlauf jedes Zielbildes ein; „Letzte
  Übernahme rückgängig“ bleibt für alle Bilder auf einmal. Zusätzlich Strg+Umschalt+Z und zwei Pfeil-Knöpfe
  neben „Zurücksetzen“. Neuer Ordner = neuer Verlauf.
- **Nebenbei:** Der zuletzt geöffnete Ordner steht als `lastFolder` in `config.json` und wird beim Start ohne
  Argument wieder geöffnet.

### Phase 9 – Ton und Dehaze

- [x] 9.1 Belichtung, Kontrast
- [x] 9.2 Lichter, Schatten (weiche Luminanzmaske über Guided Filter, ohne Halos)
- [x] 9.3 Dehaze Unterwasser (UDCP aus Grün/Blau, Hintergrundlicht aus dem ganzen Bild, Regler 0–100)
- [x] 9.4 Abgleich Vorschau ↔ Export (gleiche Wirkung trotz unterschiedlicher Auflösung, Test)

Alles in `2592afc`. Umgesetzt – Details:

- **Pipeline:** `uwcc_core::PixelOps` teilt die Schritte je Pixel (Farbmatrix · Weißabgleich/Dynamik/Belichtung
  · Kontrast/Stärke); `uwcc_filters::render` setzt Dehaze nach der Farbmatrix und Lichter/Tiefen nach der
  Belichtung dazwischen. Ohne diese beiden bleibt der schnelle Weg je Pixel.
- **9.1:** Belichtung ±100 = ±2 Blenden, auf den gamma-kodierten Werten als `×2^(EV/2,2)` (wirkt wie in
  linearem Licht). Kontrast: S-Kurve `v + a·(3v² − 2v³ − v)`, monoton, Schwarz/Mitte/Weiß bleiben.
- **9.2:** Faktor `2^(1,2 · (Tiefen · w_T + Lichter · w_L))` nach der geglätteten Umgebungshelligkeit
  (Guided Filter, Radius 30 ‰, eps 0,02); Gewichte quadratisch unter 0,6 bzw. über 0,4.
- **9.3:** Wasserfarbe aus einer 256-px-Referenz des ganzen Bildes (`Photo::reference`, nach der Farbmatrix:
  hellste 0,1 % im dunklen Kanal aus Grün/Blau). Durchlässigkeit `1 − 0,95 · Stärke · min(G/A, B/A)` über
  8 ‰, mit Guided Filter (30 ‰) an die Kanten angepasst, Untergrenze 0,1.
- **Geschwindigkeit:** Die Filter mit Nachbarschaft rechnen ihre Koeffizienten auf einem groben Raster (Radius
  ≈ 4 Zellen, Fast Guided Filter) und wenden sie mit dem vollen Leitbild an; das Raster ist am ganzen Bild
  ausgerichtet (`Context::origin`), damit Zoom-Ausschnitte (mit `margin_px` Rand) exakt dem ganzen Bild
  entsprechen. Gemessen im Akkubetrieb: Vorschau 1280 px mit Ton und Dehaze ≈ 39 ms (ohne ≈ 10–15 ms),
  16 MP ≈ 0,33 s (anfangs 3,1 s).
- **9.4:** Tests: Vorschau (verkleinert gerechnet) ≈ Export (voll gerechnet, dann verkleinert), mittlere
  Abweichung < 4 von 255; Ausschnitt mit Rand = ganzes Bild (±1). Sichtprüfung mit `tone_samples`.

### Zwischenschritt – Oberfläche aufgeräumt (Wünsche nach Phase 9)

Mit Ton und Dehaze wurde die Reglerleiste länger als das Fenster; deshalb vor Phase 10:

- [x] Umschalter **Farbe | Ton | Details** unter der Stärke; darunter nur die Regler der gewählten Gruppe.
  „Details“ zeigt bis Phase 10 einen Hinweis. Der Umschalter ist allgemein (`Segmented` mit beliebig vielen
  Teilen) und ersetzt auch „Original | Korrigiert“ in der Leiste (`13bd1b5`).
- [x] **JPG-Qualität** aus der Reglerleiste in ein Menü am Pfeil neben „Aktuelles Bild exportieren“
  („Export-Einstellungen“, schließt nur bei Klick daneben, damit der Regler sich ziehen lässt) (`2bb9f89`).
- [x] **Original** und **Split-Ansicht** aus der Reglerleiste in die Leiste unter dem Bild, links vor dem
  Ordnerpfad (`2bb9f89`); als flache 24-px-Knöpfe (`SmallButton`), damit sie in die Leiste passen (`d30c027`).
- [x] **Bildinformationen** in einem Fenster links neben der Reglerleiste (Klick auf den Dateinamen); in der
  Leiste bleibt die Kurzinfo (`4e1f2a7`).
- [x] **Feste Reglerleiste:** fest stehen Histogramm, Dateiname mit Kurzinfo, Auto/Zurücksetzen/Rückgängig,
  Stärke und der Umschalter; nur die Regler der Gruppe scrollen. „Mehrere Bilder“ und Export bleiben unten
  fest (`4e1f2a7`).
- [x] **Eine Markierungsfarbe:** alle Hervorhebungen (Umschalter, Knöpfe, aktuelles Bild in der Leiste,
  Split-Griff, Menüs) nutzen `Palette.accent-background` wie Regler, Checkbox und Hauptknopf; halbtransparente
  Flächen werden daraus abgeleitet (`25c7241`).
- [x] **Scrollbalken unter der Bildleiste**, immer sichtbar (`HScrollbar`; der eingebaute Fluent-Balken
  erscheint erst beim Darüberfahren und ist abgeschaltet): Griff ziehen oder in die Spur klicken; Griff
  mindestens 40 px breit (Wunsch nach Phase 10).

### Phase 10 – Details

- [x] 10.1 Klarheit (großer Radius, nur Luminanz)
- [x] 10.2 Farbrauschen reduzieren (Chroma-Glättung kantenerhaltend)
- [x] 10.3 Luminanzrauschen, einfache Stufe
- [x] 10.4 Schärfen: Stärke, Radius, Schwelle (Maske ab Schwelle, damit Wasser glatt bleibt)
- [x] 10.5 Regler in die Gruppe „Details“ des Umschalters (ersetzt den Hinweis)

Umgesetzt – Details:

- **Reihenfolge:** nach Lichter/Tiefen auf Helligkeit Y und Farbanteilen R − Y, B − Y: Klarheit →
  Farbrauschen → Helligkeitsrauschen → Schärfen; danach wie bisher Kontrast und Stärke.
- **10.1 Klarheit** (−100 … +100): Abstand zur kantenerhaltend geglätteten Grundhelligkeit (Fast Guided Filter
  auf dem Raster, 20 ‰, eps 0,01) verstärkt bzw. abgeschwächt, gewichtet `1 − (2Y − 1)²` (Mitteltöne).
- **10.2 Farbrauschen:** R − Y und B − Y mit Y als Leitbild geglättet (Radius 1–6 px, eps 0,0025; beide
  Ebenen teilen Mittel und Varianz des Leitbilds).
- **10.3 Helligkeitsrauschen:** Y mit sich selbst als Leitbild, Radius 1–3 px, geglättete Schwankung bis 0,03.
- **10.4 Schärfen:** Unscharf maskieren auf Y (Gauß mit dem Radius, Stärke bis 150 %). Neue Felder in
  `Detail`: `sharpen_radius` (0,5–3 px, Standard 1) und `sharpen_mask` (0–100, wie „Maskieren“ in
  Lightroom: geschärft wird nur, wo die mittlere Abweichung von der unscharfen Kopie über der Schwelle
  liegt). `.uwcc.json` schreibt `sharpenRadius`, `sharpenMask`; fehlt der Radius, gilt 1.
- **Radien:** Schärfen und Rauschen in Pixeln der **vollen Auflösung** (`Context::full_px`, dafür merkt sich
  die `Reference` die lange Kante des Originals). In der verkleinerten Vorschau wäre der Radius unter einem
  Pixel; dort rechnen sie mit mindestens einem Pixel (Schärfen mit 3 × 3-Mittel) und zeigen die Wirkung
  ungefähr. Zuerst entfielen sie dort ganz – das wirkte, als hätten die Regler keine Funktion. Genau
  beurteilt wird in der 100-%-Ansicht; darauf weist eine Zeile über den Reglern hin.
- **Nachjustiert nach Test in der App:** Die Auto-Korrektur spreizt die Kanäle bis ×4, das Rauschen ist danach
  kräftiger als angenommen. Helligkeitsrauschen glättet daher Schwankungen bis 0,08 (vorher 0,03),
  Farbrauschen mit Radius bis 8 px und eps 0,02 (vorher 6 px, 0,0025; mit kleinem eps trug das Rauschen des
  Leitbilds das Farbrauschen hindurch).
- **Oberfläche:** Der Hinweis über den Detail-Reglern ist einzeilig; umbrochener Text zählt in der Höhe des
  Scrollbereichs nur einzeilig, der letzte Regler („Rauschen: Helligkeit“) war deshalb halb abgeschnitten.
  Der gewählte Reiter wird als `panelTab` in `config.json` gemerkt.
- **Rand für Ausschnitte:** `margin_px` addiert jetzt die Reichweiten aller aktiven Filter (sie wirken
  nacheinander); vorher galt nur die größte.
- **Geschwindigkeit:** Ebenen-Operationen laufen parallel; der Box-Filter rechnet Spalten zeilenweise mit
  laufenden Summen je Spaltenstreifen (vorher Spalte für Spalte, bei 16 MP sehr cache-unfreundlich).
  Vorschau mit Ton, Dehaze und allen Details ≈ 41–51 ms; 16 MP ≈ 2,4 s – gemessen im Akkubetrieb mit stark
  schwankenden Werten (einfacher Weg dabei 12–33 ms statt sonst ≈ 11 ms); am Netzteil nachmessen.
- **Tests:** Schärfen hebt Kanten, Maskieren lässt verrauschtes Wasser ruhig; Farbrauschen halbiert die
  Farbstreuung bei gleicher Helligkeit; Klarheit hebt bzw. senkt den lokalen Kontrast; Ausschnitt mit Rand =
  ganzes Bild auch mit allen Details. Sichtprüfung `detail_samples` (ISO-1600-Ausschnitt in voller Auflösung).

### Phase 11 – Zuschneiden und Ausrichten

- [x] 11.1 Zuschnitt-Modus (Taste C): Rahmen, Griffe, Drittel-Raster, Seitenverhältnisse
- [x] 11.2 Geraderichten (Regler, Linie ziehen), Drehen 90°, Spiegeln
- [x] 11.3 Export: Pixel aufrecht, Orientation = 1, Vorschaubild aus dem Zuschnitt; Leiste zeigt Zuschnitt
- [x] 11.4 Tests: Drehungen/Orientierungen 1–8, Zuschnitt in voller Auflösung exakt

Umgesetzt – Details (`092f57c` Grundlage, danach Anzeige und Zuschnitt-Modus):

- **Geometrie** (`uwcc_filters::geometry`): aufrechtes Bild → waagerecht spiegeln → Viertel im Uhrzeigersinn
  = *Rahmen*; Geraderichten dreht den Inhalt um die Mitte und vergrößert um `cos θ + (lang/kurz) · sin θ`, damit
  keine leeren Ecken bleiben (der Rahmen behält seine Größe); der Zuschnitt (Anteile 0–1) bezieht sich auf den
  Rahmen. Ein Abtastschritt je Ausgabepixel: ohne Geraderichten exakt, sonst bilinear.
- **Bedienung:** „Spiegeln“ spiegelt so, wie das Bild angezeigt wird (hochkant: Spiegeln plus 180°), und
  spiegelt Zuschnitt und Winkel mit; Drehen dreht den Zuschnitt mit (`Geometry::rotate/flip` im Kern).
  Geraderichten ist ein normales Regler-Feld (`Field::Straighten`): Rückgängig, Doppelklick, Zusammenfassen.
- **Zuschnitt-Modus:** Werkzeugleiste oben (Seitenverhältnis Frei/Original/3:2/4:3/1:1/16:9/4:5 mit
  Hoch/Quer, Drehen, Spiegeln, Zurücksetzen, Fertig; Geraderichten-Regler und „Linie ziehen“). Gezogen wird
  in Slint, beim Loslassen geht der Zuschnitt an Rust (ein Rückgängig-Schritt). Mit festem Verhältnis nur
  Ecken. Zoom, Split-Ansicht und Mausrad-Zoom sind im Modus aus.
- **Anzeige:** Vorschau, Original-Vergleich, Pipette, Histogramm und Zoom zeigen das Ergebnis nach der
  Geometrie; das ganze Bild nach der Geometrie für den Zoom wird einmal im Hintergrund gerechnet und
  gemerkt. `Context::scale` (Pixel je Original-Pixel) ist jetzt ausdrücklich, weil die lange Kante nach einem
  Zuschnitt nichts mehr über die Auflösung sagt.
- **Export:** immer aufrecht mit Orientation 1 (`exif::upright`), auch ohne Geometrie; das EXIF-Vorschaubild
  zeigt den Zuschnitt, die Leiste („Korrigiert“) daher auch.
- **Übernehmen auf mehrere Bilder** überträgt Drehen, Geraderichten und Zuschnitt nicht (vorgezogen aus 12.1).
- **Tests:** Vierteldrehungen und Spiegeln pixelgenau, Zuschnitt exakt, Geraderichten füllt den Rahmen,
  Drehen/Spiegeln heben sich auf, alle acht EXIF-Ausrichtungen exportieren aufrecht wie die Vorschau und
  mit Drehen + Zuschnitt in der richtigen Größe; EXIF-Orientation in beiden Byte-Reihenfolgen.

### Phase 12 – Viele Bilder

- [x] 12.1 Selektives Übernehmen (Gruppen wählen; Zuschnitt standardmäßig aus) (`17c4d3e`)
- [x] 12.2 Presets: speichern, anwenden, auf Auswahl anwenden; mitgelieferte für Blauwasser, Grünwasser, Blitz (`253db9e`)
- [x] 12.3 Leiste filtern (alle / unbearbeitet / nicht exportiert)

Umgesetzt – Details:

- **12.1:** `uwcc_core::Groups` (Farbe = Auto, Abweichung, Stärke, Weißabgleich, Dynamik, Rot-Obergrenze; Ton;
  Details; Zuschnitt = ganze Geometrie) und `Settings::with_groups`. Vier Schalter unter „Mehrere Bilder“,
  gemerkt als `transferGroups` in `config.json`. Übernehmen, Presets und deren Rückgängig laufen über eine
  gemeinsame Funktion (`selection::apply`), die auch das gerade geladene Bild mitführt.
- **12.2:** Mitgeliefert „Blauwasser“, „Grünwasser (See)“, „Mit Blitz“ (relativ zu Auto, ohne Geometrie);
  eigene in `%APPDATA%\UnderwaterPhotoLab\presets.json` im Format der `.uwcc.json`. Menü „Preset anwenden …“
  (bei Auswahl „Preset auf N Bilder …“), „Speichern …“ mit Namensfeld, eigene löschbar. Angewendet werden
  Farbe, Ton und Details; Drehen und Zuschnitt bleiben.
- **12.3:** `Thumb.slot` (Platz in der gefilterten Leiste, −1 ausgeblendet) rechnet `store::refresh_marks`;
  das aktuelle Bild bleibt immer sichtbar. Pfeiltasten springen über ausgeblendete, Strg+A wählt nur
  angezeigte. Die Infozeile nennt mit Filter zusätzlich die Anzahl der angezeigten.
- **Offen:** Bei großer Auswahl nehmen „Mehrere Bilder“ und die Export-Knöpfe viel Platz; der Scrollbereich der
  Regler wird dann klein.

Zurückgestellt (Entscheidung): Bewertung/Auswahl der Guten (Markieren, Sterne) und ein Export-Profil „Web“.

### Phase 13 – Feinschliff (optional)

- [x] 13.1 Vignette
- [x] 13.2 Farbmischer vereinfacht (Wasserblau, Cyan, Orange/Gelb: Farbton, Sättigung, Helligkeit)

Umgesetzt – 13.1/13.2: neue Gruppe `effects` in `Settings` und `.uwcc.json` (Vignette, drei `ColorRange`).
Vignette ±1 Blende in den Ecken, weich ab einem Drittel, elliptisch nach dem Seitenverhältnis des ganzen
(zugeschnittenen) Bildes; Ausschnitte über `Context::origin`/`whole`, daher stimmt sie im Zoom. Farbmischer in
`PixelOps::color` nach der Dynamik: HSV, Bereiche mit Kosinus-Übergang (Wasserblau 215° ±35°, Cyan 180° ±25°,
Orange/Gelb 35° ±35°), schwach gesättigte Pixel kaum; Pixel außerhalb aller Bereiche bleiben exakt. Beim
Übertragen gehört der Farbmischer zur Gruppe Farbe, die Vignette zu Ton. Oberfläche: Vignette im Reiter
„Ton“, Farbmischer unten im Reiter „Farbe“ mit Umschalter der drei Bereiche. Außerdem: Die feste
Statuszeile ist ersetzt durch eine kurze Meldung unten im Bild (5 s).
- [x] 13.3 Partikel entfernen: Klick-Werkzeug (Fleck aus der Umgebung füllen); später evtl. automatisch

Umgesetzt – 13.3: `Settings.spots` (bis 100 Flecken, Lage als Anteil des aufrechten Bildes, Radius als Anteil
der langen Kante), in `.uwcc.json` als `"spots": [[x, y, r], …]`; wird nie auf andere Bilder übertragen.
`uwcc_filters::heal` bessert vor Geometrie und Filtern aus: sucht in 16 Richtungen (Abstand 2,8 r und 4 r) die
Quelle, deren Ring (1,35–1,9 r) am besten zur Umgebung passt, gleicht die Helligkeit an und füllt mit weichem
Rand (voll bis 0,9 r, aus bis 1,3 r). Vorschau, Zoom und Export nutzen dieselbe Funktion; die Caches hängen
an Geometrie und Flecken. Umrechnung Bildschirm ↔ aufrechtes Bild über `geometry::Mapping`. Oberfläche:
Knopf „Partikel“ unter dem Bild oder Taste P; Klick setzt einen Fleck, Klick auf einen Kreis entfernt ihn,
Größe 4–120 px (Bildschirm), „Alle entfernen“; Strg+Z nimmt zurück.
- [x] 13.4 Programmsymbol (Fenster und .exe): `assets\make-icon.ps1` zeichnet es (Ozeanblau, Ring weiß/
  korallenorange für das wiederhergestellte Rot, Luftblasen) in 16–256 px und packt `icon.ico`; Fenster über
  Slint (`icon`), .exe über `app.rc` und `embed-resource` (windres der MinGW-Toolchain).

- [x] 13.5 Rasteransicht wie bei RapidRaw: Knopf „Raster (G)“ unter dem Bild oder Taste G zeigt statt der
  Vorschau alle angezeigten (gefilterten) Bilder als Raster; die Zellen (mind. 190 px) füllen die Breite.
  Klick wählt (Strg/Umschalt wie in der Leiste), ← → ↑ ↓ wandern (↑ ↓ eine Zeile, `store::next_shown` mit
  Schrittweite), Doppelklick oder Enter öffnet das Bild zum Bearbeiten. Im Raster sind Bildleiste,
  Reglerleiste und Ansicht-Knöpfe aus (Bearbeiten erst nach dem Öffnen); unten stattdessen „Ordner öffnen …“
  und „Größe“ (110–320 px Mindestbreite, `gridSize` in config.json). Schärfere Vorschaubilder (320 px) lädt
  `strip::load` in einem zweiten Durchgang: `uwcc_photo::sharp_thumbnail` nimmt das an die Datei angehängte
  große Vorschaubild der Kamera (Olympus 1920 × 1440, ~17 ms je Bild), sonst JPG mit IDCT-Verkleinerung
  (`jpeg-decoder`, ~240 ms), sonst ganz dekodiert.

- [x] 13.6 Aussehen und Aufbau wie RapidRaw: dunkler Grund, abgerundete Flächen mit 8 px Abstand. Die
  **Übersicht** ist die Startseite: links „Ordner öffnen …“, „Bildgröße“, „Vorschaubilder Original |
  Korrigiert“ und alle Angaben zum markierten Bild; oben Titel, Ordner, Filter und „?“; quadratische Zellen,
  Bild füllt die Zelle, Name unten links auf einem Verlauf. Die **Bearbeitung** hat links eine Spalte mit
  „Zurück zur Übersicht“ und darunter den Symbolen Original, Split-Ansicht, Zuschneiden, Partikel
  (`ToolButton`, Hinweis beim Darüberfahren); die Bildleiste ohne „Ordner öffnen“ und ohne Namen sitzt unter
  dem Bild. Die Leiste unter dem Bild mit „Raster“, Filter und Pfad entfällt; G wechselt weiter.
  Dazu: Bilder in Übersicht und Bildleiste werden beim Darüberfahren sanft um 6 % größer (180 ms); die
  Kopfleiste des Fensters hat die Fensterfarbe (winit `WindowExtWindows`, ab Windows 11).

Nachgeschärft – weniger Regler (27 → 18): Rot-Obergrenze ohne Regler (bleibt beim gespeicherten Wert,
Standard 4). Schärfen ein Regler mit festem Radius 1 px und Maskieren 30 (`Detail::with_sharpen`;
`DEFAULT_SHARPEN_MASK`, auch für `.uwcc.json` ohne `sharpenMask`). „Rauschen entfernen“ statt zwei Reglern:
Farbe = Wert, Helligkeit = 0,3 × Wert (`Detail::with_noise`). Farbmischer ohne Umschalter: „Wasserfarbe“ und
„Wasser-Sättigung“ setzen Wasserblau und Cyan gemeinsam, dazu „Fische/Korallen-Sättigung“ (Orange/Gelb).
Ältere Bilder behalten Werte der entfernten Regler bis „Zurücksetzen“. Meldungen unten im Bild über
`notify()`; sie verschwinden nach 5 s, auch eine gleichlautende erscheint erneut.

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
| Reglerleiste | Gruppen Farbe / Ton / Details per Umschalter; nur die Regler scrollen, der Rest steht fest |
| Anzahl Regler | so wenige wie möglich: keine Rot-Obergrenze, Schärfen und Rauschen je ein Regler, Farbmischer drei Regler |
| Ansicht-Knöpfe | als Symbole in der Spalte links der Bearbeitung (früher unter dem Bild), nicht in der Reglerleiste |
| Startseite | die Übersicht (Raster) wie in RapidRaw; Doppelklick öffnet die Bearbeitung |
| JPG-Qualität | im Menü am Export-Knopf, nicht in der Reglerleiste |
| Markierungsfarbe | eine für alles: die Akzentfarbe der Slint-Palette |
| Versionsverwaltung | GitHub [Danccce/UnderwaterPhotoLab](https://github.com/Danccce/UnderwaterPhotoLab), Commits mit der noreply-Adresse |
