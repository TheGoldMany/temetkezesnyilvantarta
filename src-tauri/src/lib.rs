//! Desktop shell: opens the encrypted database with the key from the OS
//! keychain and forwards UI calls to `lunda_core::app::App`.

mod keystore;
mod updater;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lunda_core::app::{App, AppStatus};
use lunda_core::{db, CoreError};
use serde_json::Value;
use tauri::{Manager, RunEvent, State};

pub struct AppState(pub Arc<Mutex<App>>);

fn open_app(data_dir: PathBuf) -> App {
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        return App::open(data_dir, None, Some(e.into()));
    }
    let db_exists = data_dir.join(lunda_core::app::DB_FILE).exists();
    match keystore::get_key(&data_dir) {
        Ok(Some(k)) => App::open(data_dir, Some(k), None),
        Ok(None) if !db_exists => {
            let k = db::generate_key();
            match keystore::set_key(&data_dir, &k) {
                Ok(()) => App::open(data_dir, Some(k), None),
                Err(e) => App::open(data_dir, None, Some(e)),
            }
        }
        Ok(None) => App::open(data_dir, None, None), // -> db_key_missing
        Err(e) => App::open(data_dir, None, Some(e)),
    }
}

#[tauri::command]
fn app_status(state: State<AppState>) -> AppStatus {
    state.0.lock().unwrap().status()
}

#[tauri::command]
fn setup_admin(state: State<AppState>, username: String, display_name: String, password: String) -> Result<AppStatus, CoreError> {
    state.0.lock().unwrap().setup_admin(&username, &display_name, &password)
}

#[tauri::command]
fn login(state: State<AppState>, username: String, password: String) -> Result<AppStatus, CoreError> {
    state.0.lock().unwrap().login(&username, &password)
}

#[tauri::command]
fn logout(state: State<AppState>) -> AppStatus {
    state.0.lock().unwrap().logout()
}

#[tauri::command]
fn touch(state: State<AppState>) {
    state.0.lock().unwrap().touch()
}

/// The single entry point for UI operations.
#[tauri::command]
fn api(state: State<AppState>, method: String, args: Option<Value>) -> Result<Value, CoreError> {
    state.0.lock().unwrap().call(&method, &args.unwrap_or(Value::Null))
}

/// Restore before login (new computer, lost keychain): a new database key is generated.
#[tauri::command]
fn restore_fresh(state: State<AppState>, path: String, recovery_key: String) -> Result<AppStatus, CoreError> {
    let mut app = state.0.lock().unwrap();
    let key = db::generate_key();
    let data_dir = app.data_dir.clone();
    app.restore_fresh(std::path::Path::new(&path), &recovery_key, &key)?;
    keystore::set_key(&data_dir, &key)?;
    Ok(app.status())
}

#[tauri::command]
fn backup_before_update(state: State<AppState>) -> Result<(), CoreError> {
    let app = state.0.lock().unwrap();
    if let Some(store) = &app.store {
        lunda_core::backup::create(&store.conn, &lunda_core::auth::Session::system(), &app.data_dir, "pre-update")?;
    }
    Ok(())
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let data_dir = match std::env::var("LUNDA_DATA_DIR") {
                Ok(d) => PathBuf::from(d),
                Err(_) => app.path().app_data_dir()?,
            };
            let shared = Arc::new(Mutex::new(open_app(data_dir)));
            // FR-NAP-03: daily automatic backup – checked at start and every hour
            let bg = shared.clone();
            std::thread::spawn(move || loop {
                if let Ok(mut a) = bg.lock() {
                    if let Err(e) = a.backup_if_due() {
                        log::warn!("automatic backup failed: {e}");
                    }
                }
                std::thread::sleep(Duration::from_secs(3600));
            });
            app.manage(AppState(shared));
            app.manage(updater::PendingUpdate::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_status,
            setup_admin,
            login,
            logout,
            api,
            touch,
            restore_fresh,
            backup_before_update,
            updater::update_check,
            updater::update_install
        ])
        .build(tauri::generate_context!())
        .expect("error while building TEMISZ");
    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            // FR-NAP-03: backup on exit
            if let Some(state) = handle.try_state::<AppState>() {
                if let Ok(mut a) = state.0.lock() {
                    let _ = a.backup_on_exit();
                }
            }
        }
    });
}
