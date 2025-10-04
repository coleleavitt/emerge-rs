// atom.rs -- Package atom parsing and matching

use regex::Regex;
use lazy_static::lazy_static;
use crate::exception::InvalidAtom;
use crate::versions::{vercmp, PkgStr};

#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
    None,
    Equal,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Tilde,
    TildeGreater,
}

#[derive(Debug, Clone)]
pub struct Atom {
    pub category: String,
    pub package: String,
    pub version: Option<String>,
    pub op: Operator,
    pub slot: Option<String>,
    pub subslot: Option<String>,
    pub repo: Option<String>,
    pub use_deps: Vec<String>,
    pub blocker: Option<String>,
}

impl Atom {
    pub fn new(atom_str: &str) -> Result<Self, InvalidAtom> {
        lazy_static! {
            static ref ATOM_REGEX: Regex = Regex::new(r"^(?P<blocker>[!~]?)(?P<op>[<>=~]*)(?P<catpkg>[^:]+)(?P<slot>:[^/]+)?(?P<branch>\[.*\])?$").unwrap();
        }

        let caps = ATOM_REGEX.captures(atom_str)
            .ok_or_else(|| InvalidAtom::new(&format!("Invalid atom format: {}", atom_str), None))?;

        let catpkg = caps.name("catpkg")
            .ok_or_else(|| InvalidAtom::new("Missing category/package", None))?
            .as_str();

        let (category, package) = if let Some(slash_pos) = catpkg.find('/') {
            (catpkg[..slash_pos].to_string(), catpkg[slash_pos+1..].to_string())
        } else {
            return Err(InvalidAtom::new("Atom must contain category/package", None));
        };

        let op_str = caps.name("op").map_or("", |m| m.as_str());
        let op = match op_str {
            "" => Operator::None,
            "=" => Operator::Equal,
            ">" => Operator::Greater,
            ">=" => Operator::GreaterEqual,
            "<" => Operator::Less,
            "<=" => Operator::LessEqual,
            "~" => Operator::Tilde,
            "~>" => Operator::TildeGreater,
            _ => return Err(InvalidAtom::new(&format!("Invalid operator: {}", op_str), None)),
        };

        let blocker = caps.name("blocker").map(|m| m.as_str().to_string());

        let slot_part = caps.name("slot").map(|m| m.as_str().trim_start_matches(':'));
        let (slot, subslot) = if let Some(slot_str) = slot_part {
            if let Some(slash_pos) = slot_str.find('/') {
                (Some(slot_str[..slash_pos].to_string()), Some(slot_str[slash_pos+1..].to_string()))
            } else {
                (Some(slot_str.to_string()), None)
            }
        } else {
            (None, None)
        };

        // Parse version from package if operator present
        let (version, package_name) = if op != Operator::None {
            // Extract version from package name
            let pkg_str = PkgStr::new(&package).map_err(|_| InvalidAtom::new("Invalid package version", None))?;
            (Some(pkg_str.version), pkg_str.cpv_split[1].clone())
        } else {
            (None, package)
        };

        // Placeholder for use deps and repo parsing
        let use_deps = vec![];
        let repo = None;

        Ok(Atom {
            category,
            package: package_name,
            version,
            op,
            slot,
            subslot,
            repo,
            use_deps,
            blocker,
        })
    }

    pub fn cp(&self) -> String {
        format!("{}/{}", self.category, self.package)
    }

    pub fn cpv(&self) -> Option<String> {
        self.version.as_ref().map(|v| format!("{}/{}:{}", self.category, self.package, v))
    }

    pub fn matches(&self, cpv: &str) -> bool {
        let pkg_str = match PkgStr::new(cpv) {
            Ok(p) => p,
            Err(_) => return false,
        };

        // Check category/package match
        if pkg_str.cpv_split[0] != self.category || pkg_str.cpv_split[1] != self.package {
            return false;
        }

        // If no version constraint, any version matches
        if self.op == Operator::None {
            return true;
        }

        let version = match &self.version {
            Some(v) => v,
            None => return false,
        };

        match self.op {
            Operator::Equal => pkg_str.version == *version,
            Operator::Greater => vercmp(&pkg_str.version, version).unwrap_or(0) > 0,
            Operator::GreaterEqual => vercmp(&pkg_str.version, version).unwrap_or(0) >= 0,
            Operator::Less => vercmp(&pkg_str.version, version).unwrap_or(0) < 0,
            Operator::LessEqual => vercmp(&pkg_str.version, version).unwrap_or(0) <= 0,
            Operator::Tilde => pkg_str.version.starts_with(version) && pkg_str.version.chars().nth(version.len()) == Some('.'),
            Operator::TildeGreater => vercmp(&pkg_str.version, version).unwrap_or(0) >= 0,
            Operator::None => true,
        }
    }
}

pub fn isvalidatom(atom: &str) -> bool {
    Atom::new(atom).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_atom_parsing() {
        // Test basic atom
        let atom = Atom::new("dev-lang/rust").unwrap();
        assert_eq!(atom.category, "dev-lang");
        assert_eq!(atom.package, "rust");
        assert!(atom.version.is_none());
        assert_eq!(atom.op, Operator::None);

        // Test atom with version
        let atom = Atom::new("=dev-lang/rust-1.0.0").unwrap();
        assert_eq!(atom.category, "dev-lang");
        assert_eq!(atom.package, "rust");
        assert_eq!(atom.version, Some("1.0.0".to_string()));
        assert_eq!(atom.op, Operator::Equal);

        // Test atom with slot
        let atom = Atom::new("dev-lang/rust:1").unwrap();
        assert_eq!(atom.category, "dev-lang");
        assert_eq!(atom.package, "rust");
        assert_eq!(atom.slot, Some("1".to_string()));
    }

    #[tokio::test]
    async fn test_atom_matching() {
        let atom = Atom::new("=dev-lang/rust-1.0.0").unwrap();

        // Should match exact version
        assert!(atom.matches("dev-lang/rust-1.0.0"));

        // Should not match different version
        assert!(!atom.matches("dev-lang/rust-1.1.0"));

        // Should not match different package
        assert!(!atom.matches("dev-lang/python-1.0.0"));
    }

    #[tokio::test]
    async fn test_invalid_atoms() {
        assert!(Atom::new("").is_err());
        assert!(Atom::new("invalid").is_err());
        assert!(Atom::new("no-slash").is_err());
    }
}