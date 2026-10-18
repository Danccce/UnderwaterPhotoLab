//! Lädt die gebauten Plug-ins mit wasmi, wie PhotoCraft es tut.
//! Die Dateien müssen vorher mit `cargo plugin` gebaut werden.

use std::path::PathBuf;

use uwcc_core::{Correction, Levels, Transform};
use wasmi::{Engine, Instance, Linker, Module, Store};

type FilterArgs = (i32, i32, i32, i32, i32, i32, i32, i32);

const RGB8_ALPHA: i32 = 8 | 1 << 8 | 3 << 16;
const EXPERT: &str = "underwater_color_correction";
const RED: &str = "underwater_red_restore";

fn wasm_path(name: &str) -> PathBuf {
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    target.join(format!("wasm32-unknown-unknown/release/{name}.wasm"))
}

fn load(name: &str) -> (Engine, Module) {
    let path = wasm_path(name);
    let wasm = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (erst `cargo plugin` ausführen)", path.display()));
    let engine = Engine::default();
    let module = Module::new(&engine, &wasm[..]).unwrap();
    (engine, module)
}

fn instantiate(engine: &Engine, module: &Module) -> (Store<()>, Instance) {
    let mut store = Store::new(engine, ());
    let instance = Linker::<()>::new(engine)
        .instantiate_and_start(&mut store, module)
        .unwrap();
    (store, instance)
}

fn manifest(name: &str) -> serde_json::Value {
    let (engine, module) = load(name);
    let (mut store, inst) = instantiate(&engine, &module);
    let m = inst
        .get_typed_func::<(), i64>(&store, "pc_manifest")
        .unwrap()
        .call(&mut store, ())
        .unwrap() as u64;
    let mut buf = vec![0u8; (m >> 32) as usize];
    inst.get_memory(&store, "memory")
        .unwrap()
        .read(&store, (m & 0xffff_ffff) as usize, &mut buf)
        .unwrap();
    serde_json::from_slice(&buf).unwrap()
}

fn run_filter(name: &str, px: &[f32], channels: i32, format: i32, params: &str) -> (i32, Vec<f32>) {
    let (engine, module) = load(name);
    let (mut store, inst) = instantiate(&engine, &module);
    let mem = inst.get_memory(&store, "memory").unwrap();
    let alloc = inst.get_typed_func::<i32, i32>(&store, "pc_alloc").unwrap();
    let filter = inst.get_typed_func::<FilterArgs, i32>(&store, "pc_filter").unwrap();

    let bytes: Vec<u8> = px.iter().flat_map(|v| v.to_le_bytes()).collect();
    let buf = alloc.call(&mut store, bytes.len() as i32).unwrap();
    mem.write(&mut store, buf as usize, &bytes).unwrap();
    let pp = alloc.call(&mut store, params.len() as i32).unwrap();
    mem.write(&mut store, pp as usize, params.as_bytes()).unwrap();

    let width = px.len() as i32 / channels;
    let rc = filter
        .call(
            &mut store,
            (buf, bytes.len() as i32, width, 1, channels, format, pp, params.len() as i32),
        )
        .unwrap();
    let mut out = vec![0u8; bytes.len()];
    mem.read(&store, buf as usize, &mut out).unwrap();
    let out = out
        .chunks(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    (rc, out)
}

fn sample_pixels() -> Vec<f32> {
    (0..64)
        .flat_map(|i| {
            let t = i as f32 / 63.0;
            [0.05 + 0.2 * t, 0.3 + 0.4 * t, 0.4 + 0.5 * (1.0 - t), 1.0 - 0.5 * t]
        })
        .collect()
}

/// Erwartetes Ergebnis: `t` angewendet, auf 0–1 begrenzt, mit `strength` überblendet; nur die
/// Kanäle in `channels` ändern sich.
fn reference(px: &[f32], t: &Transform, strength: f32, channels: &[usize]) -> Vec<f32> {
    px.chunks(4)
        .flat_map(|p| {
            let (r, g, b) = t.apply(p[0], p[1], p[2]);
            let mut out = [p[0], p[1], p[2], p[3]];
            for &i in channels {
                let n = [r, g, b][i];
                out[i] = p[i] + (n.clamp(0.0, 1.0) - p[i]) * strength;
            }
            out
        })
        .collect()
}

fn assert_close(a: &[f32], b: &[f32]) {
    assert_eq!(a.len(), b.len());
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        assert!((x - y).abs() < 1e-6, "Sample {i}: {x} != {y}");
    }
}

#[test]
fn module_shape() {
    for name in [EXPERT, RED] {
        let (engine, module) = load(name);
        assert_eq!(module.imports().count(), 0, "{name}: PhotoCraft lehnt Imports ab");
        let mut exports: Vec<_> = module.exports().map(|e| e.name().to_owned()).collect();
        exports.sort();
        assert_eq!(
            exports,
            ["memory", "pc_abi_version", "pc_alloc", "pc_filter", "pc_manifest"],
            "{name}"
        );

        let (mut store, inst) = instantiate(&engine, &module);
        let abi = inst
            .get_typed_func::<(), i32>(&store, "pc_abi_version")
            .unwrap()
            .call(&mut store, ())
            .unwrap();
        assert_eq!(abi, 1, "{name}");
    }
}

#[test]
fn manifests_name_original_author() {
    for (name, id) in [
        (EXPERT, "local.underwater-color-correction"),
        (RED, "local.underwater-red-restore"),
    ] {
        let m = manifest(name);
        assert_eq!(m["id"], id);
        assert_eq!(m["kind"], "filter");
        assert!(m["author"].as_str().unwrap().contains("Nikolaj Bech Andersen"), "{name}");
        assert!(m["description"].as_str().unwrap().contains("Nikolaj Bech Andersen"), "{name}");
        assert!(m["name"].as_str().unwrap().chars().count() <= 64, "{name}");
    }
}

#[test]
fn expert_defaults_leave_pixels_unchanged() {
    let px = sample_pixels();
    let (rc, out) = run_filter(EXPERT, &px, 4, RGB8_ALPHA, "{}");
    assert_eq!(rc, 0);
    assert_close(&out, &px);
}

#[test]
fn expert_matches_core_transform() {
    let c = Correction {
        hue_shift: 47.0,
        red: Levels { low: 12.0, high: 190.0 },
        green: Levels { low: 30.0, high: 220.0 },
        blue: Levels { low: 41.0, high: 245.0 },
    };
    let params = r#"{"hueShift": 47, "redLow": 12, "redHigh": 190, "greenLow": 30, "greenHigh": 220,
        "blueLow": 41, "blueHigh": 245, "strength": 50,
        "_image": {"x": 0, "y": 0, "width": 64, "height": 1, "overlap": 0, "mode": "rgb", "depth": 8, "alpha": true, "channels": 4}}"#;

    let px = sample_pixels();
    let expected = reference(&px, &Transform::new(&c), 0.5, &[0, 1, 2]);
    let (rc, out) = run_filter(EXPERT, &px, 4, RGB8_ALPHA, params);
    assert_eq!(rc, 0);
    assert_close(&out, &expected);
}

#[test]
fn red_default_matches_manifest_and_core() {
    let default = manifest(RED)["params"]["hueShift"]["default"].as_f64().unwrap();
    let t = Transform::new(&Correction { hue_shift: default, ..Correction::NONE });

    let px = sample_pixels();
    let expected = reference(&px, &t, 1.0, &[0]);
    let (rc, out) = run_filter(RED, &px, 4, RGB8_ALPHA, "{}");
    assert_eq!(rc, 0);
    assert_close(&out, &expected);
    assert_ne!(out[0], px[0], "Rot muss sich ändern");
}

#[test]
fn red_strength_and_zero_shift() {
    let px = sample_pixels();
    let t = Transform::new(&Correction { hue_shift: 100.0, ..Correction::NONE });
    let (rc, out) = run_filter(RED, &px, 4, RGB8_ALPHA, r#"{"hueShift": 100, "strength": 25}"#);
    assert_eq!(rc, 0);
    assert_close(&out, &reference(&px, &t, 0.25, &[0]));

    let (rc, out) = run_filter(RED, &px, 4, RGB8_ALPHA, r#"{"hueShift": 0}"#);
    assert_eq!(rc, 0);
    assert_close(&out, &px);
}

#[test]
fn rejects_non_rgb() {
    let gray = [0.2f32, 1.0, 0.5, 1.0];
    for name in [EXPERT, RED] {
        let (rc, out) = run_filter(name, &gray, 2, 8 | 1 << 8 | 1 << 16, "{}");
        assert_eq!(rc, 2, "{name}");
        assert_close(&out, &gray);
    }
}
