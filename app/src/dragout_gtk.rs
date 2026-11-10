// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dragging 3DS files out to the Linux file manager (Nautilus, Dolphin, Thunar,
//! the desktop): GTK hands them over as file URIs (text/uri-list).
//!
//! Unlike Windows (dragout.rs), the download starts as soon as the drag leaves
//! the window: file managers only wait a few seconds for a drop's data (Qt) or
//! give up after 30 s without an answer (GTK), and Nautilus asks for it while
//! hovering already. When the data is asked for, Baguette waits for the
//! download to end, the window still painting, then hands the temp files over.
//! A drag dropped nowhere (or back on Baguette) stops its download and leaves
//! nothing behind.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use baguette_core::ftp::Entry;
use baguette_core::util::safe_name;
use baguette_core::os;
use baguette_core::workers::JobPtr;
use gtk::prelude::*;
use gtk::{gdk, glib};
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

/// what a drop hands over: one file:// URI per dragged item
pub fn uri_list(files: &[String]) -> Vec<String> {
    files.iter().map(|f| os::file_uri(f)).collect()
}

/// is the drop going to `own` (Baguette's window)? gdk-rs' dest_window() can't
/// say "none", which happens on Wayland
fn onto(ctx: &gdk::DragContext, own: Option<&gdk::Window>) -> bool {
    use glib::translate::ToGlibPtr;
    let Some(own) = own else { return false };
    unsafe {
        let dest = gdk::ffi::gdk_drag_context_get_dest_window(ctx.to_glib_none().0);
        !dest.is_null() && gdk::ffi::gdk_window_get_toplevel(dest) == own.to_glib_none().0
    }
}

/// the page waits for this before it takes drops again
fn finished(app: &AppHandle) {
    let _ = app.emit("dragout-done", ());
}

/// stops this drag's download, and only it (other queued transfers stay), then
/// deletes its temp folder once the worker has let go of it
fn give_up(state: &AppState, tab: u32, job: &JobPtr, dir: &str) {
    if !job.is_finished() {
        state.with_client(tab, |c| {
            if c.shared.xfer.drop_job(job) {
                c.shared.cancel.store(true, Ordering::Relaxed);
            }
        });
    }
    let (job, dir) = (job.clone(), dir.to_string());
    std::thread::spawn(move || {
        job.wait();
        let _ = std::fs::remove_dir_all(&dir);
    });
}

/// runs on the GTK thread and returns at once: the drag goes on in GTK's loop
pub fn drag_out(app: &AppHandle, state: Arc<AppState>, tab: u32, remote_dir: &str, mut entries: Vec<Entry>) {
    let Some(window) = app.get_webview_window("main").and_then(|w| w.gtk_window().ok()) else { return finished(app) };
    entries.retain(|e| safe_name(&e.name));
    if entries.is_empty() {
        return finished(app);
    }
    let icon = if entries.len() == 1 && !entries[0].dir { "text-x-generic" } else { "folder" };
    let (dir, files) = crate::drag_paths(&entries);
    let _ = std::fs::create_dir_all(&dir);
    let Some(job) = crate::start_download(&state, tab, remote_dir, entries, dir.clone(), false) else {
        let _ = std::fs::remove_dir(&dir);
        return finished(app);
    };
    let uris = uri_list(&files);
    let delivered = Rc::new(Cell::new(false));
    let waiting = Rc::new(Cell::new(false));
    let own = window.window();

    let source = gtk::Invisible::new();
    {
        let (state, job, delivered) = (state.clone(), job.clone(), delivered.clone());
        source.connect_drag_data_get(move |_, ctx, data, _, _| {
            // back on Baguette: nothing to give (the page would upload them again)
            if waiting.get() || onto(ctx, own.as_ref()) {
                return;
            }
            if !job.is_finished() {
                state.message(tab, "Downloading for the drop...".into());
            }
            waiting.set(true);
            while !job.is_finished() {
                while gtk::events_pending() {
                    gtk::main_iteration_do(false);
                }
                job.wait_timeout(Duration::from_millis(15));
            }
            waiting.set(false);
            if job.wait() {
                let refs: Vec<&str> = uris.iter().map(String::as_str).collect();
                delivered.set(data.set_uris(&refs));
            }
        });
    }
    {
        let (app, state, job, dir, delivered) = (app.clone(), state.clone(), job.clone(), dir.clone(), delivered.clone());
        source.connect_drag_end(move |w, _| {
            if !delivered.get() {
                give_up(&state, tab, &job, &dir);
            }
            let w = w.clone();
            glib::idle_add_local_once(move || unsafe { w.destroy() });
            finished(&app);
        });
    }
    let targets = gtk::TargetList::new(&[gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::OTHER_APP, 0)]);
    match source.drag_begin_with_coordinates(&targets, gdk::DragAction::COPY, 1, None, -1, -1) {
        Some(ctx) => ctx.drag_set_icon_name(icon, 0, 0),
        None => {
            give_up(&state, tab, &job, &dir);
            state.message(tab, "Could not start dragging to the file manager".into());
            unsafe { source.destroy() };
            finished(app);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_for_the_drop() {
        let entries = vec![
            Entry { name: "Game é.cia".into(), ..Default::default() },
            Entry { name: "saves".into(), dir: true, ..Default::default() },
        ];
        let (dir, files) = crate::drag_paths(&entries);
        assert!(dir.starts_with(&os::temp_root()) && dir.len() > os::temp_root().len() + 1, "{dir}");
        assert_eq!(files, vec![format!("{dir}/Game é.cia"), format!("{dir}/saves")]);
        let uris = uri_list(&files);
        assert_eq!(uris[0], format!("file://{}/Game%20%C3%A9.cia", dir));
        assert!(uris.iter().all(|u| u.starts_with("file:///")));
    }
}
