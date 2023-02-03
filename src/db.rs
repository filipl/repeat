use std::collections::VecDeque;
use guardian::ArcMutexGuardian;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use fuzzy_matcher::clangd::fuzzy_match;

const MAX_CLIPS: usize = 100;

pub struct Database {
    clips: Arc<Mutex<VecDeque<Clip>>>,
    selection: Arc<Mutex<Option<Clip>>>,
    start_idx: AtomicUsize,
}

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

    pub fn search(&self, pattern: &str, max: usize) -> Vec<Clip> {
        let clips = self.clips.lock().unwrap();
        let mut matched_clips: Vec<(usize, i64)> = clips.iter().enumerate().filter_map(|(idx, clip)| {
            match &clip.contents {
                ClipContents::Text(content) => {
                    match fuzzy_match(content, pattern) {
                        None => { None }
                        Some(score) => { Some((idx, score)) }
                    }
                }
            }
        }).collect();
        matched_clips.sort_by_key(|(_, score)| { score.clone() });
        matched_clips.iter().take(max)
            .flat_map(|(idx, _)| { clips.get(*idx).cloned() })
            .collect()
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
        Database::new();
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
        for i in 1..(MAX_CLIPS * 3) {
            let clip = Clip { source: Source::Primary, contents: ClipContents::Text(format!("clip {}", i)) };
            let idx = db.add_clip(clip.clone());
            assert_eq!(db.at(idx).unwrap(), clip);
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

    #[test]
    fn search() {
        let db = Database::new();

        let fst = Clip { source: Source::Primary, contents: ClipContents::Text("fst string".to_owned()) };
        db.add_clip(fst.clone());
        let snd = Clip { source: Source::Secondary, contents: ClipContents::Text("second string".to_owned()) };
        db.add_clip(snd.clone());

        {
            let matches = db.search("fst", 5);
            assert_eq!(matches.len(), 1);
            assert_eq!(matches.first().unwrap().clone(), fst);
        }

        {
            let matches = db.search("string", 5);
            assert_eq!(matches.len(), 2);
            assert_eq!(matches.first().unwrap().clone(), fst);
        }
    }
}