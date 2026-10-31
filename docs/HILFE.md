# UnderwaterPhotoLab – Hilfe

Farbkorrektur für Unterwasserfotos (JPG). Die korrigierten Bilder landen im Unterordner
`uw-korrigiert` und lassen sich von dort in RapidRaw, PhotoCraft, Lightroom & Co. weiterbearbeiten.
Im Programm öffnet **F1** oder der Knopf **„?“** dieselbe Hilfe.

![Hauptansicht](img/ansicht-main.jpg)

## Bilder auswählen und vergleichen

Das Programm hat zwei Ansichten wie RapidRaw: die **Übersicht** mit allen Bildern (Startseite) und die **Bearbeitung** eines Bildes.

### Übersicht

- **Links:** **Ordner öffnen …** (oder einen Ordner bzw. ein Bild ins Fenster ziehen, oder einen Ordner auf `UnderwaterPhotoLab.exe` ziehen), darunter **Bildgröße** (Doppelklick: Standard; bleibt beim nächsten Start), **Vorschaubilder Original | Korrigiert** und alle Angaben zum markierten Bild (Aufnahme, Kamera, Objektiv, Belichtung, Auto-Analyse …). Beim nächsten Start öffnet sich der zuletzt geöffnete Ordner wieder.
- **Auswählen:** Klick wählt ein Bild (Strg+Klick, Umschalt+Klick: mehrere), Pfeiltasten ← → ↑ ↓ wandern durch das Raster. **Doppelklick** oder **Enter** öffnet das Bild zum Bearbeiten.
- **Filtern** (oben rechts): **Alle**, **Unbearbeitet** (noch keine Einstellung geändert), **Nicht exportiert** (noch nie oder mit anderen Einstellungen exportiert) oder **Exportiert** (mit den aktuellen Einstellungen exportiert, grüner Haken). Das markierte Bild bleibt immer sichtbar; Pfeiltasten und Strg+A gelten für die angezeigten Bilder.
- **Impressum & Lizenz** (links unten): zeigt den Startbildschirm mit Autor, Lizenz (GPL-3.0), dem Link zum ursprünglichen Algorithmus und „Buy me a coffee“, falls dir das Programm gefällt; ein Klick hinein oder Esc schließt ihn.
- Schärfere Vorschaubilder lädt das Programm nach dem Öffnen eines Ordners im Hintergrund nach (bei Olympus aus dem großen Vorschaubild der Kamera, wenige Sekunden).

### Bearbeitung

- **Pfeil links oben** (oder Taste **G**): zurück zur Übersicht. Darunter die Symbole **Original**, **Split-Ansicht**, **Zuschneiden** und **Partikel**; beim Darüberfahren erscheint ihr Name.
- **Bild wechseln:** Klick in die Leiste unter dem Bild oder Pfeiltasten ← →. Mausrad über der Leiste oder den Scrollbalken darunter ziehen bzw. in seine Spur klicken blättert.
- **Original zeigen:** Symbol mit dem Auge oder Leertaste gedrückt halten (das Symbol wird dabei blau).
- **Split-Ansicht:** Symbol oder Taste **S**; die Trennlinie mit der Maus ziehen.
- **Zoom:** Doppelklick ins Bild (100 % an dieser Stelle) oder Taste **Z** (100 % in der Mitte). **Mausrad** oder am Touchpad bzw. Touchbildschirm **zwei Finger auf- und zuziehen** zoomt stufenlos bis 400 %; die Stelle unter dem Mauszeiger bleibt stehen. **Ziehen** mit der Maus oder **zwei Finger schieben** am Touchpad verschiebt den Ausschnitt; die Trennlinie der Split-Ansicht greift man direkt an der Linie. Doppelklick, **Z** oder **Esc** zurück zur ganzen Ansicht. So lassen sich Schärfe, Rauschen und Schwebeteilchen beurteilen.
- **Bildinformationen:** oben rechts auf den Dateinamen klicken; ein Fenster neben der Reglerleiste zeigt alle Angaben (Aufnahme, Kamera, Objektiv, Belichtung, Auto-Analyse …). Ein Klick daneben schließt es.
- **Reglerleiste:** Bildinfo, Auto-Korrektur, Stärke und der Umschalter Farbe | Ton | Details stehen fest; nur die Regler der gewählten Gruppe scrollen.
- **„Vorschaubilder Original | Korrigiert“** (links in der Übersicht): bei „Korrigiert“ erscheinen exportierte Bilder in Übersicht und Leiste korrigiert, alle anderen als Original – so sieht man, wo noch etwas zu tun ist.

![Split-Ansicht](img/ansicht-split.jpg)

## Korrigieren

- Die **Auto-Korrektur** stellt Rot wieder her und spreizt die Farbkanäle, für jedes Bild einzeln.
- **Histogramm** oben rechts: Rot-, Grün- und Blaukanal des korrigierten Bildes. Die Dreiecke leuchten, wenn Tiefen (links) oder Lichter (rechts) beschnitten werden, in der Farbe der betroffenen Kanäle.
- **Klick auf ein Dreieck oder Taste J:** beschnittene Stellen im Bild markieren (rot = Lichter, blau = Tiefen).
- **Stärke:** wie stark die Korrektur wirkt (0 % = Original).
- **Rot wiederherstellen:** wie viel Grün/Blau in den Rotkanal zurückgemischt wird.
- **Temperatur** und **Grün ↔ Magenta:** Reststich ausgleichen.
- **Pipette:** Knopf drücken, dann eine eigentlich neutrale Stelle anklicken (Sand, Flosse, Graukarte).
- **Dynamik:** schwache Farben kräftiger, kräftige bleiben.

Die Regler sind in Gruppen geteilt: der Umschalter **Farbe | Ton | Details** unter der Stärke zeigt die jeweilige Gruppe. Die Einstellungen der anderen Gruppen bleiben dabei wirksam. Die Regler oben gehören zu „Farbe“.

### Farbmischer (unten im Reiter „Farbe“)

- **Wasserfarbe:** dreht den Farbton von Blau und Cyan (±30°) – z. B. grünstichiges Wasser blauer.
- **Wasser-Sättigung:** Wasser kräftiger (+) oder blasser (−).
- **Fische/Korallen-Sättigung:** Orange und Gelb (Fische, Korallen, Haut) kräftiger oder blasser.

Wirkt weich um die Mitte der Farbbereiche und kaum auf fast graue Stellen – z. B. Wasser etwas blasser, damit Orange und Gelb hervortreten.

### Ton (Umschalter „Ton“)

- **Dunst entfernen:** nimmt den Schleier aus Schwebeteilchen heraus (Unterwasser-Verfahren aus Grün und Blau; die Wasserfarbe wird aus dem ganzen Bild geschätzt). Das Bild wird dabei oft dunkler – mit Belichtung oder Tiefen ausgleichen.
- **Belichtung:** ±2 Blenden.
- **Kontrast:** S-Kurve; Schwarz, Weiß und die Mitte bleiben.
- **Lichter:** helle Bereiche dunkler (−) oder heller (+). **Tiefen:** dunkle Bereiche heller (+) oder dunkler (−). Beide wirken nach der Helligkeit der Umgebung; Details und Kanten bleiben erhalten.
- **Vignette:** Ecken dunkler (−, lenkt den Blick zur Mitte) oder heller (+, gleicht den Lichtabfall von Blitz oder Domeport aus); bezogen auf das zugeschnittene Bild.

### Details (Umschalter „Details“)

Schärfen und Rauschreduzierung wirken auf einzelne Pixel; die verkleinerte Ansicht zeigt sie nur ungefähr. Zum genauen Beurteilen in die **100-%-Ansicht** gehen (Doppelklick ins Bild oder Taste **Z**). Der zuletzt gewählte Reiter (Farbe, Ton, Details) bleibt beim nächsten Start erhalten.

- **Klarheit:** Strukturen wie Korallen und Schuppen kräftiger (+) oder weicher (−); wirkt am stärksten in den Mitteltönen, ohne Lichthöfe an Kanten.
- **Schärfen:** schärft feine Strukturen (Radius 1 Pixel des Originals); glatte Flächen wie freies Wasser bleiben ungeschärft.
- **Rauschen entfernen:** nimmt vor allem die bunten Flecken heraus, die beim Anheben von Rot in dunklen Bereichen entstehen, und glättet die Körnung (hohe ISO) leicht mit.
- **Doppelklick auf einen Regler** setzt nur diesen zurück; „Zurücksetzen“ setzt alle zurück.
- **Rückgängig / Wiederholen:** **Strg+Z** bzw. **Strg+Y** (oder Strg+Umschalt+Z), oder die Pfeile neben „Zurücksetzen“. Jedes Bild hat seinen eigenen Verlauf (bis 100 Schritte, solange das Programm läuft); ein Zug an einem Regler zählt als ein Schritt. Auch eine Übernahme auf mehrere Bilder lässt sich je Bild zurücknehmen.

## Presets

- **Preset anwenden …** (unter „Auto / Zurücksetzen“) öffnet die Liste: mitgeliefert sind **Blauwasser**, **Grünwasser (See)** und **Mit Blitz** – Ausgangspunkte zum Nachstellen. Ein Klick wendet das Preset auf das aktuelle Bild an, bei mehreren ausgewählten Bildern auf alle (der Knopf zeigt dann die Anzahl).
- Ein Preset enthält Farbe, Ton und Details; bei eingeschaltetem Auto als Abweichung von der Analyse jedes Bildes. Drehen und Zuschnitt der Bilder bleiben.
- **Speichern …** legt die Einstellungen des aktuellen Bildes unter einem Namen ab (gleicher Name ersetzt); eigene Presets lassen sich in der Liste mit **Löschen** entfernen. Gespeichert in `%APPDATA%\UnderwaterPhotoLab\presets.json`.
- Anwenden lässt sich mit Strg+Z (je Bild) oder „Letzte Übernahme rückgängig“ (alle auf einmal) zurücknehmen.

## Zuschneiden und Ausrichten

Das Symbol **Zuschneiden** links (Bearbeitung) oder Taste **C** zeigt das ganze Bild mit dem Zuschnittrahmen und einer Werkzeugleiste. **Fertig**, **C**, **Enter** oder **Esc** beenden den Modus; der Zuschnitt bleibt (Strg+Z nimmt ihn zurück).

- **Rahmen:** an den Ecken oder Kanten ziehen ändert die Größe, innen ziehen verschiebt ihn. Das Drittel-Raster hilft beim Bildaufbau.
- **Seitenverhältnis:** Frei, Original, 3:2, 4:3, 1:1, 16:9, 4:5; **Hoch-/Querformat** tauscht es. Mit festem Verhältnis wird nur an den Ecken gezogen.
- **Geraderichten:** Regler ±45° (Doppelklick: 0°) oder **Linie ziehen** und dann eine Linie entlang des Horizonts oder einer Riffkante ziehen – sie wird waagerecht (oder senkrecht, wenn sie eher steil ist). Das Bild wird dabei so weit vergrößert, dass keine leeren Ecken entstehen.
- **Links/Rechts drehen** (90°) und **Spiegeln**; der Zuschnitt dreht bzw. spiegelt mit. **Zurücksetzen** nimmt Drehen, Spiegeln, Geraderichten und Zuschnitt zurück.
- Der Export schreibt das Bild aufrecht und zugeschnitten (EXIF-Ausrichtung 1), das eingebettete Vorschaubild zeigt den Zuschnitt. Beim Übernehmen auf mehrere Bilder bleiben Zuschnitt und Drehung jedes Bildes erhalten.

## Partikel entfernen

Das Symbol **Partikel** links (Bearbeitung) oder Taste **P** schaltet das Werkzeug ein; **Fertig**, **P**, **Enter** oder **Esc** beenden es.

- **Klick** auf einen Schwebeteilchen-Fleck (Backscatter) füllt ihn aus der passenden Umgebung; der weiße Kreis zeigt die Stelle. **Klick auf einen Kreis** entfernt ihn wieder.
- **Größe** (4–120 px am Bildschirm) – der blaue Kreis am Mauszeiger zeigt sie; der Kreis sollte den Fleck knapp umschließen. In der 100-%-Ansicht (Z) lassen sich kleine Flecken genauer treffen.
- **Alle entfernen** löscht alle Flecken des Bildes; Strg+Z nimmt einzelne Schritte zurück.
- Bis zu 100 Flecken je Bild. Sie gelten nur für dieses Bild und werden beim Übernehmen auf mehrere Bilder nicht mitkopiert.

## Mehrere Bilder

- **Strg+Klick:** Bild dazu oder weg. **Umschalt+Klick:** Bereich. **Strg+A:** alle. **Esc:** Auswahl aufheben.
- Übernommen werden die Einstellungen des **blau umrandeten** Bildes – und zwar nur die Gruppen, deren Schalter unter „Mehrere Bilder“ markiert sind: **Farbe** (Auto, Stärke, Weißabgleich, Dynamik, Rot), **Ton**, **Details**, **Zuschnitt** (Drehen, Geraderichten, Zuschnitt; anfangs aus). Die Wahl bleibt beim nächsten Start erhalten.
- **„Gleiche Werte übernehmen“:** jedes Bild bekommt genau dieselbe Korrektur – für eine Serie aus derselben Situation.
- **„Auto + Abweichung übernehmen“:** jedes Bild behält seine eigene Auto-Analyse, deine Änderungen kommen dazu.
- **„Letzte Übernahme rückgängig“** gilt, bis ein Regler bewegt wird.

## Export und Weiterbearbeiten

- **Export:** Strg+E (aktuelles Bild), Strg+Umschalt+E (Auswahl). Ziel ist immer der Unterordner `uw-korrigiert` neben den Originalen, mit gleichem Dateinamen; EXIF, Farbprofil und Ausrichtung bleiben erhalten, das eingebettete Vorschaubild wird aus dem korrigierten Bild neu erzeugt.
- **JPG-Qualität:** im Menü am Pfeil rechts neben „Aktuelles Bild exportieren“ (Standard 95; Doppelklick auf den Regler setzt zurück).
- **Originale werden nie verändert;** ein erneuter Export ersetzt das vorherige Ergebnis.
- In der Leiste: **✓** = mit den jetzigen Einstellungen exportiert, **●** = bearbeitet.
- **„In … öffnen“** exportiert Fehlendes oder Veraltetes zuerst und übergibt die Bilder dem Programm. Der Pfeil daneben wechselt das Programm; nicht gefundene Programme lassen sich dort suchen, eigene über „Anderes Programm hinzufügen …“.
  - RapidRaw öffnet bei mehreren Bildern nur eines.
  - Lightroom Classic zeigt den Import-Dialog, LightCraft importiert in seine Bibliothek.
  - „Öffnen mit …“: Windows fragt nach dem Programm (nur für ein Bild).
- Die Einstellungen jedes Bildes stehen in `uw-korrigiert\.uwcc.json` und sind beim nächsten Öffnen des Ordners wieder da.
- Programmeinstellungen (JPG-Qualität, Leistenansicht, Programme) stehen in `%APPDATA%\UnderwaterPhotoLab\config.json`; in `programs` lassen sich die Startargumente anpassen (`{files}` = exportierte Dateien, `{folder}` = Ordner `uw-korrigiert`).

![Hilfe im Programm (F1)](img/ansicht-help.jpg)

## Herkunft

Die Auto-Korrektur folgt „underwater-image-color-correction“ von Nikolaj Bech Andersen
(<https://github.com/nikolajbech/underwater-image-color-correction>, GPL-3.0). UnderwaterPhotoLab steht
deshalb ebenfalls unter der GPL-3.0 (siehe `LICENSE`).
