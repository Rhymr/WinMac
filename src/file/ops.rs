use futures_util::StreamExt;
use glib::MainContext;
use gtk::prelude::*;
use gtk::{FileDialog, Window};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct FileOps;

impl FileOps {
    pub fn open_file(parent_window: Option<Window>) -> Option<(PathBuf, String)> {
        let context = MainContext::default();
        let (sender, mut receiver) = futures_channel::mpsc::channel(1);
        let sender = Arc::new(Mutex::new(sender));

        context.spawn_local({
            let sender = sender.clone();
            async move {
                let dialog = FileDialog::new();
                dialog.set_title("Open File");
                dialog.set_modal(true);

                if let Ok(file) = dialog.open_future(parent_window.as_ref()).await
                    && let Some(path) = file.path()
                {
                    match fs::read_to_string(&path) {
                        Ok(content) => {
                            log::debug!("open dialog: reading {path:?}");
                            let _ = sender.lock().unwrap().try_send(Some((path, content)));
                            return;
                        }
                        Err(e) => log::warn!("open dialog: could not read {path:?}: {e}"),
                    }
                }
                let _ = sender.lock().unwrap().try_send(None);
            }
        });

        // Run the context until we get a response
        context.block_on(async { receiver.next().await }).flatten()
    }

    pub fn save_file(
        content: String,
        path: Option<PathBuf>,
        parent_window: Option<Window>,
    ) -> Option<PathBuf> {
        if let Some(path) = path {
            match fs::write(&path, &content) {
                Ok(()) => {
                    log::debug!("saved {path:?}");
                    return Some(path);
                }
                Err(e) => log::warn!("save to {path:?} failed, prompting for a location: {e}"),
            }
        }

        let context = MainContext::default();
        let (sender, mut receiver) = futures_channel::mpsc::channel(1);
        let sender = Arc::new(Mutex::new(sender));

        context.spawn_local({
            let sender = sender.clone();
            async move {
                let dialog = FileDialog::new();
                dialog.set_title("Save File");
                dialog.set_modal(true);

                if let Ok(file) = dialog.save_future(parent_window.as_ref()).await
                    && let Some(path) = file.path()
                {
                    match fs::write(&path, &content) {
                        Ok(()) => {
                            log::debug!("save dialog: wrote {path:?}");
                            let _ = sender.lock().unwrap().try_send(Some(path));
                            return;
                        }
                        Err(e) => log::warn!("save dialog: could not write {path:?}: {e}"),
                    }
                }
                let _ = sender.lock().unwrap().try_send(None);
            }
        });

        // Run the context until we get a response
        context.block_on(async { receiver.next().await }).flatten()
    }
}
