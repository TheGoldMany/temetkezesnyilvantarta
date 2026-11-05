//! Development / E2E bridge. Exposes the same commands as the desktop shell
//! on http://127.0.0.1:<port>/rpc so the UI can run in a normal browser
//! (Vite dev server, Playwright). Binds to localhost only; never shipped.
//!
//! Usage: lunda-devserver [--port 1421] [--data DIR] [--static DIST] [--reset] [--sample] [--large]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lunda_core::app::App;
use lunda_core::{db, CoreError};
use serde_json::{json, Value};
use tiny_http::{Header, Method, Response, Server};

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn flag(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}

fn open(dir: &Path) -> App {
    std::fs::create_dir_all(dir).unwrap();
    let kf = dir.join(".dev-db-key");
    let key = match std::fs::read_to_string(&kf) {
        Ok(k) => k.trim().to_string(),
        Err(_) => {
            let k = db::generate_key();
            std::fs::write(&kf, &k).unwrap();
            k
        }
    };
    App::open(dir.to_path_buf(), Some(key), None)
}

fn handle(app: &Mutex<App>, dir: &Path, body: &str) -> Result<Value, CoreError> {
    let req: Value = serde_json::from_str(body)?;
    let cmd = req["cmd"].as_str().unwrap_or("");
    let a = &req["args"];
    let s = |k: &str| a[k].as_str().unwrap_or("").to_string();
    let mut app = app.lock().unwrap();
    Ok(match cmd {
        "app_status" => serde_json::to_value(app.status())?,
        "setup_admin" => serde_json::to_value(app.setup_admin(&s("username"), &s("displayName"), &s("password"))?)?,
        "login" => serde_json::to_value(app.login(&s("username"), &s("password"))?)?,
        "logout" => serde_json::to_value(app.logout())?,
        "touch" => {
            app.touch();
            Value::Null
        }
        "api" => app.call(&s("method"), a.get("args").unwrap_or(&Value::Null))?,
        "restore_fresh" => {
            let key = db::generate_key();
            app.restore_fresh(Path::new(&s("path")), &s("recoveryKey"), &key)?;
            std::fs::write(dir.join(".dev-db-key"), &key)?;
            serde_json::to_value(app.status())?
        }
        "backup_before_update" => Value::Null,
        _ => return Err(CoreError::new("unknown_command", cmd)),
    })
}

fn main() {
    let port: u16 = arg("--port").and_then(|p| p.parse().ok()).unwrap_or(1421);
    let dir = PathBuf::from(arg("--data").unwrap_or_else(|| "./.devdata".into()));
    if flag("--reset") {
        let _ = std::fs::remove_dir_all(&dir);
    }
    let static_dir = arg("--static").map(PathBuf::from);
    let app = Mutex::new(open(&dir));
    if flag("--sample") {
        let mut a = app.lock().unwrap();
        if a.store.as_ref().map(|s| s.user_count().unwrap_or(0)).unwrap_or(0) == 0 {
            a.setup_admin("admin", "Minta Adminisztrátor", "admin-jelszo").expect("admin");
            a.call("sample.load", &json!({})).expect("sample");
            a.call("settings.set", &json!({"values": {"onboarding.completed": "true", "operator.name": "Mintavölgy Község Önkormányzata"}})).expect("settings");
            a.logout();
        }
    }
    if flag("--large") {
        // performance check: 50 000 plots / 100 000 deceased in a separate cemetery
        let a = app.lock().unwrap();
        let t = std::time::Instant::now();
        lunda_core::sample::generate_large(a.store.as_ref().unwrap(), 50_000, 100_000).expect("large");
        eprintln!("large data set generated in {:?}", t.elapsed());
    }
    let server = Server::http(("127.0.0.1", port)).expect("bind");
    eprintln!("lunda-devserver on http://127.0.0.1:{port} (data: {})", dir.display());
    let json_h = Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap();
    for mut req in server.incoming_requests() {
        if *req.method() == Method::Post && req.url() == "/rpc" {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let (code, out) = match handle(&app, &dir, &body) {
                Ok(v) => (200, json!({ "ok": v })),
                Err(e) => (200, json!({ "err": e })),
            };
            let _ = req.respond(Response::from_string(out.to_string()).with_status_code(code).with_header(json_h.clone()));
            continue;
        }
        if let Some(sd) = &static_dir {
            let url = req.url().split('?').next().unwrap_or("/").trim_start_matches('/').to_string();
            let mut p = sd.join(if url.is_empty() { "index.html" } else { &url });
            if !p.starts_with(sd) || !p.is_file() {
                p = sd.join("index.html");
            }
            let ct = match p.extension().and_then(|e| e.to_str()) {
                Some("js") => "text/javascript",
                Some("css") => "text/css",
                Some("html") => "text/html; charset=utf-8",
                Some("svg") => "image/svg+xml",
                Some("woff2") => "font/woff2",
                Some("woff") => "font/woff",
                Some("png") => "image/png",
                Some("pdf") => "application/pdf",
                _ => "application/octet-stream",
            };
            match std::fs::read(&p) {
                Ok(b) => {
                    let _ = req.respond(Response::from_data(b).with_header(Header::from_bytes("Content-Type", ct).unwrap()));
                }
                Err(_) => {
                    let _ = req.respond(Response::from_string("not found").with_status_code(404));
                }
            }
        } else {
            let _ = req.respond(Response::from_string("not found").with_status_code(404));
        }
    }
}
