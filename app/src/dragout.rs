// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! Dragging 3DS files out to Explorer. The drag offers temp-file paths
//! (CF_HDROP); the files are only downloaded when the mouse button is released
//! over a drop target outside Chomp, then the drop goes through.

use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::time::Duration;

use chomp_core::ftp::Entry;
use chomp_core::win;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::Shell::{DROPFILES, SHCreateStdEnumFmtEtc};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, HRESULT, Result, implement};

use crate::AppState;

const CF_HDROP: u16 = 15;
const MK_LBUTTON: u32 = 1;

fn hdrop_format() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

#[implement(IDataObject)]
struct DataObject {
    files: Vec<String>,
}

impl IDataObject_Impl for DataObject_Impl {
    fn GetData(&self, fe: *const FORMATETC) -> Result<STGMEDIUM> {
        if self.QueryGetData(fe) != S_OK {
            return Err(DV_E_FORMATETC.into());
        }
        let mut chars: Vec<u16> = Vec::new();
        for f in &self.files {
            chars.extend(f.encode_utf16());
            chars.push(0);
        }
        chars.push(0);
        let bytes = size_of::<DROPFILES>() + chars.len() * 2;
        unsafe {
            let h = GlobalAlloc(GHND, bytes)?;
            let p = GlobalLock(h) as *mut u8;
            let df = p as *mut DROPFILES;
            (*df).pFiles = size_of::<DROPFILES>() as u32;
            (*df).fWide = TRUE;
            std::ptr::copy_nonoverlapping(chars.as_ptr() as *const u8, p.add(size_of::<DROPFILES>()), chars.len() * 2);
            let _ = GlobalUnlock(h);
            Ok(STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: h },
                pUnkForRelease: ManuallyDrop::new(None),
            })
        }
    }

    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, fe: *const FORMATETC) -> HRESULT {
        let ok = unsafe { fe.as_ref() }.is_some_and(|fe| fe.cfFormat == CF_HDROP && fe.tymed & TYMED_HGLOBAL.0 as u32 != 0);
        if ok { S_OK } else { DV_E_FORMATETC }
    }

    fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, out: *mut FORMATETC) -> HRESULT {
        if let Some(out) = unsafe { out.as_mut() } {
            out.ptd = std::ptr::null_mut();
        }
        E_NOTIMPL
    }

    fn SetData(&self, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, dir: u32) -> Result<IEnumFORMATETC> {
        if dir != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        unsafe { SHCreateStdEnumFmtEtc(&[hdrop_format()]) }
    }

    fn DAdvise(&self, _: *const FORMATETC, _: u32, _: windows::core::Ref<'_, IAdviseSink>) -> Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _: u32) -> Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

#[implement(IDropSource)]
struct DropSource {
    effect: Cell<DROPEFFECT>,
    on_drop: RefCell<Box<dyn FnMut() -> bool>>,
}

impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() {
            return DRAGDROP_S_CANCEL;
        }
        if keys.0 & MK_LBUTTON != 0 {
            return S_OK;
        }
        // button released: only download if a real target outside this app accepts it
        let mut pt = POINT::default();
        let mut pid = 0u32;
        unsafe {
            let _ = GetCursorPos(&mut pt);
            GetWindowThreadProcessId(WindowFromPoint(pt), Some(&mut pid));
        }
        if self.effect.get() == DROPEFFECT_NONE || pid == std::process::id() {
            return DRAGDROP_S_CANCEL;
        }
        if (self.on_drop.borrow_mut())() { DRAGDROP_S_DROP } else { DRAGDROP_S_CANCEL }
    }

    fn GiveFeedback(&self, effect: DROPEFFECT) -> HRESULT {
        self.effect.set(effect);
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// downloads into dir while the window keeps painting; Esc cancels
fn download_modal(state: &AppState, tab: u32, remote_dir: &str, entries: Vec<Entry>, dir: &str) -> bool {
    let Some(job) = crate::start_download(state, tab, remote_dir, entries, dir.to_string(), false) else { return false };
    state.message(tab, "Downloading... (Esc to cancel)".into());
    let mut cancelled = false;
    while !job.is_finished() {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_KEYDOWN && msg.wParam.0 == VK_ESCAPE.0 as usize {
                    cancelled = true;
                    state.with_client(tab, |c| {
                        c.shared.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                        c.shared.xfer.clear_pending();
                    });
                }
                let input = (WM_KEYFIRST..=WM_KEYLAST).contains(&msg.message) || (WM_MOUSEFIRST..=WM_MOUSELAST).contains(&msg.message);
                if !input {
                    DispatchMessageW(&msg);
                }
            }
        }
        job.wait_timeout(Duration::from_millis(15));
    }
    job.wait() && !cancelled
}

/// runs on the UI thread: the drag loop is modal
pub fn drag_out(state: &AppState, tab: u32, remote_dir: &str, entries: Vec<Entry>) {
    if entries.is_empty() {
        return;
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let dir = format!("{}\\{stamp}", win::temp_root());
    let _ = std::fs::create_dir_all(&dir);
    let files: Vec<String> = entries.iter().map(|e| format!("{dir}\\{}", e.name)).collect();

    let fetched = std::rc::Rc::new(Cell::new(false));
    let f = fetched.clone();
    let d = dir.clone();
    let rd = remote_dir.to_string();
    // the closure runs inside DoDragDrop, on this thread, while `state` is borrowed
    let state_ptr = state as *const AppState;
    let data: IDataObject = DataObject { files }.into();
    let source: IDropSource = DropSource {
        effect: Cell::new(DROPEFFECT_NONE),
        on_drop: RefCell::new(Box::new(move || {
            f.set(true);
            download_modal(unsafe { &*state_ptr }, tab, &rd, entries.clone(), &d)
        })),
    }
    .into();
    let mut effect = DROPEFFECT_NONE;
    unsafe {
        let _ = DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect);
    }
    if !fetched.get() {
        let _ = std::fs::remove_dir(&dir);
    }
}
