//! Client für PhotoCrafts Steuerschnittstelle (`docs/control-protocol.md`): JSON-Zeilen über
//! TCP auf 127.0.0.1, die erste Anfrage jeder Verbindung ist `auth` mit dem Token.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    time::Duration,
};

use serde_json::{json, Value};

pub const EXPERT_PLUGIN_ID: &str = "local.underwater-color-correction";

pub struct Connection {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    next_id: u64,
}

impl Connection {
    pub fn open(port: u16, token_file: &Path) -> Result<Self, String> {
        let token = fs::read_to_string(token_file).map_err(|e| {
            format!(
                "Token-Datei {} nicht lesbar ({e}). Wurde PhotoCraft mit „PhotoCraft mit Steuerung starten“ geöffnet?",
                token_file.display()
            )
        })?;
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| {
            format!(
                "PhotoCraft ist auf Port {port} nicht erreichbar ({e}). Wurde PhotoCraft mit „PhotoCraft mit Steuerung starten“ geöffnet?"
            )
        })?;
        // PhotoCraft wartet selbst bis zu 60 s auf die Antwort eines Befehls.
        stream
            .set_read_timeout(Some(Duration::from_secs(75)))
            .map_err(|e| e.to_string())?;
        let writer = stream.try_clone().map_err(|e| e.to_string())?;
        let mut conn = Self { writer, reader: BufReader::new(stream), next_id: 1 };
        conn.request(json!({"id": "auth", "method": "auth", "params": {"token": token.trim()}}))
            .map_err(|e| format!("Anmeldung bei PhotoCraft fehlgeschlagen: {e}"))?;
        Ok(conn)
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.request(json!({"id": id, "method": method, "params": params}))
    }

    pub fn execute(&mut self, command: &str, params: Value) -> Result<Value, String> {
        self.call("engine.execute", json!({"command": command, "params": params}))
    }

    fn request(&mut self, req: Value) -> Result<Value, String> {
        writeln!(self.writer, "{req}").map_err(|e| format!("Senden an PhotoCraft fehlgeschlagen: {e}"))?;
        loop {
            let mut line = String::new();
            let n = self
                .reader
                .read_line(&mut line)
                .map_err(|e| format!("keine Antwort von PhotoCraft ({e})"))?;
            if n == 0 {
                return Err("PhotoCraft hat die Verbindung geschlossen".into());
            }
            let Ok(reply) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if reply.get("id").is_some_and(|id| *id != req["id"]) {
                continue;
            }
            return if reply["ok"] == true {
                Ok(reply["result"].clone())
            } else {
                Err(reply["error"].as_str().unwrap_or("unbekannter Fehler").to_owned())
            };
        }
    }
}

/// Führt das Experten-Plug-in mit `params` auf dem aktiven Dokument aus.
pub fn run_expert_plugin(port: u16, token_file: &Path, params: Value) -> Result<(), String> {
    let mut conn = Connection::open(port, token_file)?;
    conn.execute("plugin.run", json!({"id": EXPERT_PLUGIN_ID, "params": params}))
        .map_err(|e| {
            if e.contains("no plug-in") {
                format!("{e} – das Experten-Plug-in ist in PhotoCraft nicht installiert.")
            } else {
                e
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Gegen einen echten `photocraft-cli serve`. Aufruf:
    //! `PHOTOCRAFT_CLI=… UWCC_EXPERT_WASM=… cargo test -p uwcc-gui -- --ignored`

    use std::process::{Child, Command, Stdio};

    use super::*;

    struct Server(Child);

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.0.kill();
        }
    }

    #[test]
    #[ignore = "braucht photocraft-cli (PHOTOCRAFT_CLI) und das gebaute Plug-in (UWCC_EXPERT_WASM)"]
    fn applies_plugin_through_headless_photocraft() {
        let cli = std::env::var("PHOTOCRAFT_CLI").unwrap();
        let wasm = std::env::var("UWCC_EXPERT_WASM").unwrap();
        let dir = std::env::temp_dir().join("uwcc-control-test");
        fs::create_dir_all(&dir).unwrap();
        let token = dir.join("token");
        let _ = fs::remove_file(&token);
        let port = 47_000 + (std::process::id() % 1000) as u16;

        let _server = Server(
            Command::new(cli)
                .args(["serve", "--port", &port.to_string(), "--control-token-file"])
                .arg(&token)
                .stdin(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let mut conn = None;
        for _ in 0..100 {
            if let Ok(c) = Connection::open(port, &token) {
                conn = Some(c);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let mut conn = conn.expect("photocraft-cli serve antwortet nicht");

        conn.execute("file.new", json!({"width": 4, "height": 4, "background": "white"})).unwrap();
        // Pfade lehnt die Steuerschnittstelle ab; der Inhalt geht Base64-kodiert.
        use base64::Engine as _;
        let data = base64::engine::general_purpose::STANDARD.encode(fs::read(wasm).unwrap());
        conn.execute("plugin.install", json!({"data": data})).unwrap();
        let pixel = |conn: &mut Connection| conn.execute("document.pixel", json!({"x": 1, "y": 1})).unwrap();
        let before = pixel(&mut conn);
        drop(conn);

        // Bei 90° wird Weiß im Rotkanal zu 0,299 + 0,168 + 0,587 + 0,330 − 1,2 · 0,383 ≈ 0,92.
        let params = json!({"hueShift": 90, "redLow": 0, "redHigh": 256, "greenLow": 0,
            "greenHigh": 256, "blueLow": 0, "blueHigh": 256, "strength": 100});
        run_expert_plugin(port, &token, params).unwrap();

        let after = pixel(&mut Connection::open(port, &token).unwrap());
        assert_ne!(before, after, "Hue-Shift 90° muss den Rotkanal von Weiß verändern");
        println!("vorher {before}, nachher {after}");
    }
}
