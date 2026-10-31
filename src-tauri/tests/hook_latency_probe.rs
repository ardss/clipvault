//! L3 behavioral hook-latency probe (TESTING_ARCHITECTURE_PLAN §3.3): the
//! real proof behind incident #1 (cf2e7db — a 40ms sleep inside the
//! WH_MOUSE_LL message pump became system-wide input latency).
//!
//! The string guard (`hook_pump_must_not_sleep`, win/mod.rs:313) is only a
//! floor — any equivalent blocking without the word "sleep" bypasses it. This
//! probe measures the PROPERTY: with a WH_MOUSE_LL hook installed in THIS
//! test process, synthetic mouse moves are dispatched through the pump, and
//! the P99 of consecutive receipt gaps must stay below the threshold.
//!
//! SAFETY (plan §1.3-3 / §3.3): this probe sends REAL synthetic mouse input
//! (SendInput) and installs a REAL system-wide hook. It MUST only run behind
//! the `CV_E2E_ISOLATED=1` gate via `scripts/run-native-probes.ps1`, on a
//! machine where the production instance has been stopped and restored by the
//! executor. Without the gate it refuses with a non-zero exit. It is
//! `#[ignore]`d so no bare `cargo test` ever reaches it (plan: probes never
//! run in GH Actions).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Calibrated 2026-10-31 on machine B (in-isolation run, debug AND release
/// builds): p50=0ms, p99=8-9ms — the machine's real tail. 20ms is 2x the
/// observed tail and still only half the ~40ms incident signature
/// (cf2e7db), so the probe keeps a wide margin against both false reds and
/// the regression it exists to catch.
const P99_THRESHOLD_MS: u128 = 20;
const SAMPLES: usize = 200;
const TIMEOUT: Duration = Duration::from_secs(30);

/// Gate: probes refuse to run without explicit isolation. exit(1) is
/// intentional — the whole test binary must end non-zero so the executor's
/// gate check is impossible to miss.
fn require_isolation_gate() {
    if std::env::var("CV_E2E_ISOLATED").as_deref() != Ok("1") {
        eprintln!(
            "REFUSED: hook_latency_probe sends real synthetic mouse input and \
             installs a real WH_MOUSE_LL hook. It requires the isolation gate \
             CV_E2E_ISOLATED=1 and must be run via scripts/run-native-probes.ps1 \
             (stop production instance → preserve clipboard → probe → restore → \
             restart & verify). See TESTING_ARCHITECTURE_PLAN §1.3-3."
        );
        std::process::exit(1);
    }
}

static RECEIVED: AtomicUsize = AtomicUsize::new(0);

unsafe extern "system" fn hook_proc(
    code: i32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    if code >= 0 {
        // record the receipt of every mouse event we are shown
        RECEIVED.fetch_add(1, Ordering::SeqCst);
    }
    unsafe { windows::Win32::UI::WindowsAndMessaging::CallNextHookEx(None, code, wparam, lparam) }
}

#[test]
#[ignore = "real synthetic input + real WH_MOUSE_LL hook; only via \
            scripts/run-native-probes.ps1 behind CV_E2E_ISOLATED=1"]
fn hook_pump_delivers_synthetic_mouse_events_within_p99_threshold() {
    require_isolation_gate();

    use windows::Win32::UI::Input::KeyboardAndMouse::SendInput;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowsHookExW, UnhookWindowsHookEx, HOOKPROC, WH_MOUSE_LL,
    };

    static GAPS: Mutex<Vec<u128>> = Mutex::new(Vec::new());
    let hook_proc_fn: HOOKPROC = Some(hook_proc);

    let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, hook_proc_fn, None, 0) }
        .expect("WH_MOUSE_LL install must succeed on an interactive desktop");
    // unconditional unhook — the system must never keep our hook
    struct Guard(windows::Win32::UI::WindowsAndMessaging::HHOOK);
    impl Drop for Guard {
        fn drop(&mut self) {
            unsafe {
                let _ = UnhookWindowsHookEx(self.0);
            }
        }
    }
    let _guard = Guard(hook);

    let mut last_sent = Instant::now();
    for i in 0..SAMPLES {
        // relative move: no cursor jump on screen, still dispatches through
        // the low-level hook chain
        let input = windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
            r#type: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_MOUSE,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                mi: windows::Win32::UI::Input::KeyboardAndMouse::MOUSEINPUT {
                    dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_MOVE,
                    dx: if i % 2 == 0 { 1 } else { -1 },
                    dy: 0,
                    mouseData: 0,
                    dwExtraInfo: 0,
                    time: 0,
                },
            },
        };
        RECEIVED.store(i, Ordering::SeqCst);
        let sent_at = Instant::now();
        let sent = unsafe {
            SendInput(
                &[input],
                std::mem::size_of::<windows::Win32::UI::Input::KeyboardAndMouse::INPUT>() as i32,
            )
        };
        assert_eq!(sent as usize, 1, "SendInput must accept the event");

        // wait for the hook to see it, measuring the dispatch gap
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let now = RECEIVED.load(Ordering::SeqCst);
            if now > i {
                let gap = sent_at.duration_since(last_sent).as_millis();
                GAPS.lock().unwrap().push(gap);
                last_sent = sent_at;
                break;
            }
            if Instant::now() > deadline {
                panic!(
                    "hook callback did not observe event {} within {TIMEOUT:?} — \
                     the pump is stalled (incident #1 signature)",
                    i + 1
                );
            }
            std::thread::sleep(Duration::from_micros(200));
        }
    }

    let mut gaps = GAPS.lock().unwrap().clone();
    assert!(gaps.len() >= SAMPLES - 1, "expected a gap sample per event");
    gaps.sort_unstable();
    let p99 = gaps[(gaps.len() as f64 * 0.99) as usize % gaps.len()];
    let p50 = gaps[gaps.len() / 2];
    eprintln!(
        "[hook_latency_probe] samples={} p50={}ms p99={}ms threshold={}ms",
        gaps.len(),
        p50,
        p99,
        P99_THRESHOLD_MS
    );
    assert!(
        p99 < P99_THRESHOLD_MS,
        "gap P99 {p99}ms >= threshold {P99_THRESHOLD_MS}ms — input latency \
         regression matching incident #1 (cf2e7db)"
    );
}
