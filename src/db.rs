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

    pub fn add_clip(&self, clip: Clip) -> Option<usize> {
        let mut clips = self.clips.lock().unwrap();

        // see if it's a greater version of the previous clip
        let replace = match clips.back() {
            None => false,
            Some(latest_clip) => clip.contains(latest_clip),
        };
        if replace {
            clips.pop_back();
        }

        // see if it's already in the database
        if clips.iter().find(|c| c.contents.eq(&clip.contents)).is_some() {
            return None;
        }

        clips.push_back(clip);
        if clips.len() > MAX_CLIPS {
            clips.pop_front();
            self.start_idx.fetch_add(1, Ordering::Acquire);
        }
        Some(clips.len() + (self.start_idx.load(Ordering::Acquire)) - 1)
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

    pub fn select_clip(&self, clip: Clip) {
        *self.selection.lock().unwrap() = Some(clip)
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
            match &clip.contents.as_ref() {
                ClipContents::Text(content) => {
                    match fuzzy_match(content, pattern) {
                        None => { None }
                        Some(score) => { Some((idx, score)) }
                    }
                }
            }
        }).collect();
        matched_clips.sort_by_key(|(_, score)| { *score });
        matched_clips.iter().rev().take(max)
            .flat_map(|(idx, _)| { clips.get(*idx).cloned() })
            .collect()
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Clip {
    pub source: Source,
    pub contents: Arc<ClipContents>,
}

impl Clip {
    pub fn new(source: Source, contents: ClipContents) -> Clip {
        Clip { source, contents: Arc::new(contents) }
    }

    pub fn contains(&self, other: &Clip) -> bool {
        self.contents.contains(&other.contents)
    }

    pub fn equal(&self, other: &Clip) -> bool {
        self.contents.equal(&other.contents)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum ClipContents {
    Text(String),
}

impl ClipContents {
    pub fn contains(&self, other: &ClipContents) -> bool {
        match self {
            ClipContents::Text(my_str) => {
                match other {
                    ClipContents::Text(their_str) => {
                        my_str.contains(their_str)
                    }
                }
            }
        }
    }

    pub fn equal(&self, other: &ClipContents) -> bool {
        match self {
            ClipContents::Text(my_str) => {
                match other {
                    ClipContents::Text(their_str) => {
                        my_str.eq(their_str)
                    }
                }
            }
        }
    }
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
        let fst = Clip::new(Source::Primary, ClipContents::Text("fst string".to_owned()));
        let snd = Clip::new(Source::Secondary, ClipContents::Text("second string".to_owned()));

        let fst_idx = db.add_clip(fst.clone()).unwrap();
        assert_eq!(fst_idx, 0);

        let snd_idx = db.add_clip(snd.clone()).unwrap();
        assert_eq!(snd_idx, 1);

        assert_eq!(db.at(fst_idx).unwrap(), fst);
        assert_eq!(db.at(snd_idx).unwrap(), snd);
        assert!(db.at(2).is_none());
    }

    #[test]
    fn select() {
        let db = Database::new();
        let fst = Clip::new(Source::Primary, ClipContents::Text("fst string".to_owned()));
        let snd = Clip::new(Source::Secondary, ClipContents::Text("second string".to_owned()));

        let fst_idx = db.add_clip(fst.clone()).unwrap();
        db.add_clip(snd.clone());

        assert!(db.select(fst_idx));

        assert_eq!(db.selection().unwrap(), fst);
    }

    #[test]
    fn rolling() {
        let db = Database::new();
        for i in 1..(MAX_CLIPS * 3) {
            let clip = Clip::new(Source::Primary, ClipContents::Text(format!("clip {}", i)));
            let idx = db.add_clip(clip.clone()).unwrap();
            assert_eq!(db.at(idx).unwrap(), clip);
        }

        assert!(db.at(0).is_none());
        assert_eq!(db.clips().iter().count(), MAX_CLIPS);
    }

    #[test]
    fn selection_stays_after_roll() {
        let db = Database::new();

        let fst = Clip::new(Source::Primary, ClipContents::Text("fst string".to_owned()));
        let fst_idx = db.add_clip(fst.clone()).unwrap();
        assert!(db.select(fst_idx));

        for i in 1..(MAX_CLIPS * 2) {
            let clip = Clip::new(Source::Primary, ClipContents::Text(format!("clip {}", i)));
            let idx = db.add_clip(clip.clone()).unwrap();
            assert_eq!(db.at(idx).unwrap(), clip);
        }

        assert_eq!(db.selection().unwrap(), fst);
    }

    #[test]
    fn search() {
        let db = Database::new();

        let fst = Clip::new(Source::Primary, ClipContents::Text("fst string".to_owned()));
        db.add_clip(fst.clone());
        let snd = Clip::new(Source::Secondary, ClipContents::Text("second string".to_owned()));
        db.add_clip(snd.clone());

        {
            let matches = db.search("fst", 5);
            assert_eq!(matches.len(), 1);
            assert_eq!(matches.first().unwrap().clone(), fst);
        }

        {
            let matches = db.search("string", 5);
            assert_eq!(matches.len(), 2);
        }

        {
            let matches = db.search("second", 5);
            assert_eq!(matches.len(), 1);
            assert_eq!(matches.first().unwrap().clone(), snd);
        }
    }

    #[test]
    fn replace_smaller_text() {
        fn clip(s: &str) -> Clip {
            Clip::new(Source::Primary, ClipContents::Text(s.to_owned()))
        }

        let small = clip("fst");
        let bigger_after = clip("fst after");
        let bigger_before = clip("before fst");
        let bigger_around = clip("before fst after");
        let smaller = clip("s");

        assert_eq!(bigger_after.contains(&small), true);
        assert_eq!(bigger_before.contains(&small), true);
        assert_eq!(bigger_around.contains(&small), true);
        assert_eq!(smaller.contains(&small), false);
    }
}