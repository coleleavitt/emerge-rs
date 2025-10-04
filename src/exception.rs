// exception.rs -- Portage exceptions in Rust

use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct PortageException {
    pub value: String,
}

impl PortageException {
    pub fn new(value: &str) -> Self {
        PortageException {
            value: value.to_string(),
        }
    }
}

impl fmt::Display for PortageException {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl Error for PortageException {}

#[derive(Debug)]
pub struct PortageKeyError {
    pub value: String,
}

impl PortageKeyError {
    pub fn new(value: &str) -> Self {
        PortageKeyError {
            value: value.to_string(),
        }
    }
}

impl fmt::Display for PortageKeyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl Error for PortageKeyError {}

// Add other exceptions as needed
#[derive(Debug)]
pub struct InvalidData {
    pub value: String,
    pub category: Option<String>,
}

impl InvalidData {
    pub fn new(value: &str, category: Option<String>) -> Self {
        InvalidData {
            value: value.to_string(),
            category,
        }
    }
}

impl fmt::Display for InvalidData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Invalid data: {}", self.value)
    }
}

impl Error for InvalidData {}

// Similarly for others, but for brevity, define the main ones used in versions.rs
#[derive(Debug)]
pub struct InvalidAtom {
    pub value: String,
    pub category: Option<String>,
}

impl InvalidAtom {
    pub fn new(value: &str, category: Option<String>) -> Self {
        InvalidAtom {
            value: value.to_string(),
            category,
        }
    }
}

impl fmt::Display for InvalidAtom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Invalid atom: {}", self.value)
    }
}

impl Error for InvalidAtom {}