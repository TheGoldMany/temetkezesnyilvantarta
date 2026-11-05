//! Standalone TEMISZ server (development, E2E tests, a self-hosted VPS).
//!
//! Usage: temisz-server [--port 1430] [--bind 127.0.0.1] [--static DIST] [--workers 4]
//!                      [--sample-account | --reset-sample-account] [--insecure-cookies]
//! Database: DATABASE_URL (default postgres://postgres@127.0.0.1:5433/postgres).

use std::sync::Arc;

use lunda_core::sql::Connection;
use lunda_core::{db, Store};
use temisz_server::{handle, migrate_platform, Config, Request};
use tiny_http::{Header, Response, Server};

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn flag(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}

fn connect() -> Store {
    Store::new(Connection::connect(&db::database_url()).expect("database connection"))
}

fn main() {
    let port: u16 = arg("--port").and_then(|p| p.parse().ok()).unwrap_or(1430);
    let bind = arg("--bind").unwrap_or_else(|| "127.0.0.1".into());
    let workers: usize = arg("--workers").and_then(|w| w.parse().ok()).unwrap_or(4);
    let cfg = Arc::new(Config { secure_cookies: !flag("--insecure-cookies"), static_dir: arg("--static").map(Into::into) });

    let setup = connect();
    migrate_platform(&setup.conn).expect("platform migration");
    if flag("--reset-sample-account") {
        temisz_server::drop_sample_account(&setup).expect("drop sample account");
    }
    if flag("--sample-account") || flag("--reset-sample-account") {
        temisz_server::ensure_sample_account(&setup).expect("sample account");
    }
    drop(setup);

    let server = Arc::new(Server::http((bind.as_str(), port)).expect("bind"));
    eprintln!("temisz-server on http://{bind}:{port}");
    let mut handles = vec![];
    for _ in 0..workers {
        let (server, cfg) = (server.clone(), cfg.clone());
        handles.push(std::thread::spawn(move || {
            let mut store = connect();
            for mut rq in server.incoming_requests() {
                let mut body = Vec::new();
                let _ = rq.as_reader().read_to_end(&mut body);
                let req = Request {
                    method: rq.method().as_str().to_string(),
                    path: rq.url().to_string(),
                    headers: rq.headers().iter().map(|h| (h.field.as_str().as_str().to_string(), h.value.as_str().to_string())).collect(),
                    body,
                    ip: rq.remote_addr().map(|a| a.ip().to_string()).unwrap_or_default(),
                };
                // reconnect after a lost connection
                if store.conn.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)).is_err() {
                    store = connect();
                }
                let resp = handle(&cfg, &store, &req);
                let mut out = Response::from_data(resp.body).with_status_code(resp.status);
                for (k, v) in resp.headers {
                    if let Ok(h) = Header::from_bytes(k.as_bytes(), v.as_bytes()) {
                        out.add_header(h);
                    }
                }
                let _ = rq.respond(out);
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }
}
