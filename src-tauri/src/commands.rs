use crate::cleaner;
use crate::model::*;
use crate::scanner;
use crate::settings::{self, Settings, SettingsInput};
use crate::trash_backend::NativeTrash;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

#[derive(Default)]
pub struct InnerState {
    pub active_scan: Option<(String, Arc<AtomicBool>)>,
    pub sessions: HashMap<String, ScanSession>,
}
pub struct AppState {
    pub inner: Mutex<InnerState>,
    pub cleaning: AtomicBool,
}
impl Default for AppState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(InnerState::default()),
            cleaning: AtomicBool::new(false),
        }
    }
}

fn home_dir() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "无法定位用户目录".into())
}
fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|p| p.join("settings.json"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<Settings, String> {
    let home = home_dir()?;
    Ok(settings::load(&settings_path(&app)?, &home))
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    input: SettingsInput,
) -> Result<Settings, String> {
    let inner = state.inner.lock().map_err(|_| "状态锁损坏")?;
    if state.cleaning.load(Ordering::Relaxed) || inner.active_scan.is_some() {
        return Err("扫描或清理期间不能修改设置".into());
    }
    let home = home_dir()?;
    let valid = settings::validate_roots(&home, input.project_roots)?;
    settings::save(&settings_path(&app)?, &valid)?;
    Ok(valid)
}

#[tauri::command]
pub fn begin_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    options: ScanOptions,
    on_event: Channel<ScanEvent>,
) -> Result<String, String> {
    let scan_id = Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let (home, configured, categories) = {
        let mut inner = state.inner.lock().map_err(|_| "状态锁损坏")?;
        if state.cleaning.load(Ordering::Relaxed) {
            return Err("清理正在进行".into());
        }
        if inner.active_scan.is_some() {
            return Err("已有扫描正在进行".into());
        }
        let categories = scanner::validate_categories(options.categories)?;
        let home = home_dir()?;
        let mut configured = settings::load(&settings_path(&app)?, &home);
        if let Some(roots) = options.project_roots {
            configured = settings::validate_roots(&home, roots)?;
        }
        inner.sessions.clear();
        inner.active_scan = Some((scan_id.clone(), cancel.clone()));
        (home, configured, categories)
    };
    let app_clone = app.clone();
    let id = scan_id.clone();
    tauri::async_runtime::spawn(async move {
        let channel = on_event.clone();
        let scan_id_for_work = id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            scanner::scan(
                scan_id_for_work,
                home,
                configured,
                categories,
                cancel,
                |event| {
                    let _ = channel.send(event);
                },
            )
        })
        .await;
        let state = app_clone.state::<AppState>();
        let mut inner = match state.inner.lock() {
            Ok(v) => v,
            Err(_) => return,
        };
        inner.active_scan = None;
        match result {
            Ok(Ok(session)) => {
                let report = session.report.clone();
                inner.sessions.insert(id.clone(), session);
                let _ = on_event.send(ScanEvent::Completed { report });
            }
            Ok(Err(e)) if e == "__cancelled__" => {
                let _ = on_event.send(ScanEvent::Cancelled { scan_id: id });
            }
            Ok(Err(e)) => {
                let _ = on_event.send(ScanEvent::Failed {
                    scan_id: id,
                    message: e,
                });
            }
            Err(e) => {
                let _ = on_event.send(ScanEvent::Failed {
                    scan_id: id,
                    message: e.to_string(),
                });
            }
        }
    });
    Ok(scan_id)
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>, scan_id: String) -> Result<(), String> {
    let inner = state.inner.lock().map_err(|_| "状态锁损坏")?;
    let Some((active, cancel)) = &inner.active_scan else {
        return Err("没有活动扫描".into());
    };
    if active != &scan_id {
        return Err("扫描会话不匹配".into());
    }
    cancel.store(true, Ordering::Relaxed);
    Ok(())
}

fn trash_size(path: &Path) -> SizeMeasurement {
    match scanner::directory_size_strict(path) {
        Ok(bytes) => SizeMeasurement {
            bytes: Some(bytes),
            error: None,
        },
        Err(error) => SizeMeasurement {
            bytes: None,
            error: Some(format!("无法读取系统废纸篓：{error}")),
        },
    }
}

#[tauri::command]
pub async fn clean_candidates(
    app: AppHandle,
    state: State<'_, AppState>,
    scan_id: String,
    candidate_ids: Vec<String>,
    on_event: Channel<CleanEvent>,
) -> Result<CleanReport, String> {
    if candidate_ids.is_empty() {
        return Err("请选择至少一个项目".into());
    }
    let (selected, scan_categories) = {
        let inner = state.inner.lock().map_err(|_| "状态锁损坏")?;
        if inner.active_scan.is_some() {
            return Err("扫描仍在进行".into());
        }
        if state.cleaning.swap(true, Ordering::SeqCst) {
            return Err("清理已在进行".into());
        }
        let session = match inner.sessions.get(&scan_id) {
            Some(session) => session,
            None => {
                state.cleaning.store(false, Ordering::SeqCst);
                return Err("扫描会话已失效，请重新扫描".into());
            }
        };
        let scan_categories = session.categories.clone();
        let result = candidate_ids
            .iter()
            .map(|id| {
                session
                    .candidates
                    .get(id)
                    .cloned()
                    .ok_or_else(|| format!("未知候选：{id}"))
            })
            .collect::<Result<Vec<_>, _>>();
        match result {
            Ok(items) => (items, scan_categories),
            Err(error) => {
                state.cleaning.store(false, Ordering::SeqCst);
                return Err(error);
            }
        }
    };
    let operation: Result<CleanReport, String> = async {
        let home = home_dir()?;
        let trash_path = home.join(".Trash");
        let settings = settings::load(&settings_path(&app)?, &home);
        let free_before = fs2::available_space(&home).unwrap_or(0);
        let trash_before = trash_size(&trash_path);
        let _ = on_event.send(CleanEvent::Started {
            total: selected.len(),
        });
        let channel = on_event.clone();
        let clean_result = tauri::async_runtime::spawn_blocking(move || {
            let outcome = cleaner::clean_batch(
                &NativeTrash,
                selected,
                &home,
                &trash_path,
                |id, success, message| {
                    let _ = channel.send(CleanEvent::ItemCompleted {
                        id: id.into(),
                        success,
                        message: message.into(),
                    });
                },
            );
            (home, settings, outcome)
        })
        .await
        .map_err(|e| e.to_string())?;
        let (home, settings, outcome) = clean_result;
        let _ = on_event.send(CleanEvent::Rescanning);
        let refreshed_id = Uuid::new_v4().to_string();
        let refreshed = tauri::async_runtime::spawn_blocking({
            let home = home.clone();
            move || {
                scanner::scan(
                    refreshed_id,
                    home,
                    settings,
                    scan_categories,
                    Arc::new(AtomicBool::new(false)),
                    |_| {},
                )
            }
        })
        .await
        .map_err(|e| e.to_string())??;
        let free_after = fs2::available_space(&home).unwrap_or(0);
        let trash_after = trash_size(&home.join(".Trash"));
        let report = refreshed.report.clone();
        {
            let mut inner = state.inner.lock().map_err(|_| "状态锁损坏")?;
            inner.sessions.remove(&scan_id);
            inner.sessions.insert(report.scan_id.clone(), refreshed);
        }
        let _ = on_event.send(CleanEvent::Completed);
        Ok(CleanReport {
            moved_count: outcome.moved_count,
            moved_bytes: outcome.moved_bytes,
            failed_bytes: outcome.failed_bytes,
            failures: outcome.failures,
            free_before,
            free_after,
            trash_before,
            trash_after,
            refreshed_scan: report,
        })
    }
    .await;
    state.cleaning.store(false, Ordering::SeqCst);
    operation
}
