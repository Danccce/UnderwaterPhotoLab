//! Fotos laden, Vorschaubilder lesen, Ordner einlesen und korrigiert exportieren.
//!
//! Die Pixel bleiben in der gespeicherten Ausrichtung, EXIF (samt Orientation-Tag) und ICC-Profil
//! werden unverändert übernommen. Bildbetrachter drehen das Ergebnis dadurch genauso wie das
//! Original. Gedreht wird nur für die Anzeige ([`Photo::preview`], [`thumbnail`]).

mod exif;
mod info;
pub mod sidecar;

pub use info::{read_info, PhotoInfo};

use std::{
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
};

pub use image::RgbaImage;
use image::{
    codecs::jpeg::JpegEncoder, imageops, metadata::Orientation, DynamicImage, ExtendedColorType,
    ImageDecoder, ImageEncoder, ImageFormat, ImageReader,
};
use uwcc_core::{analyze, Correction, Settings};

pub const EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "tif", "tiff"];
pub const SUBFOLDER: &str = "uw-korrigiert";
/// Endung für Ergebnisse im selben Ordner: `foto.jpg` → `foto_uw.jpg`.
pub const SUFFIX: &str = "_uw";
pub const DEFAULT_JPEG_QUALITY: u8 = 95;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportTarget {
    /// `uw-korrigiert\` neben den Originalen, gleicher Dateiname.
    #[default]
    Subfolder,
    /// Im selben Ordner mit Endung `_uw`.
    SameFolder,
}

pub struct Photo {
    pub path: PathBuf,
    /// Pixel in der gespeicherten Ausrichtung (nicht gedreht).
    pub rgba: RgbaImage,
    pub orientation: Orientation,
    exif: Option<Vec<u8>>,
    icc: Option<Vec<u8>>,
}

impl Photo {
    pub fn open(path: &Path) -> Result<Self, String> {
        let name = file_name(path);
        let err = |e: &dyn std::fmt::Display| format!("{name}: {e}");
        let mut decoder = ImageReader::open(path)
            .map_err(|e| err(&e))?
            .with_guessed_format()
            .map_err(|e| err(&e))?
            .into_decoder()
            .map_err(|e| err(&e))?;
        let exif = decoder.exif_metadata().ok().flatten();
        let icc = decoder.icc_profile().ok().flatten();
        let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
        let rgba = DynamicImage::from_decoder(decoder).map_err(|e| err(&e))?.to_rgba8();
        Ok(Self { path: path.to_owned(), rgba, orientation, exif, icc })
    }

    /// Die Auto-Analyse des ganzen Bildes.
    pub fn analyze(&self) -> Correction {
        analyze(self.rgba.as_raw())
    }

    /// Verkleinerte, richtig gedrehte, unbearbeitete Kopie für die Anzeige.
    pub fn preview(&self, max: u32) -> RgbaImage {
        let (w, h) = self.rgba.dimensions();
        let scale = (max as f64 / w.max(h) as f64).min(1.0);
        let size = |v: u32| ((v as f64 * scale).round() as u32).max(1);
        let mut img = DynamicImage::ImageRgba8(imageops::thumbnail(&self.rgba, size(w), size(h)));
        img.apply_orientation(self.orientation);
        img.into_rgba8()
    }

    /// Das ganze Bild richtig gedreht, für die 100-%-Ansicht.
    pub fn into_oriented(self) -> RgbaImage {
        let mut img = DynamicImage::ImageRgba8(self.rgba);
        img.apply_orientation(self.orientation);
        img.into_rgba8()
    }

    /// Exportiert nach [`output_path`] und gibt den Pfad zurück.
    pub fn export(&self, settings: &Settings, auto: &Correction, target: ExportTarget, quality: u8) -> Result<PathBuf, String> {
        let out = output_path(&self.path, target);
        self.export_to(settings, auto, &out, quality)?;
        Ok(out)
    }

    /// Volle Auflösung durch die Pipeline, als JPG mit EXIF und ICC-Profil. Erst in eine
    /// Temp-Datei, dann umbenannt; ein vorhandenes Ergebnis wird ersetzt, das Original nie.
    pub fn export_to(&self, settings: &Settings, auto: &Correction, out: &Path, quality: u8) -> Result<(), String> {
        let name = file_name(out);
        let err = |e: &dyn std::fmt::Display| format!("{name}: {e}");
        if same_file(&self.path, out) {
            return Err(format!("{name}: das Ziel ist das Original"));
        }
        if let Some(dir) = out.parent() {
            fs::create_dir_all(dir).map_err(|e| err(&e))?;
        }

        let mut rgba = self.rgba.clone();
        let (w, h) = rgba.dimensions();
        uwcc_filters::render(&mut rgba, w, h, settings, &uwcc_filters::Context::new(*auto, w, h));
        let rgb = DynamicImage::ImageRgba8(rgba).into_rgb8();

        // Das Vorschaubild im EXIF zeigt sonst weiter das unkorrigierte Original (Explorer, Leiste).
        let exif = self.exif.as_ref().map(|e| {
            exif_thumbnail_jpeg(&rgb)
                .and_then(|jpeg| exif::with_thumbnail(e, &jpeg))
                .unwrap_or_else(|| exif::without_thumbnail(e))
        });

        let tmp = out.with_file_name(format!(".{name}.tmp"));
        let written = (|| {
            let mut encoder = JpegEncoder::new_with_quality(BufWriter::new(File::create(&tmp)?), quality);
            if let Some(icc) = &self.icc {
                encoder.set_icc_profile(icc.clone()).map_err(std::io::Error::other)?;
            }
            if let Some(exif) = exif {
                encoder.set_exif_metadata(exif).map_err(std::io::Error::other)?;
            }
            encoder
                .write_image(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8)
                .map_err(std::io::Error::other)
        })();
        if let Err(e) = written.and_then(|()| fs::rename(&tmp, out)) {
            let _ = fs::remove_file(&tmp);
            return Err(err(&e));
        }
        Ok(())
    }
}

/// Längste Kante des eingebetteten EXIF-Vorschaubilds, wie bei Kameras üblich.
const EXIF_THUMB_MAX: u32 = 160;

/// Vorschaubild fürs EXIF: verkleinert, ungedreht (wie die Pixel), ohne JFIF-Kopf.
fn exif_thumbnail_jpeg(rgb: &image::RgbImage) -> Option<Vec<u8>> {
    let (w, h) = rgb.dimensions();
    let scale = (EXIF_THUMB_MAX as f64 / w.max(h) as f64).min(1.0);
    let size = |v: u32| ((v as f64 * scale).round() as u32).max(1);
    let small = imageops::thumbnail(rgb, size(w), size(h));
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 85)
        .write_image(small.as_raw(), small.width(), small.height(), ExtendedColorType::Rgb8)
        .ok()?;
    if jpeg.get(2..4) == Some(&[0xFF, 0xE0]) {
        let len = u16::from_be_bytes([*jpeg.get(4)?, *jpeg.get(5)?]) as usize;
        jpeg.drain(2..4 + len);
    }
    Some(jpeg)
}

/// Öffnet, analysiert und exportiert ein Bild in voller Auflösung; gibt die Ausgabedatei zurück.
pub fn export_file(input: &Path, settings: &Settings, target: ExportTarget, quality: u8) -> Result<PathBuf, String> {
    let photo = Photo::open(input)?;
    let auto = photo.analyze();
    photo.export(settings, &auto, target, quality)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Kleines, richtig gedrehtes Vorschaubild für die Bildleiste: das EXIF-Vorschaubild, wenn
/// vorhanden, sonst aus dem ganzen Bild berechnet (deutlich langsamer).
pub fn thumbnail(path: &Path, max: u32) -> Result<RgbaImage, String> {
    match exif_thumbnail(path, max) {
        Some(t) => Ok(t),
        None => Ok(Photo::open(path)?.preview(max)),
    }
}

/// Nur das EXIF-Vorschaubild, gedreht wie das Bild; `None`, wenn es keins gibt.
pub fn exif_thumbnail(path: &Path, max: u32) -> Option<RgbaImage> {
    let tiff = exif::read_segment(path)?;
    let jpeg = exif::thumbnail_jpeg(&tiff)?;
    let mut img = image::load_from_memory_with_format(jpeg, ImageFormat::Jpeg).ok()?;
    if img.width() > max || img.height() > max {
        img = img.thumbnail(max, max);
    }
    img.apply_orientation(Orientation::from_exif_chunk(&tiff).unwrap_or(Orientation::NoTransforms));
    Some(img.into_rgba8())
}

/// Ausschnitt `(x, y, w, h)` von `img`, auf `(out_w, out_h)` verkleinert (oder unverändert,
/// wenn die Größe gleich bleibt).
pub fn region(img: &RgbaImage, (x, y, w, h): (u32, u32, u32, u32), (out_w, out_h): (u32, u32)) -> RgbaImage {
    let sub = imageops::crop_imm(img, x, y, w, h);
    if (out_w, out_h) == (w, h) {
        sub.to_image()
    } else {
        imageops::thumbnail(&*sub, out_w, out_h)
    }
}

/// Bildkoordinate eines Punkts `(x, y)` in einer Fläche `(w, h)`, in die ein Bild der Größe
/// `(iw, ih)` eingepasst ist (wie `image-fit: contain`); `None` außerhalb des Bildes.
pub fn contain_point(x: f32, y: f32, w: f32, h: f32, iw: u32, ih: u32) -> Option<(u32, u32)> {
    let (iw, ih) = (iw as f32, ih as f32);
    let scale = (w / iw).min(h / ih);
    let px = (x - (w - iw * scale) / 2.0) / scale;
    let py = (y - (h - ih * scale) / 2.0) / scale;
    (px >= 0.0 && py >= 0.0 && px < iw && py < ih).then_some((px as u32, py as u32))
}

pub fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// Ziel eines Exports: `uw-korrigiert\foto.jpg` bzw. `foto_uw.jpg`. Nicht-JPG-Originale bekommen
/// die Endung `.jpg`.
pub fn output_path(input: &Path, target: ExportTarget) -> PathBuf {
    let dir = input.parent().unwrap_or(Path::new(""));
    let stem = input.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    match target {
        ExportTarget::Subfolder => {
            let is_jpg = input
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg"));
            let name = if is_jpg { file_name(input) } else { format!("{stem}.jpg") };
            dir.join(SUBFOLDER).join(name)
        }
        ExportTarget::SameFolder => dir.join(format!("{stem}{SUFFIX}.jpg")),
    }
}

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Ein Ergebnis im selben Ordner (`…_uw.jpg`), das nicht noch einmal bearbeitet werden soll.
pub fn is_output(path: &Path) -> bool {
    path.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.ends_with(SUFFIX))
}

/// Die Bilder eines Ordners, nach Namen sortiert. Unterordner (auch `uw-korrigiert\`) werden
/// nicht gelesen, Ergebnisse mit `_uw` übersprungen.
pub fn scan_folder(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_supported(p) && !is_output(p))
        .collect();
    files.sort_by_key(|p| file_name(p).to_lowercase());
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("uwcc-photo-tests").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Ein kleines JPG mit EXIF (Orientation + Vorschaubild) und ICC-Profil.
    fn write_test_jpeg(path: &Path, orientation: u16) -> (Vec<u8>, Vec<u8>) {
        let thumb = {
            let img = RgbaImage::from_fn(16, 12, |x, _| image::Rgba([200, x as u8 * 10, 50, 255]));
            let rgb = DynamicImage::ImageRgba8(img).into_rgb8();
            let mut buf = Vec::new();
            JpegEncoder::new(&mut buf).write_image(rgb.as_raw(), 16, 12, ExtendedColorType::Rgb8).unwrap();
            buf
        };
        let exif = exif::tests::exif_with_thumbnail(orientation, &thumb);
        let icc = b"kein echtes Profil, nur Bytes zum Durchreichen".to_vec();

        let img = RgbaImage::from_fn(64, 48, |x, y| image::Rgba([20, 60 + x as u8, 100 + y as u8, 255]));
        let rgb = DynamicImage::ImageRgba8(img).into_rgb8();
        let mut enc = JpegEncoder::new_with_quality(File::create(path).unwrap(), 95);
        enc.set_exif_metadata(exif.clone()).unwrap();
        enc.set_icc_profile(icc.clone()).unwrap();
        enc.write_image(rgb.as_raw(), 64, 48, ExtendedColorType::Rgb8).unwrap();
        (exif, icc)
    }

    #[test]
    fn contain_point_maps_into_the_fitted_image() {
        // 400 × 400 in 200 × 100: Maßstab 0,25, Bild bei x = 50..150.
        assert_eq!(contain_point(60.0, 10.0, 200.0, 100.0, 400, 400), Some((40, 40)));
        assert_eq!(contain_point(10.0, 10.0, 200.0, 100.0, 400, 400), None, "im Rand links");
        assert_eq!(contain_point(149.9, 99.9, 200.0, 100.0, 400, 400), Some((399, 399)));
    }

    #[test]
    fn output_paths() {
        let p = Path::new("C:/tauchgang/P4130328.JPG");
        assert_eq!(output_path(p, ExportTarget::Subfolder), PathBuf::from("C:/tauchgang/uw-korrigiert/P4130328.JPG"));
        assert_eq!(output_path(p, ExportTarget::SameFolder), PathBuf::from("C:/tauchgang/P4130328_uw.jpg"));
        let png = Path::new("C:/tauchgang/scan.png");
        assert_eq!(output_path(png, ExportTarget::Subfolder), PathBuf::from("C:/tauchgang/uw-korrigiert/scan.jpg"));
    }

    #[test]
    fn scan_skips_results_subfolders_and_other_files() {
        let dir = temp_dir("scan");
        for f in ["b.jpg", "A.JPG", "a_uw.jpg", "notiz.txt", "c.PNG", "d.cr2"] {
            fs::write(dir.join(f), b"x").unwrap();
        }
        fs::create_dir_all(dir.join(SUBFOLDER)).unwrap();
        fs::write(dir.join(SUBFOLDER).join("A.JPG"), b"x").unwrap();

        let names: Vec<String> = scan_folder(&dir).unwrap().iter().map(|p| file_name(p)).collect();
        assert_eq!(names, ["A.JPG", "b.jpg", "c.PNG"]);
    }

    #[test]
    fn export_keeps_metadata_and_replaces_previous_result() {
        let dir = temp_dir("export");
        let input = dir.join("foto.jpg");
        let (exif, icc) = write_test_jpeg(&input, 6);
        let original = fs::read(&input).unwrap();

        let photo = Photo::open(&input).unwrap();
        assert_eq!(photo.orientation, Orientation::Rotate90);
        assert_eq!(photo.preview(100).dimensions(), (48, 64), "Vorschau gedreht");
        let auto = photo.analyze();

        let out = photo.export(&Settings::default(), &auto, ExportTarget::Subfolder, 95).unwrap();
        assert_eq!(out, dir.join(SUBFOLDER).join("foto.jpg"));
        let saved = Photo::open(&out).unwrap();
        let saved_exif = saved.exif.as_deref().unwrap();
        assert_ne!(exif::thumbnail_jpeg(saved_exif), exif::thumbnail_jpeg(&exif), "neues Vorschaubild");
        assert_eq!(exif_thumbnail(&out, 200).unwrap().dimensions(), (48, 64), "aus dem Export, gedreht wie das Bild");
        assert_eq!(saved.icc.as_deref(), Some(&icc[..]));
        assert_eq!(saved.orientation, Orientation::Rotate90);
        assert_eq!(saved.rgba.dimensions(), (64, 48), "Pixel nicht gedreht");

        let first = fs::read(&out).unwrap();
        let warmer = Settings { temperature: 60.0, ..Settings::default() };
        photo.export(&warmer, &auto, ExportTarget::Subfolder, 95).unwrap();
        assert_ne!(fs::read(&out).unwrap(), first, "erneuter Export ersetzt das Ergebnis");

        let leftovers: Vec<_> = fs::read_dir(dir.join(SUBFOLDER)).unwrap().flatten().map(|e| file_name(&e.path())).collect();
        assert_eq!(leftovers, ["foto.jpg"], "keine Temp-Dateien übrig");
        assert_eq!(fs::read(&input).unwrap(), original, "Original unverändert");
    }

    #[test]
    fn never_overwrites_the_original() {
        let dir = temp_dir("guard");
        let input = dir.join("foto.jpg");
        write_test_jpeg(&input, 1);
        let original = fs::read(&input).unwrap();
        let photo = Photo::open(&input).unwrap();

        let same = dir.join(".").join("foto.jpg");
        assert!(photo.export_to(&Settings::default(), &photo.analyze(), &same, 95).is_err());
        assert_eq!(fs::read(&input).unwrap(), original);
    }

    #[test]
    fn exif_thumbnail_is_read_and_rotated() {
        let dir = temp_dir("thumb");
        let rotated = dir.join("hoch.jpg");
        write_test_jpeg(&rotated, 8);
        assert_eq!(exif_thumbnail(&rotated, 200).unwrap().dimensions(), (12, 16));

        let plain = dir.join("ohne.jpg");
        let rgb = DynamicImage::ImageRgba8(RgbaImage::new(40, 30)).into_rgb8();
        JpegEncoder::new(File::create(&plain).unwrap()).write_image(rgb.as_raw(), 40, 30, ExtendedColorType::Rgb8).unwrap();
        assert!(exif_thumbnail(&plain, 200).is_none());
        assert_eq!(thumbnail(&plain, 20).unwrap().dimensions(), (20, 15), "Rückfall auf Dekodieren");
    }

    /// Gegen echte Fotos: `UWCC_PHOTOS=<Ordner> cargo test -p uwcc-photo --release -- --ignored --nocapture`
    #[test]
    #[ignore = "braucht einen Fotoordner in UWCC_PHOTOS"]
    fn real_photos() {
        let dir = PathBuf::from(std::env::var("UWCC_PHOTOS").unwrap());
        let files = scan_folder(&dir).unwrap();
        println!("{} Bilder", files.len());

        let start = std::time::Instant::now();
        let mut without = Vec::new();
        let mut rotated = Vec::new();
        for f in &files {
            match exif_thumbnail(f, 160) {
                Some(t) if t.height() > t.width() => rotated.push(f.clone()),
                Some(_) => {}
                None => without.push(file_name(f)),
            }
        }
        println!("EXIF-Vorschaubilder: {:?} für {} Bilder", start.elapsed(), files.len());
        println!("ohne EXIF-Vorschaubild: {without:?}");
        println!("Hochformat laut Vorschaubild: {:?}", rotated.iter().map(|p| file_name(p)).collect::<Vec<_>>());

        let mut cameras = std::collections::BTreeMap::new();
        for f in &files {
            let info = read_info(f);
            cameras.entry(info.camera.clone()).or_insert((file_name(f), info));
        }
        for (camera, (name, info)) in &cameras {
            println!("{camera:?} z. B. {name}: {info:#?}");
        }

        // Reglerreaktion: so lange braucht eine Neuberechnung der Vorschau (1280 px).
        let photo = Photo::open(&files[0]).unwrap();
        let auto = photo.analyze();
        let preview = photo.preview(1280);
        let settings = Settings { temperature: 20.0, vibrance: 30.0, ..Settings::default() };
        let start = std::time::Instant::now();
        for _ in 0..20 {
            let mut px = preview.clone();
            uwcc_core::process_rgba8(&mut px, &settings, &auto);
        }
        println!("Vorschau {}×{} neu rechnen: {:?} im Schnitt", preview.width(), preview.height(), start.elapsed() / 20);
        let (w, h) = preview.dimensions();
        let ctx = uwcc_filters::Context::new(auto, w, h);
        let start = std::time::Instant::now();
        for _ in 0..20 {
            let mut px = preview.clone();
            uwcc_filters::render(&mut px, w, h, &settings, &ctx);
        }
        println!("… mit uwcc_filters::render wie in der App: {:?} im Schnitt", start.elapsed() / 20);

        // Stapel-Export: 8 Bilder mit 4 Threads wie in der App, in einen Temp-Ordner.
        let out_dir = std::env::temp_dir().join("uwcc-photo-tests").join("batch");
        let _ = fs::remove_dir_all(&out_dir);
        fs::create_dir_all(&out_dir).unwrap();
        let batch: Vec<PathBuf> = files.iter().take(8).cloned().collect();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let start = std::time::Instant::now();
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(f) = batch.get(i) else { break };
                    let photo = Photo::open(f).unwrap();
                    let auto = photo.analyze();
                    photo.export_to(&Settings::default(), &auto, &out_dir.join(file_name(f)), DEFAULT_JPEG_QUALITY).unwrap();
                });
            }
        });
        let took = start.elapsed();
        println!("Stapel-Export {} Bilder mit 4 Threads: {took:?} ({:?} pro Bild)", batch.len(), took / batch.len() as u32);

        if let Some(f) = rotated.first() {
            let start = std::time::Instant::now();
            let photo = Photo::open(f).unwrap();
            let opened = start.elapsed();
            let auto = photo.analyze();
            let preview = photo.preview(1280);
            assert!(preview.height() > preview.width(), "Vorschau muss Hochformat sein");

            let out = std::env::temp_dir().join("uwcc-photo-tests").join(file_name(f));
            let start = std::time::Instant::now();
            photo.export_to(&Settings::default(), &auto, &out, DEFAULT_JPEG_QUALITY).unwrap();
            let exported = start.elapsed();
            let saved = Photo::open(&out).unwrap();
            assert_eq!(saved.orientation, photo.orientation);
            assert_eq!(saved.rgba.dimensions(), photo.rgba.dimensions());
            println!("{}: öffnen {opened:?}, Export {exported:?}, Auto {auto:?}", file_name(f));
        }
    }
}
