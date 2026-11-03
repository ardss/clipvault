//! Edge-peek probe windows: while the panel is HIDDEN, one 3px-wide
//! mouse-transparent window sits flush against the left edge of every monitor
//! (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | TOPMOST). A
//! WS_EX_TRANSPARENT window never receives WM_MOUSEMOVE itself, so the summon
//! detection is position-based — exactly like the zoom-watchdog: a polling
//! thread calls GetCursorPos and requires ~180ms of dwell inside a probe rect
//! before showing the panel (left-edge anchored by commands::show_panel).
//!
//! Everything lives on ONE dedicated thread that owns the probe windows and
//! pumps their messages (WM_DISPLAYCHANGE and friends), rescanning the monitor
//! set periodically. The polling body is deliberately in this module, far from
//! win/mod.rs `hook_thread_body` — the hook-pump source guard
//! (hook_pump_must_not_sleep) scans only that pump region, and a probe poller
//! sleeping 60ms per tick is by design: it holds no system-wide hook.
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{PANEL_PINNED, PANEL_VISIBLE};

/// Cursor must dwell this long inside a probe strip before the panel is
/// summoned (aligned with the prototype's 180ms).
const DWELL_MS: u64 = 180;
/// Poll tick — 3 consecutive in-strip ticks ≈ 180ms.
const TICK_MS: u64 = 60;
/// How often the monitor set is rescanned (WM_DISPLAYCHANGE triggers one
/// sooner; the periodic rescan is the reliable fallback).
const RESCAN_EVERY_TICKS: u32 = 50; // ~3s

pub static EDGE_PEEK_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static DISPLAY_CHANGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct ProbeWindow {
    hwnd: isize,
    /// monitor rect the probe was built against, for dwell checks
    rect: (i32, i32, i32, i32),
}

static PROBES: Mutex<Vec<ProbeWindow>> = Mutex::new(Vec::new());

/// Enable/disable edge peek at runtime (settings). The probe thread tears the
/// windows down / rebuilds them on its next ticks.
pub fn set_edge_peek_enabled(on: bool) {
    EDGE_PEEK_ENABLED.store(on, Ordering::SeqCst);
}

/// Spawn the probe thread. Call once at setup; toggling is done via
/// `set_edge_peek_enabled` — the thread is resident and stays cheap when idle.
pub fn spawn_edge_probes(app: tauri::AppHandle<tauri::Wry>) {
    std::thread::Builder::new()
        .name("cv-edge-probe".into())
        .spawn(move || {
            probe_thread_body(app);
        })
        .ok();
}

unsafe extern "system" fn probe_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // WM_DISPLAYCHANGE: topology changed — flag it; the poll loop rebuilds
    if msg == WM_DISPLAYCHANGE {
        DISPLAY_CHANGED.store(true, Ordering::SeqCst);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Enumerate monitors the raw way (EnumDisplayMonitors): the probe thread has
/// no Tauri runtime access and must not depend on main-thread dispatch.
fn monitor_rects() -> Vec<(i32, i32, i32, i32)> {
    unsafe extern "system" fn cb(
        _hmon: HMONITOR,
        _hdc: HDC,
        rect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let list = &mut *(lparam.0 as *mut Vec<(i32, i32, i32, i32)>);
        let r = unsafe { &*rect };
        list.push((r.left, r.top, r.right - r.left, r.bottom - r.top));
        true.into()
    }
    let mut out: Vec<(i32, i32, i32, i32)> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

/// One probe window flush against the left edge of `rect` (3px wide, full
/// monitor height). Returns the HWND, or None when creation failed.
unsafe fn create_probe(class: PCWSTR, rect: (i32, i32, i32, i32)) -> Option<isize> {
    let (mx, my, _mw, mh) = rect;
    let Ok(hmod) = GetModuleHandleW(None) else {
        return None;
    };
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        class,
        WS_POPUP,
        mx,
        my,
        3,
        mh,
        None,
        None,
        HINSTANCE(hmod.0),
        None,
    )
    .ok()?;
    // SetWindowLongPtrW directly: the (LAYERED|TRANSPARENT|NOACTIVATE)
    // combination must hold exactly — these windows are never interacted with.
    let ex = WINDOW_LONG_PTR_INDEX(GWL_EXSTYLE.0);
    SetWindowLongPtrW(
        hwnd,
        ex,
        (WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0 | WS_EX_NOACTIVATE.0) as isize,
    );
    let _ = SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        mx,
        my,
        3,
        mh,
        SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_NOREDRAW,
    );
    // DPI/position verify against the OS truth (commands.rs OS-correction
    // pattern): Tauri-style bookkeeping divergence applies to raw HWNDs too
    // after per-monitor DPI rescale.
    let mut r = RECT::default();
    if GetWindowRect(hwnd, &mut r).is_ok()
        && ((r.left - mx).abs() > 4 || (r.top - my).abs() > 4 || (r.right - r.left) != 3)
    {
        let _ = SetWindowPos(
            hwnd,
            None,
            mx,
            my,
            3,
            mh,
            SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_NOZORDER,
        );
    }
    Some(hwnd.0 as isize)
}

fn destroy_probes() {
    let mut probes = PROBES.lock().unwrap_or_else(|p| p.into_inner());
    for p in probes.drain(..) {
        unsafe {
            let _ = DestroyWindow(HWND(p.hwnd as _));
        }
    }
}

fn rebuild_probes(class: PCWSTR) {
    // Only tear down / recreate when the monitor set actually changed: an
    // unconditional periodic rebuild churns TOPMOST z-order every ~3s and
    // made the whole desktop flicker (0.2.0 incident, alongside the
    // auto-hide fight).
    let rects = monitor_rects();
    {
        let probes = PROBES.lock().unwrap_or_else(|p| p.into_inner());
        if probes.len() == rects.len() && probes.iter().zip(&rects).all(|(p, r)| p.rect == *r) {
            return;
        }
    }
    destroy_probes();
    let mut probes = PROBES.lock().unwrap_or_else(|p| p.into_inner());
    for rect in rects {
        if let Some(hwnd) = unsafe { create_probe(class, rect) } {
            probes.push(ProbeWindow { hwnd, rect });
        }
    }
}

fn cursor_inside_a_probe(x: i32, y: i32) -> bool {
    let probes = PROBES.lock().unwrap_or_else(|p| p.into_inner());
    probes.iter().any(|p| {
        x >= p.rect.0 && x < p.rect.0 + p.rect.2 && y >= p.rect.1 && y < p.rect.1 + p.rect.3
    })
}

/// Hook-thread-facing guard: is this physical point inside a live probe
/// strip? The mouse-leave auto-hide branch consults this so a cursor parked
/// at the summon edge never flags "left the panel" (the 0.2.0 flicker loop
/// was exactly that branch fighting the edge-probe re-summon).
pub fn cursor_in_edge_strip(x: i32, y: i32) -> bool {
    EDGE_PEEK_ENABLED.load(Ordering::SeqCst) && cursor_inside_a_probe(x, y)
}

/// The probe thread body: owns the probe windows, pumps messages, polls the
/// cursor. NOT the hook pump — sleeping here holds no system-wide input
/// hostage (the source guard only scans win/mod.rs's pump).
fn probe_thread_body(app: tauri::AppHandle<tauri::Wry>) {
    unsafe {
        let Ok(hmod) = GetModuleHandleW(None) else {
            cvlog!("[cv] edge-probe: GetModuleHandleW failed — edge peek disabled");
            return;
        };
        let class_name: Vec<u16> = "ClipVaultEdgeProbe\0".encode_utf16().collect();
        let class = PCWSTR(class_name.as_ptr());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(probe_wndproc),
            lpszClassName: class,
            hInstance: HINSTANCE(hmod.0),
            ..Default::default()
        };
        RegisterClassW(&wc);
        rebuild_probes(class);
        let mut ticks = 0u32;
        let mut dwell_hits = 0u32;
        loop {
            // dispatch WM_DISPLAYCHANGE etc. without blocking the poll
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if !EDGE_PEEK_ENABLED.load(Ordering::SeqCst) {
                if !PROBES.lock().unwrap_or_else(|p| p.into_inner()).is_empty() {
                    destroy_probes();
                }
                dwell_hits = 0;
                std::thread::sleep(Duration::from_millis(TICK_MS));
                continue;
            }
            let rebuild =
                DISPLAY_CHANGED.swap(false, Ordering::SeqCst) || ticks % RESCAN_EVERY_TICKS == 0;
            ticks += 1;
            if rebuild {
                rebuild_probes(class);
            }
            if PANEL_VISIBLE.load(Ordering::SeqCst) || PANEL_PINNED.load(Ordering::SeqCst) {
                dwell_hits = 0;
            } else {
                let (x, y) = crate::win::get_cursor_pos();
                if cursor_inside_a_probe(x, y) {
                    dwell_hits += 1;
                } else {
                    dwell_hits = 0;
                }
                if dwell_hits * TICK_MS as u32 >= DWELL_MS as u32 {
                    dwell_hits = 0;
                    cvlog!("[cv] edge-probe: dwell satisfied, showing panel");
                    crate::commands::show_panel(&app);
                }
            }
            std::thread::sleep(Duration::from_millis(TICK_MS));
        }
    }
}
