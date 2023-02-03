use guardian::ArcMutexGuardian;
use std::sync::{Arc, Mutex};

pub struct Database {
    clips: Arc<Mutex<Vec<Clip>>>,
    selection: Arc<Mutex<Option<Clip>>>,
}
// TODO: Max amount
// Searching (fuzzy)

impl Database {
    pub fn new() -> Database {
        Database {
            clips: Arc::new(Mutex::new(Vec::new())),
            selection: Arc::new(Mutex::new(None)),
        }
    }

    pub fn add_clip(&self, clip: Clip) -> usize {
        let mut clips = self.clips.lock().unwrap();
        clips.push(clip);
        clips.len() - 1
    }

    pub fn clips(&self) -> ArcMutexGuardian<Vec<Clip>> {
        ArcMutexGuardian::take(Arc::clone(&self.clips)).unwrap()
    }

    pub fn at(&self, idx: usize) -> Option<Clip> {
        self.clips.lock().unwrap()
            .get(idx as usize)
            .cloned()
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
    use crate::db::{Clip, ClipContents, Database, Source};

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
}