//! Resident injector helper process: performs the paste keystroke.
use super::*;

// ---------- resident injector ----------
// keybd_event issued from inside the app process is silently swallowed
// (cause unknown, reproducible); the identical call from a helper process
// lands reliably, so the app keeps a tiny resident injector and signals it
// through a named event.

use windows::Win32::System::Threading::{
    CreateEventW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE,
    SYNCHRONIZATION_ACCESS_RIGHTS,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Runs in a helper process (`app.exe --injector`): waits on the named event
/// and performs the paste keystroke. Exits when the main app is gone.
pub fn injector_loop() -> ! {
    unsafe {
        let name = wide("ClipVaultInject");
        let created = CreateEventW(None, false, false, PCWSTR(name.as_ptr()));
        if created.is_err() {
            std::process::exit(0);
        }
        let Ok(event) = created else {
            eprintln!("[cv] injector: CreateEventW failed — exiting (respawned on demand)");
            std::process::exit(1);
        };
        let listener_class = wide("ClipVaultListener");
        loop {
            if WaitForSingleObject(event, 2000) == WAIT_OBJECT_0 {
                // wait until the target really has focus back (its activation
                // is asynchronous); injecting too early loses the keystroke
                let want = read_inject_target();
                for _ in 0..80 {
                    let (fg, focus) = foreground_focus();
                    if want.0 != 0 && fg == want.0 && (focus == want.1 || want.1 == 0) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                keybd_event(VK_CONTROL.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
                keybd_event(VK_V.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
                std::thread::sleep(std::time::Duration::from_millis(30));
                keybd_event(VK_V.0 as u8, 0, KEYEVENTF_KEYUP, 0);
                keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_KEYUP, 0);
            }
            if FindWindowW(PCWSTR(listener_class.as_ptr()), None).is_err() {
                std::process::exit(0);
            }
        }
    }
}

/// (foreground hwnd, focused-child hwnd) of the current foreground thread.
fn foreground_focus() -> (isize, isize) {
    unsafe {
        let fg = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(fg, None);
        let mut gi = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread, &mut gi).is_ok() {
            (fg.0 as isize, gi.hwndFocus.0 as isize)
        } else {
            (0, 0)
        }
    }
}

/// Handshake file lives in the app's own (user-ACL'd) data dir, not TEMP —
/// any same-user process could rewrite a TEMP file and redirect the paste.
fn inject_file() -> std::path::PathBuf {
    let dir = std::env::var("APPDATA")
        .map(|d| std::path::Path::new(&d).join("com.clipvault.app"))
        .unwrap_or_else(|_| std::path::PathBuf::from("com.clipvault.app"));
    let _ = std::fs::create_dir_all(&dir);
    dir.join("cv-inject.txt")
}

/// (target hwnd, target focus hwnd) written by the main app before signaling.
/// The file is consumed (deleted) on read so a stale signal can't be replayed.
fn read_inject_target() -> (isize, isize) {
    let path = inject_file();
    if let Ok(content) = std::fs::read_to_string(&path) {
        let _ = std::fs::remove_file(&path);
        let nums: Vec<isize> = content
            .split_whitespace()
            .filter_map(|p| p.parse().ok())
            .collect();
        if nums.len() == 2 {
            return (nums[0], nums[1]);
        }
    }
    (0, 0)
}

/// The remembered focus child (0 once consumed by restore_focus).
pub fn focus_hwnd() -> isize {
    FOCUS_HWND.load(Ordering::SeqCst)
}

/// Signals the resident injector to send Ctrl+V. Returns false when the
/// injector can't be reached — the caller must surface that (the panel is
/// already hidden at that point, so a silent no-op would lose the paste).
pub fn request_paste_keystroke(target: isize, focus: isize) -> bool {
    // tell the injector what "ready" looks like
    let _ = std::fs::write(inject_file(), format!("{target} {focus}"));
    static CACHE: StdMutex<Option<isize>> = StdMutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(h) = *cache {
        if h != 0 {
            let ok = unsafe { SetEvent(HANDLE(h as *mut core::ffi::c_void)) }.is_ok();
            if ok {
                return true;
            }
            // stale handle (injector restarted) — drop it and reopen below
            *cache = None;
        }
    }
    // (re)try opening; only cache on success — a failure (injector not yet
    // up, or gone) must stay retryable
    let open = || -> Option<isize> {
        let name = wide("ClipVaultInject");
        unsafe {
            OpenEventW(
                SYNCHRONIZATION_ACCESS_RIGHTS(0x00100000) | EVENT_MODIFY_STATE,
                false,
                PCWSTR(name.as_ptr()),
            )
            .ok()
            .map(|hv| hv.0 as isize)
        }
    };
    let mut hv = open();
    if hv.is_none() {
        // injector died — respawn it (detached: it's a resident loop) and
        // give it a moment to create the event
        if let Ok(exe) = std::env::current_exe() {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let _ = std::process::Command::new(exe)
                .arg("--injector")
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
        for _ in 0..20 {
            if let Some(h) = open() {
                hv = Some(h);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    match hv {
        Some(h) => {
            let ok = unsafe { SetEvent(HANDLE(h as *mut core::ffi::c_void)) }.is_ok();
            if ok {
                *cache = Some(h);
            }
            ok
        }
        None => false,
    }
}
