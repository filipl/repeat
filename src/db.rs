use guardian::ArcMutexGuardian;
use std::sync::{Arc, Mutex};

pub struct Database {
    clips: Arc<Mutex<Vec<Clip>>>,
}
// TODO: Max amount
// Searching (fuzzy)
// Selection

impl Database {
    pub fn new() -> Database {
        Database {
            clips: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn add_clip(&self, clip: Clip) {
        self.clips.lock().unwrap().push(clip);
    }

    pub fn clips(&self) -> ArcMutexGuardian<Vec<Clip>> {
        ArcMutexGuardian::take(Arc::clone(&self.clips)).unwrap()
    }

    pub fn selection(&self) -> Option<Clip> {
        self.clips.lock().unwrap().last().map(|c| c.clone())
    }
}

#[derive(Clone)]
pub struct Clip {
    pub source: Source,
    pub contents: ClipContents,
}

#[derive(Clone)]
pub enum ClipContents {
    Text(String),
}

#[derive(Clone)]
pub enum Source {
    Primary,
    Secondary,
    Clipboard,
}
