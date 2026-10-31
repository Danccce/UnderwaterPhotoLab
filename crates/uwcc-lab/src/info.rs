//! Bildinformationen in der Reglerleiste: Aufnahmedaten aus dem EXIF sofort beim Auswählen,
//! Bildgröße und Auto-Analyse, sobald das Bild geladen ist. Die Übersicht zeigt dieselben Angaben
//! ohne die Auto-Analyse (`info-photo-rows`); die gehört zur Bearbeitung.

use std::path::Path;

use slint::{ModelRc, VecModel};

use crate::{preview::Loaded, InfoRow, LabWindow};

pub fn clear(app: &LabWindow) {
    app.set_info_title("".into());
    app.set_info_summary("".into());
    app.set_info_rows(ModelRc::default());
    app.set_info_photo_rows(ModelRc::default());
}

const ANALYSIS: &str = "Auto-Analyse";

/// Zeigt die Informationen zu `path`; mit `loaded` zusätzlich Bildgröße und Auto-Analyse.
pub fn show(app: &LabWindow, path: &Path, loaded: Option<&Loaded>) {
    let i = uwcc_photo::read_info(path);
    let exposure: Vec<&str> =
        [&i.exposure_time, &i.aperture, &i.iso].into_iter().filter_map(|v| v.as_deref()).collect();
    let exposure = (!exposure.is_empty()).then(|| exposure.join(" · "));

    let summary: Vec<&str> =
        [i.taken.as_deref(), i.camera.as_deref(), exposure.as_deref()].into_iter().flatten().collect();
    let size = std::fs::metadata(path).ok().map(|m| format!("{:.1} MB", m.len() as f64 / 1_048_576.0).replace('.', ","));

    let mut rows: Vec<(&str, Option<String>)> = vec![
        ("Aufnahme", i.taken.clone()),
        ("Kamera", i.camera.clone()),
        ("Objektiv", i.lens.clone()),
        ("Belichtung", exposure.clone()),
        ("Brennweite", i.focal_length.clone()),
        ("Belichtungskorr.", i.exposure_bias.clone()),
        ("Blitz", i.flash.clone()),
        ("Dateigröße", size),
        ("Software", i.software.clone()),
    ];
    if let Some(l) = loaded {
        let a = &l.auto;
        rows.insert(7, ("Bildgröße", Some(format!("{} × {} px", l.full.width(), l.full.height()))));
        rows.push((
            ANALYSIS,
            Some(format!(
                "Rot wiederherstellen {}°, Levels Rot {}–{}, Grün {}–{}, Blau {}–{}",
                a.hue_shift, a.red.low, a.red.high, a.green.low, a.green.high, a.blue.low, a.blue.high
            )),
        ));
    }
    let rows: Vec<InfoRow> = rows
        .into_iter()
        .filter_map(|(label, value)| Some(InfoRow { label: label.into(), value: value?.into() }))
        .collect();

    app.set_info_title(uwcc_photo::file_name(path).into());
    app.set_info_summary(if summary.is_empty() { "Keine Aufnahmedaten (EXIF) vorhanden.".into() } else { summary.join(" · ").into() });
    let photo_rows: Vec<InfoRow> = rows.iter().filter(|r| r.label != ANALYSIS).cloned().collect();
    app.set_info_rows(ModelRc::new(VecModel::from(rows)));
    app.set_info_photo_rows(ModelRc::new(VecModel::from(photo_rows)));
}

/// Das Bild ließ sich nicht laden: Der Titel bleibt, Zusammenfassung und Zeilen zeigen den Fehler.
pub fn show_error(app: &LabWindow, error: &str) {
    app.set_info_summary(error.into());
    let rows = ModelRc::new(VecModel::from(vec![InfoRow { label: "Fehler".into(), value: error.into() }]));
    app.set_info_rows(rows.clone());
    app.set_info_photo_rows(rows);
}
