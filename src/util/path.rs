// path.rs -- Path utilities

use std::path::Path;

pub fn first_existing(path: &Path) -> &Path {
    for p in iter_parents(path) {
        if p.exists() {
            return p;
        }
    }
    Path::new("/")
}

pub fn iter_parents(path: &Path) -> impl Iterator<Item = &Path> {
    let mut current = Some(path);
    std::iter::from_fn(move || {
        if let Some(p) = current {
            current = p.parent();
            Some(p)
        } else {
            None
        }
    })
}