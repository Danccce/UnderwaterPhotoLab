//! Ermittelt die Parameter der Unterwasser-Farbkorrektur für ein ganzes Bild.
//!
//! Das PhotoCraft-Plug-in bekommt das Bild nur streifenweise und kann diese Analyse daher nicht
//! selbst durchführen.

use std::{env, fs, process::ExitCode};

use uwcc_core::{analyze, Correction};

const PLUGIN_ID: &str = "local.underwater-color-correction";

fn usage() -> ExitCode {
    eprintln!("Aufruf: uwcc-analyze <bild> [--json] [--dump-rgba <datei>]");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut path = None;
    let mut json = false;
    let mut dump = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--dump-rgba" => match args.next() {
                Some(p) => dump = Some(p),
                None => return usage(),
            },
            _ if path.is_none() && !arg.starts_with("--") => path = Some(arg),
            _ => return usage(),
        }
    }
    let Some(path) = path else {
        return usage();
    };

    let img = match image::open(&path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (width, height) = img.dimensions();
    if let Some(dump) = dump {
        if let Err(e) = fs::write(&dump, img.as_raw()) {
            eprintln!("{dump}: {e}");
            return ExitCode::FAILURE;
        }
    }

    let c = analyze(img.as_raw());
    let params = params_json(&c);
    if json {
        println!(
            r#"{{"width":{width},"height":{height},"params":{params},"matrix":{}}}"#,
            matrix_json(&c.matrix())
        );
        return ExitCode::SUCCESS;
    }

    println!("{path} ({width} × {height} px)");
    println!();
    println!("Werte für Filter › Plug-ins › Unterwasser-Farbkorrektur (Experte)…:");
    println!("  hueShift   {}", c.hue_shift);
    println!("  redLow     {:<6} redHigh    {}", c.red.low, c.red.high);
    println!("  greenLow   {:<6} greenHigh  {}", c.green.low, c.green.high);
    println!("  blueLow    {:<6} blueHigh   {}", c.blue.low, c.blue.high);
    println!("  strength   100");
    println!();
    println!("plugin.run:");
    println!(r#"{{"id":"{PLUGIN_ID}","params":{params}}}"#);
    ExitCode::SUCCESS
}

fn params_json(c: &Correction) -> String {
    format!(
        r#"{{"hueShift":{},"redLow":{},"redHigh":{},"greenLow":{},"greenHigh":{},"blueLow":{},"blueHigh":{},"strength":100}}"#,
        c.hue_shift, c.red.low, c.red.high, c.green.low, c.green.high, c.blue.low, c.blue.high
    )
}

fn matrix_json(m: &[f64; 20]) -> String {
    let values: Vec<String> = m.iter().map(f64::to_string).collect();
    format!("[{}]", values.join(","))
}
