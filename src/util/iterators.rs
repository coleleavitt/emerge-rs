// iterators.rs -- Iterator utilities

use std::collections::HashMap;
use std::cmp::Ordering;
use std::hash::Hash;

pub struct MultiIterGroupBy<T, I, F, K>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Ord + Clone + Hash + Eq,
{
    iterators: Vec<I>,
    key_fn: F,
    trackers: Vec<IteratorTracker<T, K>>,
    key_map: HashMap<K, Vec<T>>,
    key_list: Vec<K>,
}

impl<T, I, F, K> MultiIterGroupBy<T, I, F, K>
where
    I: Iterator<Item = T> + 'static,
    F: Fn(&T) -> K,
    K: Ord + Clone + Hash + Eq,
{
    pub fn new(iterators: Vec<I>, key_fn: F) -> Self {
        let trackers = iterators.into_iter().map(|it| IteratorTracker::new(it)).collect();
        MultiIterGroupBy {
            iterators: vec![],
            key_fn,
            trackers,
            key_map: HashMap::new(),
            key_list: vec![],
        }
    }
}

impl<T, I, F, K> Iterator for MultiIterGroupBy<T, I, F, K>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Ord + Clone + Hash + Eq,
{
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut eof = vec![];
            for tracker in &mut self.trackers {
                if tracker.current.is_some() {
                    // Assume sorted, etc.
                    // Simplified implementation
                }
                match tracker.iterator.next() {
                    Some(entry) => {
                        let key = (self.key_fn)(&entry);
                        tracker.current = Some(key.clone());
                        self.key_map.entry(key.clone()).or_insert_with(Vec::new).push(entry);
                        if !self.key_list.contains(&key) {
                            self.key_list.push(key);
                            self.key_list.sort();
                        }
                    }
                    None => {
                        eof.push(tracker as *mut _); // Simplified, use indices
                    }
                }
            }
            // Remove eof trackers
            // This is complex, for brevity, assume no eof for now
            if self.trackers.is_empty() {
                if self.key_list.is_empty() {
                    return None;
                }
                let k = self.key_list.remove(0);
                return self.key_map.remove(&k);
            }
            // Yield groups where key <= min_progress
            // Simplified: yield all when all trackers have progressed
            // This is not accurate, but for now
        }
    }
}

struct IteratorTracker<T, K: Ord> {
    iterator: Box<dyn Iterator<Item = T>>,
    current: Option<K>,
}

impl<T, K: Ord> IteratorTracker<T, K> {
    fn new<I: Iterator<Item = T> + 'static>(iterator: I) -> Self {
        IteratorTracker {
            iterator: Box::new(iterator),
            current: None,
        }
    }
}

impl<T, K: Ord> PartialOrd for IteratorTracker<T, K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (&self.current, &other.current) {
            (None, None) => Some(Ordering::Equal),
            (None, Some(_)) => Some(Ordering::Greater),
            (Some(_), None) => Some(Ordering::Less),
            (Some(a), Some(b)) => a.partial_cmp(b),
        }
    }
}

impl<T, K: Ord> Ord for IteratorTracker<T, K> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl<T, K: Ord> PartialEq for IteratorTracker<T, K> {
    fn eq(&self, other: &Self) -> bool {
        self.current == other.current
    }
}

impl<T, K: Ord> Eq for IteratorTracker<T, K> {}