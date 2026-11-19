// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! EON Contacts -- the window.
//!
//! ```text
//! eon-contacts                    open the window
//! eon-contacts --selftest [FILE]  strings and interface checks
//! eon-contacts --version
//! ```

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eon_contacts::checks;
use eon_contacts::contacts::{Contacts, Outcome};
use eon_office_sdk::{Client, Incoming};
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tauri::{Emitter, Manager};

const INDEX: &str = include_str!("../../src/index.html");
const SCRIPT: &str = include_str!("../../src/app.js");

#[derive(Default)]
struct Shared {
    contacts: Mutex<Option<Arc<Contacts>>>,
}

impl Shared {
    fn contacts(&self) -> Result<Arc<Contacts>, Value> {
        self.contacts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| json!({ "code": "not_connected", "message": "not connected" }))
    }
}

type State<'a> = tauri::State<'a, Shared>;

/// Connect to the core as EON Contacts and start answering other applications.
fn connect(app: &tauri::AppHandle, shared: &Shared) -> Outcome {
    if let Ok(contacts) = shared.contacts() {
        return Ok(json!({ "granted": contacts.client.granted() }));
    }
    let client = Client::connect_app(eon_contacts::APP_ID).map_err(|e| {
        let code = if e.code() == "unauthenticated" {
            "not_installed"
        } else if e.code() == "connection" {
            "no_core"
        } else {
            e.code()
        };
        json!({ "code": code, "message": e.to_string() })
    })?;
    let granted = client.granted();
    let contacts = Arc::new(Contacts::new(client));
    *shared
        .contacts
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&contacts));
    let handle = app.clone();
    std::thread::spawn(move || {
        while let Some(message) = contacts.client.next(None) {
            match message {
                Incoming::Invoke {
                    id, method, params, ..
                } => {
                    if let Some(show) = contacts.answer(&id, &method, &params) {
                        if let Some(window) = handle.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                        let _ = handle.emit("show", show);
                    }
                }
                Incoming::Event { topic, .. } if topic == "core/prefs.changed" => {
                    let _ = handle.emit("prefs-changed", ());
                }
                Incoming::Shutdown => {
                    // The core asks Contacts to leave: the interface saves first.
                    let _ = handle.emit("save-and-quit", ());
                    std::thread::sleep(Duration::from_millis(1500));
                    handle.exit(0);
                    break;
                }
                _ => {}
            }
        }
    });
    Ok(json!({ "granted": granted }))
}

#[tauri::command(async)]
fn strings(language: String) -> Value {
    eon_contacts::strings(&language)
}

#[tauri::command(async)]
fn status(app: tauri::AppHandle, state: State<'_>) -> Outcome {
    connect(&app, &state)
}

#[tauri::command(async)]
fn prefs(state: State<'_>) -> Outcome {
    state
        .contacts()?
        .client
        .prefs()
        .map_err(|e| json!({ "code": e.code(), "message": e.to_string() }))
}

#[tauri::command(async)]
fn list(state: State<'_>, group: Option<String>) -> Outcome {
    state.contacts()?.list(group.as_deref())
}

#[tauri::command(async)]
fn get(state: State<'_>, id: String) -> Outcome {
    state.contacts()?.get(&id)
}

#[tauri::command(async)]
fn create(state: State<'_>, fields: Value) -> Outcome {
    state.contacts()?.create(&fields)
}

#[tauri::command(async)]
fn update(state: State<'_>, id: String, fields: Value, rev: u64) -> Outcome {
    state.contacts()?.update(&id, &fields, rev)
}

#[tauri::command(async)]
fn delete(state: State<'_>, id: String, rev: u64) -> Outcome {
    state.contacts()?.delete(&id, rev)
}

#[tauri::command(async)]
fn search(state: State<'_>, query: String) -> Outcome {
    state.contacts()?.lookup(&query)
}

#[tauri::command(async)]
fn export_vcf(state: State<'_>, path: String, group: Option<String>) -> Outcome {
    let out = state.contacts()?.export_vcf(group.as_deref())?;
    let path = path.trim().trim_matches('"');
    std::fs::write(path, out["vcf"].as_str().unwrap_or_default())
        .map_err(|e| json!({ "code": "io", "message": e.to_string() }))?;
    Ok(json!({ "path": path, "count": out["count"] }))
}

#[tauri::command(async)]
fn import_vcf(state: State<'_>, path: String) -> Outcome {
    let text = std::fs::read_to_string(path.trim().trim_matches('"'))
        .map_err(|e| json!({ "code": "unreadable", "message": e.to_string() }))?;
    state.contacts()?.import_vcf(&text)
}

fn main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    #[allow(unused_mut)]
    let mut builder =
        tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
            .title("EON Contacts")
            .inner_size(1100.0, 760.0)
            .min_inner_size(640.0, 480.0);
    // Debug builds only: a DevTools port for tools/ui-check.mjs.
    #[cfg(debug_assertions)]
    if let Ok(extra) = std::env::var("EON_WEBVIEW_ARGS") {
        builder = builder.additional_browser_args(&format!(
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection {extra}"
        ));
    }
    builder.build().map(|_| ())
}

fn window() {
    let app = tauri::Builder::default()
        .manage(Shared::default())
        .setup(|app| {
            main_window(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            strings, status, prefs, list, get, create, update, delete, search, export_vcf,
            import_vcf,
        ])
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|_, _| {}),
        Err(e) => {
            eprintln!("EON Contacts could not start: {e}");
            std::process::exit(1);
        }
    }
}

fn selftest() -> String {
    let mut report = format!("EON Contacts {} self-test\n", env!("CARGO_PKG_VERSION"));
    let mut failed = false;
    let mut section = |name: &str, problems: Vec<String>| {
        if problems.is_empty() {
            let _ = writeln!(report, "  ok    {name}");
        } else {
            failed = true;
            for problem in problems {
                let _ = writeln!(report, "  FAIL  {name}: {problem}");
            }
        }
    };
    section(
        "strings (en, tr)",
        checks::catalogue_problems(eon_contacts::EN, eon_contacts::TR),
    );
    section("interface", checks::ui_problems(INDEX, SCRIPT));
    let manifest: Value = serde_json::from_str(eon_contacts::MANIFEST).unwrap_or(Value::Null);
    let mut problems = Vec::new();
    if manifest["id"] != eon_contacts::APP_ID || manifest["version"] != env!("CARGO_PKG_VERSION") {
        problems.push("manifest.json's id or version does not match this build".to_owned());
    }
    section("manifest", problems);
    report.push_str(if failed { "FAILED\n" } else { "ok\n" });
    report
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        Some("--version" | "-V") => println!("EON Contacts {}", env!("CARGO_PKG_VERSION")),
        Some("--selftest") => {
            let report = selftest();
            match arguments.next() {
                Some(path) => {
                    if let Err(e) = std::fs::write(&path, &report) {
                        eprintln!("could not write {path}: {e}");
                        std::process::exit(2);
                    }
                }
                None => print!("{report}"),
            }
            if !report.ends_with("ok\n") {
                std::process::exit(1);
            }
        }
        _ => window(),
    }
}
