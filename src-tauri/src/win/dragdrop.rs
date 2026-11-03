//! Native OLE drag-out (spike: CF_UNICODETEXT only). A hand-written minimal
//! IDataObject (EnumFormatEtc via the shell's standard enumerator) plus a
//! minimal IDropSource (default cursors, drop on button release, cancel on
//! ESC). DoDragDrop runs on a DEDICATED CoInitializeEx(STA) thread so the
//! main/WebView2 threads never block during the drag.
//!
//! This is the user-initiated native drag path (P1 of the 0.2 task list) —
//! not synthetic input: the OS tracks the real mouse, the target app decides
//! whether and where to accept the drop.
#![allow(non_snake_case)]

use std::sync::mpsc;
use windows::core::{implement, IUnknown};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::SHCreateStdEnumFmtEtc;

/// Outcome of a completed native drag session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOutcome {
    /// The user dropped onto a target that accepted the data.
    Dropped,
    /// The user cancelled (ESC / released outside a valid target).
    Cancelled,
    /// The drag never started (COM/thread failure).
    Failed,
}

/// Runs a native drag of `text` (CF_UNICODETEXT) from a dedicated STA thread.
/// Blocks the CALLER until the drag ends (the async command wraps this off the
/// main thread), never the OS.
pub fn run_text_drag(text: String) -> DragOutcome {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("cv-drag-out".into())
        .spawn(move || {
            let outcome = unsafe {
                let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                if hr.is_err() {
                    // RPC_E_CHANGED_MODE etc. — cannot run an STA drag here
                    DragOutcome::Failed
                } else {
                    run_drag_on_this_sta_thread(hglobal_wide_string(&text))
                }
            };
            let _ = tx.send(outcome);
        })
        .ok();
    rx.recv().unwrap_or(DragOutcome::Failed)
}

unsafe fn run_drag_on_this_sta_thread(payload: *mut core::ffi::c_void) -> DragOutcome {
    let data: IDataObject = TextDataObject::new(payload).into();
    let source: IDropSource = DropSource.into();
    let mut effect = DROPEFFECT::default();
    let hr = DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect);
    let _ = CoUninitialize();
    if hr == DRAGDROP_S_DROP {
        DragOutcome::Dropped
    } else if hr == DRAGDROP_S_CANCEL {
        DragOutcome::Cancelled
    } else {
        DragOutcome::Failed
    }
}

/// UTF-16LE with NUL terminator inside a GMEM_MOVEABLE HGLOBAL — the exact
/// medium CF_UNICODETEXT consumers expect.
fn hglobal_wide_string(text: &str) -> *mut core::ffi::c_void {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let bytes = wide.len() * 2;
    unsafe {
        let Ok(h) = GlobalAlloc(GMEM_MOVEABLE, bytes) else {
            return std::ptr::null_mut();
        };
        let dst = GlobalLock(h);
        if dst.is_null() {
            let _ = GlobalFree(h);
            return std::ptr::null_mut();
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr().cast::<u8>(), dst.cast(), bytes);
        let _ = GlobalUnlock(h);
        h.0
    }
}

fn format_is_text(fmt: *const FORMATETC) -> bool {
    let f = unsafe { &*fmt };
    f.cfFormat == CF_UNICODETEXT.0 && (f.tymed & TYMED_HGLOBAL.0 as u32) != 0
}

// ---------- minimal IDataObject ----------

#[implement(IDataObject)]
struct TextDataObject {
    /// HGLOBAL owned for the object's lifetime (freed in Drop).
    payload: *mut core::ffi::c_void,
}

impl TextDataObject {
    fn new(payload: *mut core::ffi::c_void) -> Self {
        Self { payload }
    }
}

impl Drop for TextDataObject {
    fn drop(&mut self) {
        if !self.payload.is_null() {
            unsafe {
                let _ = GlobalFree(HGLOBAL(self.payload));
            }
        }
    }
}

impl IDataObject_Impl for TextDataObject_Impl {
    fn GetData(&self, _pformatetcin: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        unsafe {
            if self.payload.is_null() {
                return Err(windows::core::Error::from(DV_E_FORMATETC));
            }
            // hand out a COPY: pUnkForRelease=None means the consumer frees the
            // medium, and two GetData calls must not hand out the same handle.
            let bytes = GlobalSize(HGLOBAL(self.payload));
            let copy: HGLOBAL =
                GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(windows::core::Error::from)?;
            let src = GlobalLock(HGLOBAL(self.payload));
            let dst = GlobalLock(copy);
            if src.is_null() || dst.is_null() {
                let _ = GlobalUnlock(copy);
                let _ = GlobalFree(Some(&copy));
                return Err(E_OUTOFMEMORY.into());
            }
            std::ptr::copy_nonoverlapping(src.cast::<u8>(), dst.cast::<u8>(), bytes);
            let _ = GlobalUnlock(HGLOBAL(self.payload));
            let _ = GlobalUnlock(copy);
            Ok(STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: copy },
                pUnkForRelease: std::mem::ManuallyDrop::new(None::<IUnknown>),
            })
        }
    }

    fn GetDataHere(
        &self,
        pformatetc: *const FORMATETC,
        pmedium: *mut STGMEDIUM,
    ) -> windows::core::Result<()> {
        // minimal: consumers that pre-allocate media are rare; not supported
        let _ = (pformatetc, pmedium);
        Err(windows::core::Error::from(E_NOTIMPL))
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> windows::core::HRESULT {
        if format_is_text(pformatetc) {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        pformatectin: *const FORMATETC,
        pformatetcout: *mut FORMATETC,
    ) -> windows::core::HRESULT {
        unsafe {
            *pformatetcout = *pformatectin;
            (*pformatetcout).ptd = std::ptr::null_mut();
        }
        DATA_S_SAMEFORMATETC
    }

    fn SetData(
        &self,
        pformatetc: *const FORMATETC,
        pmedium: *const STGMEDIUM,
        frelease: BOOL,
    ) -> windows::core::Result<()> {
        let _ = (pformatetc, pmedium, frelease);
        Err(windows::core::Error::from(E_NOTIMPL))
    }

    fn EnumFormatEtc(&self, dwdirection: u32) -> windows::core::Result<IEnumFORMATETC> {
        if dwdirection != DATADIR_GET.0 as u32 {
            return Err(windows::core::Error::from(E_NOTIMPL));
        }
        let formats = [FORMATETC {
            cfFormat: CF_UNICODETEXT.0,
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
            ptd: std::ptr::null_mut(),
        }];
        unsafe { SHCreateStdEnumFmtEtc(&formats) }
    }

    fn DAdvise(
        &self,
        pformatetc: *const FORMATETC,
        advf: u32,
        padvsink: Option<&IAdviseSink>,
    ) -> windows::core::Result<u32> {
        let _ = (pformatetc, advf, padvsink);
        Err(windows::core::Error::from(OLE_E_ADVISENOTSUPPORTED))
    }

    fn DUnadvise(&self, dwconnection: u32) -> windows::core::Result<()> {
        let _ = dwconnection;
        Err(windows::core::Error::from(OLE_E_NOCONNECTION))
    }

    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(windows::core::Error::from(OLE_E_ADVISENOTSUPPORTED))
    }
}

// ---------- minimal IDropSource ----------

#[implement(IDropSource)]
struct DropSource;

impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(
        &self,
        fescapepressed: BOOL,
        grfkeystate: MODIFIERKEYS_FLAGS,
    ) -> windows::core::HRESULT {
        if fescapepressed.as_bool() {
            return DRAGDROP_S_CANCEL;
        }
        // spike contract: the drag is initiated with the LEFT button held
        // (the frontend starts the native drag inside its own mousedown
        // handler); releasing it ends the session with a drop.
        if grfkeystate.0 & 0x0001 == 0 {
            // 0x0001 == MK_LBUTTON
            return DRAGDROP_S_DROP;
        }
        S_OK
    }

    fn GiveFeedback(&self, dweffect: DROPEFFECT) -> windows::core::HRESULT {
        let _ = dweffect;
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

#[cfg(test)]
mod dragdrop_tests {
    use super::*;

    #[test]
    fn outcome_states_are_distinct() {
        assert_ne!(DragOutcome::Dropped, DragOutcome::Cancelled);
        assert_ne!(DragOutcome::Cancelled, DragOutcome::Failed);
        assert_ne!(DRAGDROP_S_CANCEL, DRAGDROP_S_DROP);
        assert_ne!(DRAGDROP_S_DROP, S_OK);
    }
}
