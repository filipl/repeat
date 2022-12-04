use guardian::ArcMutexGuardian;
use std::sync::{Arc, Mutex};

pub struct Database {
    clips: Arc<Mutex<Vec<Clip>>>,
}

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
}

pub struct Clip {
    pub source: Source,
    pub contents: ClipContents,
}

pub enum ClipContents {
    Text(String),
}

pub enum Source {
    Primary,
    Secondary,
    Clipboard,
}
