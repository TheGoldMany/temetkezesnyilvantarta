//! Vercel Function: every /api/* and /rpc request of TEMISZ.
//!
//! One warm instance keeps one database connection (Neon, sslmode=require)
//! and handles requests on a blocking thread; vercel.json routes the paths here.

use std::sync::{Arc, Mutex};

use http_body_util::BodyExt;
use lunda_core::sql::Connection;
use lunda_core::{db, Store};
use temisz_server::{handle, migrate_platform, Config, Request};
use vercel_runtime::{run, service_fn, Error, Response, ResponseBody};

type Shared = Arc<Mutex<Option<Store>>>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let shared: Shared = Arc::new(Mutex::new(None));
    run(service_fn(move |req| handler(req, shared.clone()))).await
}

fn connect() -> lunda_core::Result<Store> {
    let store = Store::new(Connection::connect(&db::database_url())?);
    migrate_platform(&store.conn)?;
    Ok(store)
}

async fn handler(req: vercel_runtime::Request, shared: Shared) -> Result<Response<ResponseBody>, Error> {
    let (parts, body) = req.into_parts();
    let body = body.collect().await?.to_bytes().to_vec();
    let headers: Vec<(String, String)> = parts.headers.iter().map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string())).collect();
    let ip = parts.headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()).and_then(|v| v.split(',').next()).unwrap_or("").trim().to_string();
    let request = Request {
        method: parts.method.as_str().to_string(),
        path: parts.uri.path_and_query().map(|p| p.as_str()).unwrap_or("/").to_string(),
        headers,
        body,
        ip,
    };
    let resp = tokio::task::spawn_blocking(move || {
        let cfg = Config { secure_cookies: true, static_dir: None, dev_mode: false };
        let mut guard = shared.lock().unwrap_or_else(|p| p.into_inner());
        let alive = guard.as_ref().map(|s| s.conn.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)).is_ok()).unwrap_or(false);
        if !alive {
            *guard = match connect() {
                Ok(s) => Some(s),
                Err(e) => return temisz_server::Response::text(503, &format!("database unavailable: {}", e.code)),
            };
        }
        handle(&cfg, guard.as_ref().unwrap(), &request)
    })
    .await?;
    let mut builder = Response::builder().status(resp.status);
    for (k, v) in &resp.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    Ok(builder.body(ResponseBody::from(resp.body))?)
}
