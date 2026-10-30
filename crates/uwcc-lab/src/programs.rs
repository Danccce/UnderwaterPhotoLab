//! „In [Programm] öffnen“: bekannte Bildprogramme finden, die Auswahl merken und die
//! exportierten Dateien übergeben – vorher wird exportiert, was fehlt oder veraltet ist.

use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};

use slint::{ModelRc, VecModel};

use crate::{config::ProgramPath, export, store, with_state, LabWindow, ProgramItem};

pub const DEFAULT_PROGRAM: &str = "rapidraw";
const WINDOWS_DEFAULT: &str = "windows";
const OPEN_WITH: &str = "open-with";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Exe,
    /// Jede Datei mit dem in Windows für JPG eingestellten Programm.
    WindowsDefault,
    /// Der Windows-Dialog „Öffnen mit“ (nur für eine Datei).
    OpenWith,
}

#[derive(Clone, Debug)]
pub struct Program {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    /// `None`: nicht gefunden.
    pub exe: Option<PathBuf>,
    pub args: Vec<String>,
}

struct Known {
    id: &'static str,
    name: &'static str,
    /// Übliche Installationsorte; `%VAR%` wird ersetzt, `*` passt auf einen Ordnernamen.
    candidates: &'static [&'static str],
}

const KNOWN: [Known; 5] = [
    Known {
        id: "rapidraw",
        name: "RapidRaw",
        candidates: &[
            r"%LOCALAPPDATA%\RapidRAW\RapidRAW.exe",
            r"%LOCALAPPDATA%\Programs\RapidRAW\RapidRAW.exe",
            r"%ProgramFiles%\RapidRAW\RapidRAW.exe",
        ],
    },
    Known {
        id: "photocraft",
        name: "PhotoCraft",
        candidates: &[r"%ProgramFiles%\PhotoCraft\photocraft.exe", r"%LOCALAPPDATA%\Programs\PhotoCraft\photocraft.exe"],
    },
    Known {
        id: "lightcraft",
        name: "LightCraft",
        candidates: &[r"%ProgramFiles%\LightCraft\lightcraft.exe", r"%LOCALAPPDATA%\Programs\LightCraft\lightcraft.exe"],
    },
    Known {
        id: "photoshop",
        name: "Photoshop",
        candidates: &[r"%ProgramFiles%\Adobe\Adobe Photoshop*\Photoshop.exe"],
    },
    Known {
        id: "lightroom",
        name: "Lightroom Classic",
        candidates: &[r"%ProgramFiles%\Adobe\Adobe Lightroom Classic\Lightroom.exe"],
    },
];

fn expand_env(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        let end = rest[start + 1..].find('%')? + start + 1;
        out.push_str(&rest[..start]);
        out.push_str(&std::env::var(&rest[start + 1..end]).ok()?);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Erster vorhandener Kandidat; bei `*` im Ordnernamen der alphabetisch letzte Treffer (neueste Version).
fn find(candidates: &[&str]) -> Option<PathBuf> {
    candidates.iter().filter_map(|c| expand_env(c)).find_map(|c| {
        let Some(star) = c.find('*') else {
            return Path::new(&c).is_file().then(|| PathBuf::from(&c));
        };
        let dir_start = c[..star].rfind('\\')?;
        let dir_end = star + c[star..].find('\\')?;
        let (parent, prefix, rest) = (&c[..dir_start], &c[dir_start + 1..star], &c[dir_end + 1..]);
        let mut hits: Vec<PathBuf> = std::fs::read_dir(parent)
            .ok()?
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
            .map(|e| e.path().join(rest))
            .filter(|p| p.is_file())
            .collect();
        hits.sort();
        hits.pop()
    })
}

/// Alle Programme: die bekannten (selbst gewählter Pfad vor der Suche), eigene, dann die
/// beiden Windows-Einträge.
pub fn list(own: &[ProgramPath]) -> Vec<Program> {
    let override_for = |id: &str| own.iter().find(|p| p.id == id && p.exe.is_file());
    let mut out: Vec<Program> = KNOWN
        .iter()
        .map(|k| {
            let own = override_for(k.id);
            Program {
                id: k.id.to_owned(),
                name: k.name.to_owned(),
                kind: Kind::Exe,
                exe: own.map(|p| p.exe.clone()).or_else(|| find(k.candidates)),
                args: own.map_or_else(|| vec!["{files}".to_owned()], |p| p.args.clone()),
            }
        })
        .collect();
    out.extend(own.iter().filter(|p| p.id.starts_with("custom-")).map(|p| Program {
        id: p.id.clone(),
        name: p.name.clone(),
        kind: Kind::Exe,
        exe: p.exe.is_file().then(|| p.exe.clone()),
        args: p.args.clone(),
    }));
    out.push(Program {
        id: WINDOWS_DEFAULT.into(),
        name: "Windows-Standardprogramm".into(),
        kind: Kind::WindowsDefault,
        exe: None,
        args: Vec::new(),
    });
    out.push(Program {
        id: OPEN_WITH.into(),
        name: "Öffnen mit …".into(),
        kind: Kind::OpenWith,
        exe: None,
        args: Vec::new(),
    });
    out
}

fn available(p: &Program) -> bool {
    p.kind != Kind::Exe || p.exe.is_some()
}

/// Das gewählte Programm; ist es nicht (mehr) vorhanden, das erste gefundene.
fn chosen(programs: &[Program], id: &str) -> Option<Program> {
    programs.iter().find(|p| p.id == id && available(p)).or_else(|| programs.iter().find(|p| available(p))).cloned()
}

/// Liste neu aufbauen und Knopf, Hinweis und Menü aktualisieren.
pub fn refresh(app: &LabWindow) {
    let (programs, id) = with_state(|st| {
        st.programs = list(&st.config.programs);
        (st.programs.clone(), st.config.open_with.clone())
    });
    let current = chosen(&programs, &id);
    let items: Vec<ProgramItem> = programs
        .iter()
        .map(|p| ProgramItem {
            name: p.name.clone().into(),
            found: available(p),
            chosen: current.as_ref().is_some_and(|c| c.id == p.id),
        })
        .collect();
    app.set_programs(ModelRc::new(VecModel::from(items)));
    app.set_open_with_name(current.map_or("…".into(), |p| p.name.into()));
}

/// Menüeintrag gewählt; ein nicht gefundenes Programm wird per Dateiauswahl gesucht.
pub fn choose(app: &LabWindow, index: usize) {
    let Some(p) = with_state(|st| st.programs.get(index).cloned()) else { return };
    if !available(&p) {
        let Some(exe) = pick_exe(&format!("{} suchen", p.name)) else { return };
        crate::update_config(app, |c| {
            c.programs.retain(|o| o.id != p.id);
            c.programs.push(ProgramPath { id: p.id.clone(), name: p.name.clone(), exe, args: p.args.clone() });
        });
    }
    crate::update_config(app, |c| c.open_with = p.id.clone());
    refresh(app);
}

/// Ein eigenes Programm über seine .exe hinzufügen und gleich auswählen.
pub fn add(app: &LabWindow) {
    let Some(exe) = pick_exe("Programm zum Öffnen der Bilder wählen") else { return };
    let name = exe.file_stem().map_or_else(|| "Programm".to_owned(), |s| s.to_string_lossy().into_owned());
    crate::update_config(app, |c| {
        let n = (1..).find(|n| !c.programs.iter().any(|p| p.id == format!("custom-{n}"))).unwrap_or(1);
        let id = format!("custom-{n}");
        c.programs.push(ProgramPath { id: id.clone(), name, exe, args: vec!["{files}".to_owned()] });
        c.open_with = id;
    });
    refresh(app);
}

fn pick_exe(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).add_filter("Programm", &["exe"]).pick_file()
}

/// Knopf „In … öffnen“: aktuelles Bild oder Auswahl; fehlende oder veraltete Exporte zuerst.
pub fn open(app: &LabWindow) {
    let (targets, missing) = with_state(|st| {
        let indices: Vec<usize> = if st.selection.len() > 1 {
            st.selection.iter().copied().collect()
        } else {
            usize::try_from(app.get_current()).ok().into_iter().collect()
        };
        let targets: Vec<PathBuf> = indices.iter().filter_map(|&i| st.files.get(i).cloned()).collect();
        let missing: Vec<PathBuf> = targets.iter().filter(|p| !store::is_exported(st, p)).cloned().collect();
        (targets, missing)
    });
    if targets.is_empty() {
        return;
    }
    if missing.is_empty() {
        launch_now(app, targets);
    } else {
        with_state(|st| st.pending_open = Some(targets));
        export::start(app, missing);
    }
}

/// Nach einem Export: wartet ein „Öffnen“, wird es jetzt ausgeführt (nicht nach Abbruch/Fehlern).
pub fn after_export(app: &LabWindow, complete: bool) {
    if let Some(targets) = with_state(|st| st.pending_open.take()) {
        if complete {
            launch_now(app, targets);
        }
    }
}

fn launch_now(app: &LabWindow, targets: Vec<PathBuf>) {
    let (program, folder) = with_state(|st| {
        (chosen(&st.programs, &st.config.open_with), st.dir.clone().unwrap_or_default().join(uwcc_photo::SUBFOLDER))
    });
    let Some(program) = program else { return };
    let files: Vec<PathBuf> = targets
        .iter()
        .map(|f| uwcc_photo::output_path(f, crate::EXPORT_TARGET))
        .filter(|p| p.is_file())
        .collect();
    if files.is_empty() {
        app.invoke_notify("Keine exportierte Datei zum Öffnen gefunden.".into());
        return;
    }
    let n = files.len();
    match launch(&program, &files, &folder) {
        Ok(()) => app.invoke_notify(
            if n == 1 { format!("In {} geöffnet.", program.name) } else { format!("{n} Bilder an {} übergeben.", program.name) }
                .into(),
        ),
        Err(e) => app.invoke_notify(format!("{} ließ sich nicht starten: {e}", program.name).into()),
    }
}

/// Startet das Programm mit den Dateien (bzw. dem Ordner) und wartet nicht darauf.
fn launch(program: &Program, files: &[PathBuf], folder: &Path) -> Result<(), String> {
    let spawn = |cmd: &mut Command| cmd.spawn().map(|_| ()).map_err(|e| e.to_string());
    match program.kind {
        Kind::WindowsDefault => {
            for f in files {
                spawn(Command::new("explorer.exe").arg(f))?;
            }
            Ok(())
        }
        Kind::OpenWith => {
            let mut cmd = Command::new("rundll32.exe");
            // rundll32 erwartet „dll,Funktion Pfad“ als ein ungequotetes Argument.
            cmd.raw_arg(format!("shell32.dll,OpenAs_RunDLL {}", files[0].display())).creation_flags(CREATE_NO_WINDOW);
            spawn(&mut cmd)
        }
        Kind::Exe => {
            let exe = program.exe.as_ref().ok_or("nicht gefunden")?;
            let mut cmd = Command::new(exe);
            for arg in &program.args {
                match arg.as_str() {
                    "{files}" => {
                        cmd.args(files);
                    }
                    "{folder}" => {
                        cmd.arg(folder);
                    }
                    other => {
                        cmd.arg(other);
                    }
                }
            }
            spawn(&mut cmd)
        }
    }
}
