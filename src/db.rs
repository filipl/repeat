use std::collections::VecDeque;
use guardian::ArcMutexGuardian;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

const MAX_CLIPS: usize = 100;

pub struct Database {
    clips: Arc<Mutex<VecDeque<Clip>>>,
    selection: Arc<Mutex<Option<Clip>>>,
    start_idx: AtomicUsize,
}
// Searching (fuzzy)

impl Database {
    pub fn new() -> Database {
        Database {
            clips: Arc::new(Mutex::new(VecDeque::new())),
            selection: Arc::new(Mutex::new(None)),
            start_idx: AtomicUsize::new(0),
        }
    }

    pub fn add_clip(&self, clip: Clip) -> usize {
        let mut clips = self.clips.lock().unwrap();
        clips.push_back(clip);
        if clips.len() > MAX_CLIPS {
            clips.pop_front();
            self.start_idx.fetch_add(1, Ordering::Acquire);
        }
        clips.len() + (self.start_idx.load(Ordering::Acquire)) - 1
    }

    pub fn clips(&self) -> ArcMutexGuardian<VecDeque<Clip>> {
        ArcMutexGuardian::take(Arc::clone(&self.clips)).unwrap()
    }

    pub fn at(&self, idx: usize) -> Option<Clip> {
        let clips = self.clips.lock().unwrap();
        let start = self.start_idx.load(Ordering::Acquire);
        if idx < start {
            None
        } else {
            clips
                .get(idx - start)
                .cloned()
        }
    }

    pub fn selection(&self) -> Option<Clip> {
        self.selection.lock().unwrap().clone()
    }

    pub fn select(&self, idx: usize) -> bool {
        match self.at(idx) {
            None => false,
            Some(clip) => {
                *self.selection.lock().unwrap() = Some(clip);
                true
            }
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Clip {
    pub source: Source,
    pub contents: ClipContents,
}

#[derive(Clone, PartialEq, Debug)]
pub enum ClipContents {
    Text(String),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Source {
    Primary,
    Secondary,
    Clipboard,
}

#[cfg(test)]
mod tests {
    use crate::db::{Clip, ClipContents, Database, MAX_CLIPS, Source};

    #[test]
    fn creating() {
        let db = Database::new();
    }

    #[test]
    fn add_and_get() {
        let db = Database::new();
        let fst = Clip { source: Source::Primary, contents: ClipContents::Text("fst string".to_owned()) };
        let snd = Clip { source: Source::Secondary, contents: ClipContents::Text("second string".to_owned()) };

        let fst_idx = db.add_clip(fst.clone());
        assert_eq!(fst_idx, 0);

        let snd_idx = db.add_clip(snd.clone());
        assert_eq!(snd_idx, 1);

        assert_eq!(db.at(fst_idx).unwrap(), fst);
        assert_eq!(db.at(snd_idx).unwrap(), snd);
        assert!(db.at(2).is_none());
    }

    #[test]
    fn select() {
        let db = Database::new();
        let fst = Clip { source: Source::Primary, contents: ClipContents::Text("fst string".to_owned()) };
        let snd = Clip { source: Source::Secondary, contents: ClipContents::Text("second string".to_owned()) };

        let fst_idx = db.add_clip(fst.clone());
        db.add_clip(snd.clone());

        assert!(db.select(fst_idx));

        assert_eq!(db.selection().unwrap(), fst);
    }

    #[test]
    fn rolling() {
        let db = Database::new();
        let mut last_idx = 0;

        for i in 1..(MAX_CLIPS * 2) {
            let clip = Clip { source: Source::Primary, contents: ClipContents::Text(format!("clip {}", i)) };
            last_idx = db.add_clip(clip.clone());
            assert_eq!(db.at(last_idx).unwrap(), clip);
        }

        assert!(db.at(0).is_none());
        assert_eq!(db.clips().iter().count(), MAX_CLIPS);
    }

    #[test]
    fn selection_stays_after_roll() {
        let db = Database::new();

        let fst = Clip { source: Source::Primary, contents: ClipContents::Text("fst string".to_owned()) };
        let fst_idx = db.add_clip(fst.clone());
        assert!(db.select(fst_idx));

        for i in 1..(MAX_CLIPS * 2) {
            let clip = Clip { source: Source::Primary, contents: ClipContents::Text(format!("clip {}", i)) };
            let idx = db.add_clip(clip.clone());
            assert_eq!(db.at(idx).unwrap(), clip);
        }

        assert_eq!(db.selection().unwrap(), fst);
    }
}