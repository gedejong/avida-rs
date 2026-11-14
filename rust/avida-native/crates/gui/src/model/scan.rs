//! The subset's time series over a whole recording.
//!
//! A live run keeps no past per organism, so the subset's line starts when
//! the filter is set. A recording holds every frame, so a worker thread
//! opens the file again, applies the filter to each frame and sends one
//! [`SubsetRow`] per frame, a few at a time. It reads only; the playback and
//! the files are untouched. Replacing or clearing the filter stops it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;

use avida_session::Recording;
use avida_viewmodel::{subset_row, Filter, SubsetRow};

/// Rows sent per message.
const BATCH: usize = 4;

enum Message {
    Rows(Vec<SubsetRow>),
    Done,
    Failed(String),
}

/// A running scan.
#[derive(Debug)]
pub struct PlaybackScan {
    rx: Receiver<Message>,
    cancel: Arc<AtomicBool>,
    done: bool,
    failed: Option<String>,
}

impl PlaybackScan {
    /// Starts reading `path` with `filter` on a worker thread.
    pub fn start(path: PathBuf, filter: Filter) -> PlaybackScan {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        std::thread::Builder::new()
            .name("filter-scan".into())
            .spawn(move || {
                let mut rec = match Recording::open(&path) {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(Message::Failed(e.to_string()));
                        return;
                    }
                };
                let mut batch = Vec::with_capacity(BATCH);
                for i in 0..rec.len() {
                    if flag.load(Ordering::Relaxed) {
                        return;
                    }
                    match rec.frame(i) {
                        Ok(f) => {
                            if let Some(map) = &f.map {
                                let sel = filter.select(map);
                                batch.push(subset_row(f.update, map, &sel));
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Message::Failed(e.to_string()));
                            return;
                        }
                    }
                    if batch.len() >= BATCH
                        && tx.send(Message::Rows(std::mem::take(&mut batch))).is_err()
                    {
                        return;
                    }
                }
                if !batch.is_empty() {
                    let _ = tx.send(Message::Rows(batch));
                }
                let _ = tx.send(Message::Done);
            })
            .ok();
        PlaybackScan {
            rx,
            cancel,
            done: false,
            failed: None,
        }
    }

    /// The rows that arrived since the last call.
    pub fn poll(&mut self) -> Vec<SubsetRow> {
        let mut rows = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(Message::Rows(r)) => rows.extend(r),
                Ok(Message::Done) => self.done = true,
                Ok(Message::Failed(why)) => {
                    self.done = true;
                    self.failed = Some(why);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.done = true;
                    break;
                }
            }
        }
        rows
    }

    /// Whether the worker has finished (or failed).
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Why the worker stopped early, if it did.
    pub fn failure(&self) -> Option<&str> {
        self.failed.as_deref()
    }
}

impl Drop for PlaybackScan {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
