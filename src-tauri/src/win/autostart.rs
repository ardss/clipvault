//! Autostart entry in HKCU Run.
use super::*;

// ---------- autostart (HKCU Run) ----------

pub fn set_autostart(enable: bool) -> bool {
    use windows::Win32::System::Registry::*;
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let name: Vec<u16> = "ClipVault"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut hk = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            0,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            &mut hk,
        )
        .is_err()
        {
            return false;
        }
        let ok = if enable {
            let exe = std::env::current_exe().unwrap_or_default();
            let cmd: Vec<u16> = format!("\"{}\" /autostart", exe.display())
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            RegSetValueExW(
                hk,
                PCWSTR(name.as_ptr()),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(
                    cmd.as_ptr() as *const u8,
                    cmd.len() * 2,
                )),
            )
            .is_ok()
        } else {
            // "value already gone" counts as success
            let r = RegDeleteValueW(hk, PCWSTR(name.as_ptr()));
            r == NO_ERROR || r == ERROR_FILE_NOT_FOUND
        };
        let _ = RegCloseKey(hk);
        ok
    }
}
