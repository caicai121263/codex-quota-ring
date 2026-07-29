mod app_server;
mod quota;

use crate::quota::QuotaStatus;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};
use sysinfo::{get_current_pid, ProcessesToUpdate, System};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;

const BASE_WIDTH: f64 = 450.0;
const COLLAPSED_HEIGHT: f64 = 160.0;
const SETTINGS_HEIGHT: f64 = 520.0;
const SUCCESS_CACHE_MS: i64 = 5_000;
const DOCK_SNAP_DISTANCE: f64 = 12.0;
const DOCK_RELEASE_DISTANCE: f64 = 32.0;
const DOCK_MOVE_DEBOUNCE_MS: u64 = 250;
const DOCK_ANIMATION_MS: u64 = 180;
const DOCK_ANIMATION_STEPS: u64 = 12;
const DOCK_TOP_WIDTH: f64 = 108.0;
const DOCK_TOP_HEIGHT: f64 = 36.0;
const DOCK_SIDE_WIDTH: f64 = 42.0;
const DOCK_SIDE_HEIGHT: f64 = 112.0;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum PrimaryQuotaWindow {
    #[default]
    FiveHour,
    Weekly,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum DockEdge {
    Left,
    Right,
    Top,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DockState {
    docked: bool,
    expanded: bool,
    edge: Option<DockEdge>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
struct Preferences {
    refresh_interval_secs: u64,
    visible: bool,
    always_on_top: bool,
    autostart: bool,
    window_x: Option<i32>,
    window_y: Option<i32>,
    window_width: Option<u32>,
    window_height: Option<u32>,
    primary_quota_window: PrimaryQuotaWindow,
    ui_scale: f64,
    show_credits: bool,
    auto_show_on_codex: bool,
    auto_hide_on_codex_close: bool,
    start_hidden_on_autostart: bool,
    lock_window_position: bool,
    edge_dock_enabled: bool,
    dock_edge: Option<DockEdge>,
    dock_monitor_id: Option<String>,
    dock_offset: Option<f64>,
    dock_auto_collapse_delay_ms: u64,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            refresh_interval_secs: 300,
            visible: true,
            always_on_top: true,
            autostart: false,
            window_x: None,
            window_y: None,
            window_width: None,
            window_height: None,
            primary_quota_window: PrimaryQuotaWindow::FiveHour,
            ui_scale: 1.0,
            show_credits: true,
            auto_show_on_codex: false,
            auto_hide_on_codex_close: false,
            start_hidden_on_autostart: false,
            lock_window_position: false,
            edge_dock_enabled: false,
            dock_edge: None,
            dock_monitor_id: None,
            dock_offset: None,
            dock_auto_collapse_delay_ms: 800,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreferencesPatch {
    refresh_interval_secs: Option<u64>,
    always_on_top: Option<bool>,
    autostart: Option<bool>,
    primary_quota_window: Option<PrimaryQuotaWindow>,
    ui_scale: Option<f64>,
    show_credits: Option<bool>,
    auto_show_on_codex: Option<bool>,
    auto_hide_on_codex_close: Option<bool>,
    start_hidden_on_autostart: Option<bool>,
    lock_window_position: Option<bool>,
    edge_dock_enabled: Option<bool>,
    dock_auto_collapse_delay_ms: Option<u64>,
}

impl PreferencesPatch {
    fn apply(self, target: &mut Preferences) {
        if let Some(value) = self.refresh_interval_secs {
            target.refresh_interval_secs = value.clamp(60, 900);
        }
        if let Some(value) = self.always_on_top {
            target.always_on_top = value;
        }
        if let Some(value) = self.autostart {
            target.autostart = value;
        }
        if let Some(value) = self.primary_quota_window {
            target.primary_quota_window = value;
        }
        if let Some(value) = self.ui_scale {
            target.ui_scale = normalize_scale(value);
        }
        if let Some(value) = self.show_credits {
            target.show_credits = value;
        }
        if let Some(value) = self.auto_show_on_codex {
            target.auto_show_on_codex = value;
        }
        if let Some(value) = self.auto_hide_on_codex_close {
            target.auto_hide_on_codex_close = value;
        }
        if let Some(value) = self.start_hidden_on_autostart {
            target.start_hidden_on_autostart = value;
        }
        if let Some(value) = self.lock_window_position {
            target.lock_window_position = value;
        }
        if let Some(value) = self.edge_dock_enabled {
            target.edge_dock_enabled = value;
            if !value {
                target.dock_edge = None;
                target.dock_monitor_id = None;
                target.dock_offset = None;
            }
        }
        if let Some(value) = self.dock_auto_collapse_delay_ms {
            target.dock_auto_collapse_delay_ms = value.clamp(300, 3_000);
        }
    }
}

#[derive(Clone, Debug, Default)]
struct WindowGeometry {
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
}

#[derive(Clone, Debug, Default)]
struct DiagnosticsState {
    codex_found: bool,
    candidate_source: Option<String>,
    last_error_code: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticInfo {
    app_version: String,
    windows_version: String,
    codex_found: bool,
    candidate_source: Option<String>,
    last_success_at: Option<i64>,
    last_error_code: Option<String>,
}

struct AppState {
    status: Mutex<QuotaStatus>,
    preferences: Mutex<Preferences>,
    settings_open: AtomicBool,
    settings_geometry: Mutex<Option<WindowGeometry>>,
    refresh_gate: tokio::sync::Mutex<()>,
    refresh_generation: AtomicU64,
    position_generation: AtomicU64,
    process_running: AtomicBool,
    manual_hidden_cycle: AtomicBool,
    diagnostics: Mutex<DiagnosticsState>,
    dock_expanded: AtomicBool,
    dock_animating: AtomicBool,
    dock_motion_generation: AtomicU64,
    dock_animation_generation: AtomicU64,
    dock_ignore_until_ms: AtomicU64,
}

#[tauri::command]
fn get_quota_status(state: tauri::State<'_, AppState>) -> QuotaStatus {
    state.status.lock().unwrap().clone()
}

#[tauri::command]
fn get_preferences(state: tauri::State<'_, AppState>) -> Preferences {
    state.preferences.lock().unwrap().clone()
}

#[tauri::command]
fn get_dock_state(state: tauri::State<'_, AppState>) -> DockState {
    dock_state(&state)
}

#[tauri::command]
async fn set_dock_expanded(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    expanded: bool,
) -> Result<DockState, String> {
    if !expanded && state.settings_open.load(Ordering::SeqCst) {
        return Ok(dock_state(&state));
    }
    set_dock_expanded_internal(&app, expanded, true).await?;
    Ok(dock_state(&state))
}

#[tauri::command]
fn get_diagnostics(app: AppHandle, state: tauri::State<'_, AppState>) -> DiagnosticInfo {
    let diagnostics = state.diagnostics.lock().unwrap().clone();
    let last_success_at = state.status.lock().unwrap().last_success_at;
    DiagnosticInfo {
        app_version: app.package_info().version.to_string(),
        windows_version: System::long_os_version().unwrap_or_else(|| "Windows（版本未知）".into()),
        codex_found: diagnostics.codex_found,
        candidate_source: diagnostics.candidate_source,
        last_success_at,
        last_error_code: diagnostics.last_error_code,
    }
}

#[tauri::command]
async fn refresh_quota(app: AppHandle, force: Option<bool>) -> QuotaStatus {
    refresh_into_state(&app, force.unwrap_or(true)).await
}

#[tauri::command]
fn set_settings_open(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    open: bool,
) -> Result<String, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "未找到主窗口。".to_string())?;
    if state.settings_open.swap(open, Ordering::SeqCst) == open {
        return Ok("unchanged".into());
    }
    suppress_dock_detection(&state, 600);

    if open {
        if state.preferences.lock().unwrap().dock_edge.is_some()
            && !state.dock_expanded.load(Ordering::SeqCst)
        {
            apply_dock_geometry_immediate(&app, true)?;
        }
        let geometry = WindowGeometry {
            position: window.outer_position().map_err(|_| "无法读取窗口位置。")?,
            size: window.outer_size().map_err(|_| "无法读取窗口尺寸。")?,
        };
        *state.settings_geometry.lock().unwrap() = Some(geometry.clone());
        let scale_factor = window.scale_factor().unwrap_or(1.0);
        let ui_scale = state.preferences.lock().unwrap().ui_scale;
        let target_height = (SETTINGS_HEIGHT * ui_scale * scale_factor).round() as u32;
        let target_size = PhysicalSize::new(geometry.size.width, target_height);
        let (target_position, direction) =
            settings_position(&window, geometry.position, geometry.size, target_size);
        window
            .set_size(target_size)
            .map_err(|_| "无法展开设置面板。")?;
        window
            .set_position(target_position)
            .map_err(|_| "无法调整设置面板位置。")?;
        Ok(direction.into())
    } else {
        if let Some(geometry) = state.settings_geometry.lock().unwrap().take() {
            window
                .set_size(geometry.size)
                .map_err(|_| "无法恢复窗口尺寸。")?;
            window
                .set_position(geometry.position)
                .map_err(|_| "无法恢复窗口位置。")?;
        }
        Ok("closed".into())
    }
}

#[tauri::command]
fn update_preferences(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    patch: PreferencesPatch,
) -> Result<Preferences, String> {
    let previous = state.preferences.lock().unwrap().clone();
    let next = {
        let mut current = state.preferences.lock().unwrap();
        patch.apply(&mut current);
        current.clone()
    };

    if let Some(window) = app.get_webview_window("main") {
        window
            .set_always_on_top(next.always_on_top)
            .map_err(|_| "无法更新置顶状态。")?;
        window
            .set_resizable(!next.lock_window_position)
            .map_err(|_| "无法更新位置锁定状态。")?;
        if (previous.ui_scale - next.ui_scale).abs() > f64::EPSILON {
            apply_scale_size(&window, &state, next.ui_scale)?;
        }
        if previous.edge_dock_enabled && !next.edge_dock_enabled {
            if state.settings_open.load(Ordering::SeqCst) {
                state.dock_expanded.store(true, Ordering::SeqCst);
                emit_dock_state(&app);
            } else {
                restore_undocked_window(&app, &window, &next);
            }
        }
    }

    if previous.autostart != next.autostart {
        apply_autostart(&app, next.autostart)?;
    }
    save_preferences(&app, &next);
    rebuild_tray_menu(&app, &next)?;
    let _ = app.emit("preferences-updated", &next);
    Ok(next)
}

fn normalize_scale(value: f64) -> f64 {
    const SCALES: [f64; 4] = [0.8, 1.0, 1.25, 1.5];
    SCALES
        .into_iter()
        .min_by(|left, right| {
            (left - value)
                .abs()
                .partial_cmp(&(right - value).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(1.0)
}

fn apply_scale_size(
    window: &tauri::WebviewWindow,
    state: &AppState,
    scale: f64,
) -> Result<(), String> {
    suppress_dock_detection(state, 600);
    window
        .set_min_size(Some(LogicalSize::new(
            BASE_WIDTH * scale,
            COLLAPSED_HEIGHT * scale,
        )))
        .map_err(|_| "无法更新窗口最小尺寸。")?;
    window
        .set_max_size(Some(LogicalSize::new(1080.0, 900.0)))
        .map_err(|_| "无法更新窗口最大尺寸。")?;

    let preferences = state.preferences.lock().unwrap().clone();
    if let Some(edge) = preferences.dock_edge {
        if !state.settings_open.load(Ordering::SeqCst) {
            if let Some(target) = dock_target_geometry(
                window,
                &preferences,
                edge,
                state.dock_expanded.load(Ordering::SeqCst),
            ) {
                window
                    .set_size(target.size)
                    .map_err(|_| "无法调整停靠窗口尺寸。")?;
                window
                    .set_position(target.position)
                    .map_err(|_| "无法调整停靠窗口位置。")?;
            }
            return Ok(());
        }
    }

    if state.settings_open.load(Ordering::SeqCst) {
        let factor = window.scale_factor().unwrap_or(1.0);
        let collapsed = PhysicalSize::new(
            (BASE_WIDTH * scale * factor).round() as u32,
            (COLLAPSED_HEIGHT * scale * factor).round() as u32,
        );
        if let Some(geometry) = state.settings_geometry.lock().unwrap().as_mut() {
            geometry.size = collapsed;
        }
        window
            .set_size(LogicalSize::new(
                BASE_WIDTH * scale,
                SETTINGS_HEIGHT * scale,
            ))
            .map_err(|_| "无法调整设置窗口尺寸。".to_string())
    } else {
        window
            .set_size(LogicalSize::new(
                BASE_WIDTH * scale,
                COLLAPSED_HEIGHT * scale,
            ))
            .map_err(|_| "无法调整窗口大小。".to_string())
    }
}

fn settings_position(
    window: &tauri::WebviewWindow,
    current_position: PhysicalPosition<i32>,
    current_size: PhysicalSize<u32>,
    target_size: PhysicalSize<u32>,
) -> (PhysicalPosition<i32>, &'static str) {
    let Some((work_position, work_size)) = current_work_area(window) else {
        return (current_position, "down");
    };
    let top = work_position.y;
    let bottom = top.saturating_add(work_size.height as i32);
    let target_bottom = current_position.y.saturating_add(target_size.height as i32);
    if target_bottom <= bottom {
        (current_position, "down")
    } else {
        let growth = target_size.height.saturating_sub(current_size.height) as i32;
        (
            PhysicalPosition::new(current_position.x, (current_position.y - growth).max(top)),
            "up",
        )
    }
}

fn current_work_area(
    window: &tauri::WebviewWindow,
) -> Option<(PhysicalPosition<i32>, PhysicalSize<u32>)> {
    #[cfg(windows)]
    {
        use windows::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        };

        let hwnd = window.hwnd().ok()?;
        let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            let rect = info.rcWork;
            return Some((
                PhysicalPosition::new(rect.left, rect.top),
                PhysicalSize::new(
                    rect.right.saturating_sub(rect.left) as u32,
                    rect.bottom.saturating_sub(rect.top) as u32,
                ),
            ));
        }
    }

    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())?;
    Some((*monitor.position(), *monitor.size()))
}

#[derive(Clone, Copy, Debug)]
struct WorkArea {
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
}

fn dock_state(state: &AppState) -> DockState {
    let edge = state.preferences.lock().unwrap().dock_edge;
    DockState {
        docked: edge.is_some(),
        expanded: edge.is_none() || state.dock_expanded.load(Ordering::SeqCst),
        edge,
    }
}

fn detect_dock_edge(
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    work_area: WorkArea,
    threshold: i32,
) -> Option<DockEdge> {
    let work_right = work_area
        .position
        .x
        .saturating_add(work_area.size.width as i32);
    let window_right = position.x.saturating_add(size.width as i32);
    let candidates = [
        (
            DockEdge::Left,
            (position.x - work_area.position.x).unsigned_abs() as i32,
        ),
        (
            DockEdge::Right,
            (window_right - work_right).unsigned_abs() as i32,
        ),
        (
            DockEdge::Top,
            (position.y - work_area.position.y).unsigned_abs() as i32,
        ),
    ];
    candidates
        .into_iter()
        .filter(|(_, distance)| *distance <= threshold)
        .min_by_key(|(_, distance)| *distance)
        .map(|(edge, _)| edge)
}

fn should_release_dock(
    edge: DockEdge,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    work_area: WorkArea,
    distance: i32,
) -> bool {
    let work_right = work_area
        .position
        .x
        .saturating_add(work_area.size.width as i32);
    let window_right = position.x.saturating_add(size.width as i32);
    match edge {
        DockEdge::Left => position.x - work_area.position.x > distance,
        DockEdge::Right => work_right - window_right > distance,
        DockEdge::Top => position.y - work_area.position.y > distance,
    }
}

fn dock_offset(
    edge: DockEdge,
    position: PhysicalPosition<i32>,
    work_area: WorkArea,
    scale_factor: f64,
) -> f64 {
    let physical = match edge {
        DockEdge::Left | DockEdge::Right => position.y - work_area.position.y,
        DockEdge::Top => position.x - work_area.position.x,
    };
    physical.max(0) as f64 / scale_factor.max(0.1)
}

fn dock_window_size(
    preferences: &Preferences,
    edge: DockEdge,
    expanded: bool,
    scale_factor: f64,
) -> PhysicalSize<u32> {
    let (width, height) = if expanded {
        (
            BASE_WIDTH * preferences.ui_scale,
            COLLAPSED_HEIGHT * preferences.ui_scale,
        )
    } else {
        match edge {
            DockEdge::Top => (
                DOCK_TOP_WIDTH * preferences.ui_scale,
                DOCK_TOP_HEIGHT * preferences.ui_scale,
            ),
            DockEdge::Left | DockEdge::Right => (
                DOCK_SIDE_WIDTH * preferences.ui_scale,
                DOCK_SIDE_HEIGHT * preferences.ui_scale,
            ),
        }
    };
    PhysicalSize::new(
        (width * scale_factor).round().max(1.0) as u32,
        (height * scale_factor).round().max(1.0) as u32,
    )
}

fn dock_target_geometry(
    window: &tauri::WebviewWindow,
    preferences: &Preferences,
    edge: DockEdge,
    expanded: bool,
) -> Option<WindowGeometry> {
    let (work_position, work_size) = current_work_area(window)?;
    let factor = window.scale_factor().unwrap_or(1.0);
    let size = dock_window_size(preferences, edge, expanded, factor);
    let raw_offset = (preferences.dock_offset.unwrap_or(0.0) * factor).round() as i32;
    let max_offset = match edge {
        DockEdge::Left | DockEdge::Right => work_size.height.saturating_sub(size.height) as i32,
        DockEdge::Top => work_size.width.saturating_sub(size.width) as i32,
    };
    let offset = raw_offset.clamp(0, max_offset.max(0));
    let work_right = work_position.x.saturating_add(work_size.width as i32);
    let position = match edge {
        DockEdge::Left => PhysicalPosition::new(work_position.x, work_position.y + offset),
        DockEdge::Right => {
            PhysicalPosition::new(work_right - size.width as i32, work_position.y + offset)
        }
        DockEdge::Top => PhysicalPosition::new(work_position.x + offset, work_position.y),
    };
    Some(WindowGeometry { position, size })
}

fn set_dock_size_limits(window: &tauri::WebviewWindow, preferences: &Preferences, expanded: bool) {
    if expanded {
        let _ = window.set_min_size(Some(LogicalSize::new(
            BASE_WIDTH * preferences.ui_scale,
            COLLAPSED_HEIGHT * preferences.ui_scale,
        )));
    } else {
        let _ = window.set_min_size(Some(LogicalSize::new(1.0, 1.0)));
    }
}

fn emit_dock_state(app: &AppHandle) {
    let state = app.state::<AppState>();
    let _ = app.emit("dock-state-updated", dock_state(&state));
}

fn suppress_dock_detection(state: &AppState, duration_ms: u64) {
    let until = app_server::now_ms()
        .saturating_add(duration_ms as i64)
        .max(0) as u64;
    state
        .dock_ignore_until_ms
        .fetch_max(until, Ordering::SeqCst);
}

fn apply_dock_geometry_immediate(app: &AppHandle, expanded: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let preferences = state.preferences.lock().unwrap().clone();
    let Some(edge) = preferences.dock_edge else {
        state.dock_expanded.store(true, Ordering::SeqCst);
        emit_dock_state(app);
        return Ok(());
    };
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "未找到主窗口。".to_string())?;
    let target = dock_target_geometry(&window, &preferences, edge, expanded)
        .ok_or_else(|| "无法确定停靠显示器。".to_string())?;
    state.dock_animating.store(true, Ordering::SeqCst);
    suppress_dock_detection(&state, 600);
    if !expanded {
        set_dock_size_limits(&window, &preferences, false);
    }
    window
        .set_size(target.size)
        .map_err(|_| "无法调整停靠窗口尺寸。")?;
    window
        .set_position(target.position)
        .map_err(|_| "无法调整停靠窗口位置。")?;
    state.dock_expanded.store(expanded, Ordering::SeqCst);
    if expanded {
        set_dock_size_limits(&window, &preferences, true);
    }
    state.dock_animating.store(false, Ordering::SeqCst);
    emit_dock_state(app);
    Ok(())
}

async fn set_dock_expanded_internal(
    app: &AppHandle,
    expanded: bool,
    animated: bool,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let preferences = state.preferences.lock().unwrap().clone();
    let Some(edge) = preferences.dock_edge else {
        state.dock_expanded.store(true, Ordering::SeqCst);
        emit_dock_state(app);
        return Ok(());
    };
    if state.dock_expanded.load(Ordering::SeqCst) == expanded
        && !state.dock_animating.load(Ordering::SeqCst)
    {
        return Ok(());
    }
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "未找到主窗口。".to_string())?;
    let target = dock_target_geometry(&window, &preferences, edge, expanded)
        .ok_or_else(|| "无法确定停靠显示器。".to_string())?;
    let start = WindowGeometry {
        position: window.outer_position().map_err(|_| "无法读取窗口位置。")?,
        size: window.outer_size().map_err(|_| "无法读取窗口尺寸。")?,
    };

    state.dock_expanded.store(expanded, Ordering::SeqCst);
    state.dock_animating.store(true, Ordering::SeqCst);
    suppress_dock_detection(&state, DOCK_ANIMATION_MS + 500);
    if !expanded {
        set_dock_size_limits(&window, &preferences, false);
    }
    let generation = state
        .dock_animation_generation
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    emit_dock_state(app);

    let steps = if animated { DOCK_ANIMATION_STEPS } else { 1 };
    for step in 1..=steps {
        if state.dock_animation_generation.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        let progress = step as f64 / steps as f64;
        let eased = 1.0 - (1.0 - progress).powi(3);
        let position = PhysicalPosition::new(
            interpolate_i32(start.position.x, target.position.x, eased),
            interpolate_i32(start.position.y, target.position.y, eased),
        );
        let size = PhysicalSize::new(
            interpolate_u32(start.size.width, target.size.width, eased),
            interpolate_u32(start.size.height, target.size.height, eased),
        );
        let _ = window.set_size(size);
        let _ = window.set_position(position);
        if animated && step < steps {
            tokio::time::sleep(Duration::from_millis(
                DOCK_ANIMATION_MS / DOCK_ANIMATION_STEPS,
            ))
            .await;
        }
    }
    tokio::time::sleep(Duration::from_millis(40)).await;
    if expanded {
        set_dock_size_limits(&window, &preferences, true);
    }
    state.dock_animating.store(false, Ordering::SeqCst);
    Ok(())
}

fn interpolate_i32(start: i32, end: i32, progress: f64) -> i32 {
    (start as f64 + (end - start) as f64 * progress).round() as i32
}

fn interpolate_u32(start: u32, end: u32, progress: f64) -> u32 {
    (start as f64 + (end as f64 - start as f64) * progress)
        .round()
        .max(1.0) as u32
}

fn restore_undocked_window(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    preferences: &Preferences,
) {
    let state = app.state::<AppState>();
    state
        .dock_animation_generation
        .fetch_add(1, Ordering::SeqCst);
    state.dock_animating.store(true, Ordering::SeqCst);
    suppress_dock_detection(&state, 600);
    state.dock_expanded.store(true, Ordering::SeqCst);
    set_dock_size_limits(window, preferences, true);
    let factor = window.scale_factor().unwrap_or(1.0);
    let size = PhysicalSize::new(
        preferences
            .window_width
            .unwrap_or((BASE_WIDTH * preferences.ui_scale * factor).round() as u32),
        preferences
            .window_height
            .unwrap_or((COLLAPSED_HEIGHT * preferences.ui_scale * factor).round() as u32),
    );
    let _ = window.set_size(size);
    if let (Some(x), Some(y)) = (preferences.window_x, preferences.window_y) {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    ensure_window_visible(window);
    state.dock_animating.store(false, Ordering::SeqCst);
    emit_dock_state(app);
}

fn schedule_dock_evaluation(app: AppHandle) {
    let state = app.state::<AppState>();
    let generation = state.dock_motion_generation.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(DOCK_MOVE_DEBOUNCE_MS)).await;
        if left_mouse_pressed() {
            schedule_dock_evaluation(app);
            return;
        }
        let state = app.state::<AppState>();
        if state.dock_motion_generation.load(Ordering::SeqCst) != generation
            || state.dock_animating.load(Ordering::SeqCst)
            || state.settings_open.load(Ordering::SeqCst)
            || (app_server::now_ms().max(0) as u64)
                < state.dock_ignore_until_ms.load(Ordering::SeqCst)
        {
            return;
        }
        evaluate_dock(&app);
    });
}

fn left_mouse_pressed() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
        return unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0;
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn evaluate_dock(app: &AppHandle) {
    let state = app.state::<AppState>();
    let current_preferences = state.preferences.lock().unwrap().clone();
    if !current_preferences.edge_dock_enabled || current_preferences.lock_window_position {
        return;
    }
    if current_preferences.dock_edge.is_some() && !state.dock_expanded.load(Ordering::SeqCst) {
        return;
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    let Some((work_position, work_size)) = current_work_area(&window) else {
        return;
    };
    let work_area = WorkArea {
        position: work_position,
        size: work_size,
    };
    let factor = window.scale_factor().unwrap_or(1.0);
    let snap_distance = (DOCK_SNAP_DISTANCE * factor).round() as i32;
    let release_distance = (DOCK_RELEASE_DISTANCE * factor).round() as i32;

    if let Some(current_edge) = current_preferences.dock_edge {
        if should_release_dock(current_edge, position, size, work_area, release_distance) {
            let next = {
                let mut preferences = state.preferences.lock().unwrap();
                preferences.dock_edge = None;
                preferences.dock_monitor_id = None;
                preferences.dock_offset = None;
                preferences.window_x = Some(position.x);
                preferences.window_y = Some(position.y);
                preferences.window_width = Some(size.width);
                preferences.window_height = Some(size.height);
                preferences.clone()
            };
            state.dock_expanded.store(true, Ordering::SeqCst);
            save_preferences(app, &next);
            let _ = rebuild_tray_menu(app, &next);
            let _ = app.emit("preferences-updated", &next);
            emit_dock_state(app);
            return;
        }
    }

    let detected = detect_dock_edge(position, size, work_area, snap_distance);
    let Some(edge) = detected.or(current_preferences.dock_edge) else {
        return;
    };
    let monitor_id = window.current_monitor().ok().flatten().map(|monitor| {
        format!(
            "{}@{},{}",
            monitor.name().map(String::as_str).unwrap_or("monitor"),
            monitor.position().x,
            monitor.position().y
        )
    });
    let next = {
        let mut preferences = state.preferences.lock().unwrap();
        if preferences.dock_edge.is_none() {
            preferences.window_x = Some(position.x);
            preferences.window_y = Some(position.y);
            preferences.window_width = Some(size.width);
            preferences.window_height = Some(size.height);
        }
        preferences.dock_edge = Some(edge);
        preferences.dock_monitor_id = monitor_id;
        preferences.dock_offset = Some(dock_offset(edge, position, work_area, factor));
        preferences.clone()
    };
    save_preferences(app, &next);
    let _ = rebuild_tray_menu(app, &next);
    let _ = app.emit("preferences-updated", &next);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = set_dock_expanded_internal(&handle, false, true).await;
    });
}

async fn refresh_into_state(app: &AppHandle, force: bool) -> QuotaStatus {
    if !main_window_visible(app) {
        return app.state::<AppState>().status.lock().unwrap().clone();
    }

    let state = app.state::<AppState>();
    let observed_generation = state.refresh_generation.load(Ordering::Acquire);
    let _guard = state.refresh_gate.lock().await;
    if state.refresh_generation.load(Ordering::Acquire) != observed_generation {
        return state.status.lock().unwrap().clone();
    }

    let now = app_server::now_ms();
    let current = state.status.lock().unwrap().clone();
    if should_use_success_cache(&current, now, force) {
        return current;
    }

    let next = match app_server::read_quota().await {
        Ok(outcome) => {
            *state.diagnostics.lock().unwrap() = DiagnosticsState {
                codex_found: true,
                candidate_source: Some(outcome.candidate_source),
                last_error_code: None,
            };
            outcome.status
        }
        Err(error) => {
            *state.diagnostics.lock().unwrap() = DiagnosticsState {
                codex_found: error.codex_found,
                candidate_source: error.candidate_source,
                last_error_code: Some(error.code.clone()),
            };
            QuotaStatus::with_failure(&current, error.code, error.message, now)
        }
    };
    *state.status.lock().unwrap() = next.clone();
    state.refresh_generation.fetch_add(1, Ordering::Release);
    let _ = app.emit("quota-updated", &next);
    next
}

fn should_use_success_cache(status: &QuotaStatus, now_ms: i64, force: bool) -> bool {
    !force
        && status.state == "ready"
        && status
            .last_success_at
            .is_some_and(|last| (0..=SUCCESS_CACHE_MS).contains(&(now_ms - last)))
}

fn main_window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

fn config_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|path| path.join("preferences.json"))
}

fn load_preferences(app: &AppHandle) -> Preferences {
    let Some(path) = config_path(app) else {
        return Preferences::default();
    };
    let Ok(bytes) = fs::read(&path) else {
        return Preferences::default();
    };
    match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => {
            let timestamp = app_server::now_ms();
            let backup = path.with_file_name(format!("preferences.corrupt-{timestamp}.json"));
            let _ = fs::rename(path, backup);
            Preferences::default()
        }
    }
}

fn save_preferences(app: &AppHandle, value: &Preferences) {
    if let Some(path) = config_path(app) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, serde_json::to_vec_pretty(value).unwrap_or_default());
    }
}

fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch()
            .enable()
            .map_err(|_| "无法启用开机启动。".to_string())
    } else {
        app.autolaunch()
            .disable()
            .map_err(|_| "无法禁用开机启动。".to_string())
    }
}

fn show_window(app: &AppHandle, focus: bool) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    ensure_window_visible(&window);
    let _ = window.show();
    if focus {
        let _ = window.set_focus();
    }
    let preferences = {
        let state = app.state::<AppState>();
        let mut preferences = state.preferences.lock().unwrap();
        preferences.visible = true;
        preferences.clone()
    };
    save_preferences(app, &preferences);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        refresh_into_state(&handle, false).await;
    });
}

fn hide_window(app: &AppHandle, manual: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    let state = app.state::<AppState>();
    if manual && state.process_running.load(Ordering::SeqCst) {
        state.manual_hidden_cycle.store(true, Ordering::SeqCst);
    }
    let preferences = {
        let mut preferences = state.preferences.lock().unwrap();
        preferences.visible = false;
        preferences.clone()
    };
    save_preferences(app, &preferences);
}

fn toggle_window(app: &AppHandle) {
    if main_window_visible(app) {
        hide_window(app, true);
    } else {
        show_window(app, true);
    }
}

fn ensure_window_visible(window: &tauri::WebviewWindow) {
    let Ok(position) = window.outer_position() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let monitors = window.available_monitors().unwrap_or_default();
    let intersects = monitors
        .iter()
        .any(|monitor| rectangles_intersect(position, size, *monitor.position(), *monitor.size()));
    if intersects {
        return;
    }
    let monitor = window
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| monitors.into_iter().next());
    if let Some(monitor) = monitor {
        let margin = 24;
        let target = PhysicalPosition::new(
            monitor.position().x.saturating_add(margin),
            monitor.position().y.saturating_add(margin),
        );
        let _ = window.set_position(target);
    }
}

fn rectangles_intersect(
    left_position: PhysicalPosition<i32>,
    left_size: PhysicalSize<u32>,
    right_position: PhysicalPosition<i32>,
    right_size: PhysicalSize<u32>,
) -> bool {
    let left_right = left_position.x.saturating_add(left_size.width as i32);
    let left_bottom = left_position.y.saturating_add(left_size.height as i32);
    let right_right = right_position.x.saturating_add(right_size.width as i32);
    let right_bottom = right_position.y.saturating_add(right_size.height as i32);
    left_position.x < right_right
        && left_right > right_position.x
        && left_position.y < right_bottom
        && left_bottom > right_position.y
}

fn schedule_geometry_save(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.settings_open.load(Ordering::SeqCst)
        || state.preferences.lock().unwrap().dock_edge.is_some()
    {
        return;
    }
    let generation = state.position_generation.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(350)).await;
        let state = app.state::<AppState>();
        if state.position_generation.load(Ordering::SeqCst) != generation
            || state.settings_open.load(Ordering::SeqCst)
            || state.preferences.lock().unwrap().dock_edge.is_some()
        {
            return;
        }
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
            return;
        };
        let preferences = {
            let mut preferences = state.preferences.lock().unwrap();
            preferences.window_x = Some(position.x);
            preferences.window_y = Some(position.y);
            preferences.window_width = Some(size.width);
            preferences.window_height = Some(size.height);
            preferences.clone()
        };
        save_preferences(&app, &preferences);
    });
}

fn build_tray_menu(app: &AppHandle, preferences: &Preferences) -> tauri::Result<Menu<tauri::Wry>> {
    let toggle = MenuItem::with_id(app, "toggle", "显示 / 隐藏额度轨道", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "立即刷新", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let primary_five = CheckMenuItem::with_id(
        app,
        "primary_five",
        "主环显示：5 小时额度",
        true,
        preferences.primary_quota_window == PrimaryQuotaWindow::FiveHour,
        None::<&str>,
    )?;
    let primary_week = CheckMenuItem::with_id(
        app,
        "primary_week",
        "主环显示：周额度",
        true,
        preferences.primary_quota_window == PrimaryQuotaWindow::Weekly,
        None::<&str>,
    )?;
    let lock = CheckMenuItem::with_id(
        app,
        "lock_position",
        "锁定窗口位置",
        true,
        preferences.lock_window_position,
        None::<&str>,
    )?;
    let top = CheckMenuItem::with_id(
        app,
        "always_on_top",
        "始终置顶",
        true,
        preferences.always_on_top,
        None::<&str>,
    )?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "开机启动",
        true,
        preferences.autostart,
        None::<&str>,
    )?;
    let edge_dock = CheckMenuItem::with_id(
        app,
        "edge_dock",
        "贴边收起",
        true,
        preferences.edge_dock_enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &toggle,
            &refresh,
            &settings,
            &primary_five,
            &primary_week,
            &lock,
            &top,
            &autostart,
            &edge_dock,
            &quit,
        ],
    )
}

fn rebuild_tray_menu(app: &AppHandle, preferences: &Preferences) -> Result<(), String> {
    let menu = build_tray_menu(app, preferences).map_err(|_| "无法更新托盘菜单。")?;
    if let Some(tray) = app.tray_by_id("main-tray") {
        tray.set_menu(Some(menu))
            .map_err(|_| "无法更新托盘菜单。")?;
    }
    Ok(())
}

fn patch_from_tray(app: &AppHandle, patch: PreferencesPatch) {
    let state = app.state::<AppState>();
    let _ = update_preferences(app.clone(), state, patch);
}

fn empty_patch() -> PreferencesPatch {
    PreferencesPatch {
        refresh_interval_secs: None,
        always_on_top: None,
        autostart: None,
        primary_quota_window: None,
        ui_scale: None,
        show_credits: None,
        auto_show_on_codex: None,
        auto_hide_on_codex_close: None,
        start_hidden_on_autostart: None,
        lock_window_position: None,
        edge_dock_enabled: None,
        dock_auto_collapse_delay_ms: None,
    }
}

fn handle_tray_event(app: &AppHandle, id: &str) {
    match id {
        "toggle" => toggle_window(app),
        "refresh" => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                refresh_into_state(&handle, true).await;
            });
        }
        "settings" => {
            show_window(app, true);
            let _ = set_settings_open(app.clone(), app.state::<AppState>(), true);
            let _ = app.emit("settings-open-requested", ());
        }
        "primary_five" | "primary_week" => {
            let mut patch = empty_patch();
            patch.primary_quota_window = Some(if id == "primary_week" {
                PrimaryQuotaWindow::Weekly
            } else {
                PrimaryQuotaWindow::FiveHour
            });
            patch_from_tray(app, patch);
        }
        "lock_position" => {
            let current = app
                .state::<AppState>()
                .preferences
                .lock()
                .unwrap()
                .lock_window_position;
            let mut patch = empty_patch();
            patch.lock_window_position = Some(!current);
            patch_from_tray(app, patch);
        }
        "always_on_top" => {
            let current = app
                .state::<AppState>()
                .preferences
                .lock()
                .unwrap()
                .always_on_top;
            let mut patch = empty_patch();
            patch.always_on_top = Some(!current);
            patch_from_tray(app, patch);
        }
        "autostart" => {
            let current = app
                .state::<AppState>()
                .preferences
                .lock()
                .unwrap()
                .autostart;
            let mut patch = empty_patch();
            patch.autostart = Some(!current);
            patch_from_tray(app, patch);
        }
        "edge_dock" => {
            let current = app
                .state::<AppState>()
                .preferences
                .lock()
                .unwrap()
                .edge_dock_enabled;
            let mut patch = empty_patch();
            patch.edge_dock_enabled = Some(!current);
            patch_from_tray(app, patch);
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

fn spawn_refresh_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if main_window_visible(&app) {
            refresh_into_state(&app, false).await;
        }
        loop {
            let seconds = app
                .state::<AppState>()
                .preferences
                .lock()
                .unwrap()
                .refresh_interval_secs;
            tokio::time::sleep(Duration::from_secs(seconds)).await;
            if main_window_visible(&app) {
                refresh_into_state(&app, false).await;
            }
        }
    });
}

fn spawn_process_monitor(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut system = System::new();
        let own_pid = get_current_pid().ok();
        let mut was_running = false;
        let mut missing_checks = 0_u8;
        loop {
            system.refresh_processes(ProcessesToUpdate::All, true);
            let running = system.processes().iter().any(|(_, process)| {
                is_target_process(
                    &process.name().to_string_lossy(),
                    process.parent() == own_pid,
                )
            });
            let state = app.state::<AppState>();
            state.process_running.store(running, Ordering::SeqCst);
            if running {
                missing_checks = 0;
                if !was_running {
                    state.manual_hidden_cycle.store(false, Ordering::SeqCst);
                    let preferences = state.preferences.lock().unwrap().clone();
                    if preferences.auto_show_on_codex
                        && !state.manual_hidden_cycle.load(Ordering::SeqCst)
                    {
                        show_window(&app, false);
                    }
                }
                was_running = true;
            } else if was_running {
                missing_checks = missing_checks.saturating_add(1);
                if missing_checks >= 2 {
                    let preferences = state.preferences.lock().unwrap().clone();
                    if preferences.auto_hide_on_codex_close {
                        hide_window(&app, false);
                    }
                    state.manual_hidden_cycle.store(false, Ordering::SeqCst);
                    was_running = false;
                    missing_checks = 0;
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

fn is_target_process(name: &str, is_own_child: bool) -> bool {
    let name = name.to_ascii_lowercase();
    name == "chatgpt.exe" || (name == "codex.exe" && !is_own_child)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app, true);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .manage(AppState {
            status: Mutex::new(QuotaStatus::loading()),
            preferences: Mutex::new(Preferences::default()),
            settings_open: AtomicBool::new(false),
            settings_geometry: Mutex::new(None),
            refresh_gate: tokio::sync::Mutex::new(()),
            refresh_generation: AtomicU64::new(0),
            position_generation: AtomicU64::new(0),
            process_running: AtomicBool::new(false),
            manual_hidden_cycle: AtomicBool::new(false),
            diagnostics: Mutex::new(DiagnosticsState::default()),
            dock_expanded: AtomicBool::new(true),
            dock_animating: AtomicBool::new(false),
            dock_motion_generation: AtomicU64::new(0),
            dock_animation_generation: AtomicU64::new(0),
            dock_ignore_until_ms: AtomicU64::new(0),
        })
        .invoke_handler(tauri::generate_handler![
            get_quota_status,
            get_preferences,
            get_dock_state,
            get_diagnostics,
            refresh_quota,
            update_preferences,
            set_settings_open,
            set_dock_expanded
        ])
        .setup(|app| {
            let preferences = load_preferences(&app.handle());
            *app.state::<AppState>().preferences.lock().unwrap() = preferences.clone();
            *app.state::<AppState>().diagnostics.lock().unwrap() = DiagnosticsState {
                codex_found: app_server::codex_installation().is_some(),
                candidate_source: app_server::codex_installation(),
                last_error_code: None,
            };

            let window = app.get_webview_window("main").expect("main window missing");
            let _ = window.set_always_on_top(preferences.always_on_top);
            let _ = window.set_resizable(!preferences.lock_window_position);
            let _ = apply_scale_size(&window, &app.state::<AppState>(), preferences.ui_scale);
            if let (Some(x), Some(y)) = (preferences.window_x, preferences.window_y) {
                let _ = window.set_position(PhysicalPosition::new(x, y));
            }
            if let (Some(width), Some(height)) =
                (preferences.window_width, preferences.window_height)
            {
                let _ = window.set_size(PhysicalSize::new(width, height));
            }
            ensure_window_visible(&window);
            if preferences.edge_dock_enabled && preferences.dock_edge.is_some() {
                let _ = apply_dock_geometry_immediate(&app.handle(), false);
            }

            let launched_from_autostart = std::env::args().any(|arg| arg == "--autostart");
            if launched_from_autostart && preferences.start_hidden_on_autostart {
                let _ = window.hide();
                app.state::<AppState>().preferences.lock().unwrap().visible = false;
            } else {
                let _ = window.show();
                app.state::<AppState>().preferences.lock().unwrap().visible = true;
            }

            let menu = build_tray_menu(&app.handle(), &preferences)?;
            let mut tray = TrayIconBuilder::with_id("main-tray")
                .tooltip("Codex Quota Ring")
                .menu(&menu)
                .on_menu_event(|app, event| handle_tray_event(app, event.id().as_ref()));
            if let Some(icon) = app.default_window_icon().cloned() {
                tray = tray.icon(icon);
            }
            tray.build(app)?;

            spawn_refresh_loop(app.handle().clone());
            spawn_process_monitor(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
                if let Some(webview) = window.app_handle().get_webview_window("main") {
                    ensure_window_visible(&webview);
                }
                schedule_geometry_save(window.app_handle().clone());
            }
            if matches!(event, WindowEvent::Moved(_)) {
                schedule_dock_evaluation(window.app_handle().clone());
            }
            if matches!(event, WindowEvent::ScaleFactorChanged { .. }) {
                if let Some(webview) = window.app_handle().get_webview_window("main") {
                    ensure_window_visible(&webview);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run Codex Quota Ring");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_v020_preferences_with_v030_defaults() {
        let json = r#"{
            "refreshIntervalSecs": 300,
            "visible": true,
            "alwaysOnTop": true,
            "autostart": false,
            "windowX": 20,
            "windowY": 30,
            "primaryQuotaWindow": "fiveHour",
            "uiScale": 1.25
        }"#;
        let preferences: Preferences = serde_json::from_str(json).unwrap();
        assert!(preferences.show_credits);
        assert!(!preferences.auto_show_on_codex);
        assert!(!preferences.auto_hide_on_codex_close);
        assert!(!preferences.start_hidden_on_autostart);
        assert!(!preferences.lock_window_position);
        assert!(!preferences.edge_dock_enabled);
        assert!(preferences.dock_edge.is_none());
        assert_eq!(preferences.dock_auto_collapse_delay_ms, 800);
        assert_eq!(preferences.ui_scale, 1.25);
        assert_eq!(preferences.window_x, Some(20));
    }

    #[test]
    fn snaps_ui_scale_to_supported_values() {
        assert_eq!(normalize_scale(0.79), 0.8);
        assert_eq!(normalize_scale(1.18), 1.25);
        assert_eq!(normalize_scale(1.49), 1.5);
    }

    #[test]
    fn detects_intersecting_and_offscreen_rectangles() {
        let monitor_position = PhysicalPosition::new(0, 0);
        let monitor_size = PhysicalSize::new(1920, 1080);
        assert!(rectangles_intersect(
            PhysicalPosition::new(1900, 100),
            PhysicalSize::new(100, 100),
            monitor_position,
            monitor_size
        ));
        assert!(!rectangles_intersect(
            PhysicalPosition::new(2000, 100),
            PhysicalSize::new(100, 100),
            monitor_position,
            monitor_size
        ));
    }

    #[test]
    fn five_second_cache_can_be_bypassed_by_force_refresh() {
        let mut status = QuotaStatus::loading();
        status.state = "ready".into();
        status.last_success_at = Some(10_000);
        assert!(should_use_success_cache(&status, 15_000, false));
        assert!(!should_use_success_cache(&status, 15_001, false));
        assert!(!should_use_success_cache(&status, 10_001, true));
    }

    #[test]
    fn classifies_codex_processes_without_counting_own_child() {
        assert!(is_target_process("ChatGPT.exe", false));
        assert!(is_target_process("codex.exe", false));
        assert!(!is_target_process("codex.exe", true));
        assert!(!is_target_process("other.exe", false));
    }

    #[test]
    fn detects_left_right_and_top_dock_edges_but_not_bottom() {
        let work = WorkArea {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(1920, 1080),
        };
        let size = PhysicalSize::new(450, 160);
        assert_eq!(
            detect_dock_edge(PhysicalPosition::new(8, 300), size, work, 12),
            Some(DockEdge::Left)
        );
        assert_eq!(
            detect_dock_edge(PhysicalPosition::new(1464, 300), size, work, 12),
            Some(DockEdge::Right)
        );
        assert_eq!(
            detect_dock_edge(PhysicalPosition::new(500, 10), size, work, 12),
            Some(DockEdge::Top)
        );
        assert_eq!(
            detect_dock_edge(PhysicalPosition::new(500, 920), size, work, 12),
            None
        );
    }

    #[test]
    fn releases_only_when_dragged_inward_beyond_threshold() {
        let work = WorkArea {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(1920, 1080),
        };
        let size = PhysicalSize::new(450, 160);
        assert!(!should_release_dock(
            DockEdge::Left,
            PhysicalPosition::new(32, 200),
            size,
            work,
            32
        ));
        assert!(should_release_dock(
            DockEdge::Left,
            PhysicalPosition::new(33, 200),
            size,
            work,
            32
        ));
        assert!(should_release_dock(
            DockEdge::Top,
            PhysicalPosition::new(300, 40),
            size,
            work,
            32
        ));
    }

    #[test]
    fn stores_dock_offsets_in_logical_pixels() {
        let work = WorkArea {
            position: PhysicalPosition::new(100, 50),
            size: PhysicalSize::new(1600, 900),
        };
        assert_eq!(
            dock_offset(DockEdge::Left, PhysicalPosition::new(100, 250), work, 2.0),
            100.0
        );
        assert_eq!(
            dock_offset(DockEdge::Top, PhysicalPosition::new(500, 50), work, 2.0),
            200.0
        );
    }
}
