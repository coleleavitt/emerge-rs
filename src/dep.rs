// dep.rs -- Dependency and atom handling

use regex::Regex;
use lazy_static::lazy_static;
use crate::versions::{PkgStr, catpkgsplit};
use crate::exception::{InvalidAtom, InvalidData};

lazy_static! {
    static ref ATOM_RE: Regex = Regex::new(r"^(?P<blocker>!!?)?(?P<op>[=~<>]+)?(?P<cpv>[\w+./-]+)(?P<slot>:[\w+./-]+)?(?P<berepo>::[\w-]+)?(?P<use>\[.*\])?$").unwrap();
}

#[derive(Debug, Clone)]
pub struct Atom {
    pub cpv: String,
    pub op: Option<String>,
    pub slot: Option<String>,
    pub sub_slot: Option<String>,
    pub repo: Option<String>,
    pub use_deps: Vec<String>,
    pub blocker: Option<String>,
}

impl Atom {
    pub fn new(atom_str: &str) -> Result<Self, InvalidAtom> {
        let captures = ATOM_RE.captures(atom_str).ok_or_else(|| InvalidAtom::new(atom_str, None))?;

        let blocker = captures.name("blocker").map(|m| m.as_str().to_string());
        let op = captures.name("op").map(|m| m.as_str().to_string());
        let cpv = captures.name("cpv").map(|m| m.as_str().to_string()).unwrap();
        let slot_part = captures.name("slot").map(|m| m.as_str().to_string());
        let repo = captures.name("berepo").map(|m| m.as_str().to_string());
        let use_str = captures.name("use").map(|m| m.as_str().to_string());

        let (slot, sub_slot) = if let Some(slot_str) = slot_part {
            let slot_str = &slot_str[1..]; // remove :
            
            // Handle slot operators: :=, :*, :slot=
            // Examples:
            // :0 - specific slot
            // := - rebuild on slot/subslot change
            // :* - any slot
            // :0/2.1= - specific slot with subslot rebuild operator
            
            if slot_str == "=" {
                // Slot rebuild operator - will use installed package's slot
                (Some("=".to_string()), Some("=".to_string()))
            } else if slot_str == "*" {
                // Any slot operator
                (Some("*".to_string()), None)
            } else if let Some(slash_pos) = slot_str.find('/') {
                // Has subslot
                let slot_part = &slot_str[..slash_pos];
                let subslot_part = &slot_str[slash_pos + 1..];
                
                // Check for subslot rebuild operator
                if subslot_part.ends_with('=') {
                    let subslot_value = &subslot_part[..subslot_part.len() - 1];
                    if subslot_value.is_empty() {
                        // :slot/= means rebuild on subslot change
                        (Some(slot_part.to_string()), Some("=".to_string()))
                    } else {
                        // :slot/subslot= means specific subslot with rebuild
                        (Some(slot_part.to_string()), Some(subslot_value.to_string()))
                    }
                } else {
                    (Some(slot_part.to_string()), Some(subslot_part.to_string()))
                }
            } else if slot_str.ends_with('=') {
                // Slot rebuild operator with specific slot
                let slot_value = &slot_str[..slot_str.len() - 1];
                (Some(slot_value.to_string()), Some("=".to_string()))
            } else {
                (Some(slot_str.to_string()), None)
            }
        } else {
            (None, None)
        };

        let use_deps = if let Some(use_str) = use_str {
            Self::parse_use_deps(&use_str)?
        } else {
            vec![]
        };

        Ok(Atom {
            cpv,
            op,
            slot,
            sub_slot,
            repo,
            use_deps,
            blocker,
        })
    }

    pub fn cp(&self) -> String {
        if let Some(split) = catpkgsplit(&self.cpv) {
            format!("{}/{}", split[0], split[1])
        } else {
            self.cpv.clone()
        }
    }

    /// Parse USE dependencies from [use_flag,...] format
    fn parse_use_deps(use_str: &str) -> Result<Vec<String>, InvalidAtom> {
        if !use_str.starts_with('[') || !use_str.ends_with(']') {
            return Err(InvalidAtom::new("Invalid USE dependency format", None));
        }

        let inner = &use_str[1..use_str.len() - 1];
        let mut deps = Vec::new();

        for flag in inner.split(',') {
            let flag = flag.trim();
            if flag.is_empty() {
                continue;
            }

            // Handle conditional flags like !flag, flag?, -flag
            if flag.starts_with('!') || flag.ends_with('?') || flag.starts_with('-') {
                deps.push(flag.to_string());
            } else {
                deps.push(flag.to_string());
            }
        }

        Ok(deps)
    }

    pub fn matches(&self, pkg: &PkgStr) -> bool {
        // Simplified matching
        if self.cp() != pkg.cp {
            return false;
        }
        // Check version if op present
        if let Some(op) = &self.op {
            // Implement version comparison
            true // placeholder
        } else {
            true
        }
    }
}

pub fn isvalidatom(atom_str: &str) -> bool {
    ATOM_RE.is_match(atom_str)
}

pub fn dep_getkey(atom_str: &str) -> Option<String> {
    if let Ok(atom) = Atom::new(atom_str) {
        Some(atom.cp())
    } else {
        None
    }
}

/// Expand USE flags in dependency strings
pub fn expand_use_flags(dep_str: &str, use_flags: &std::collections::HashMap<String, bool>) -> String {
    let mut result = dep_str.to_string();

    // Handle conditional dependencies like flag? ( dep )
    let conditional_re = regex::Regex::new(r"(\w+)\?\s*\(\s*([^)]+)\s*\)").unwrap();

    result = conditional_re.replace_all(&result, |caps: &regex::Captures| {
        let flag = &caps[1];
        let deps = &caps[2];

        if let Some(&enabled) = use_flags.get(flag) {
            if enabled {
                deps.to_string()
            } else {
                "".to_string()
            }
        } else {
            // Flag not set, assume disabled
            "".to_string()
        }
    }).to_string();

    // Clean up extra whitespace
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Check if a dependency atom is satisfied given USE flags
pub fn dep_satisfied_with_use(atom: &crate::atom::Atom, use_flags: &std::collections::HashMap<String, bool>) -> bool {
    // Check USE dependencies
    for use_dep in &atom.use_deps {
        if use_dep.ends_with('?') {
            // Conditional dependency: flag?
            let flag = &use_dep[..use_dep.len() - 1];
            if let Some(&enabled) = use_flags.get(flag) {
                if !enabled {
                    return false; // Required flag is disabled
                }
            } else {
                return false; // Required flag not set
            }
        } else if use_dep.starts_with('!') {
            // Blocking dependency: !flag
            let flag = &use_dep[1..];
            if let Some(&enabled) = use_flags.get(flag) {
                if enabled {
                    return false; // Blocking flag is enabled
                }
            }
        } else if use_dep.starts_with('-') {
            // Disabled dependency: -flag (same as !flag)
            let flag = &use_dep[1..];
            if let Some(&enabled) = use_flags.get(flag) {
                if enabled {
                    return false; // Should be disabled but is enabled
                }
            }
        } else {
            // Required flag
            if let Some(&enabled) = use_flags.get(use_dep) {
                if !enabled {
                    return false; // Required flag is disabled
                }
            } else {
                return false; // Required flag not set
            }
        }
    }

    true
}

// Placeholder for other functions
pub fn match_from_list(_atom: &str, _pkgs: &[String]) -> Vec<String> {
    vec![] // placeholder
}

/// Parse a dependency string into a vector of Atoms
pub fn parse_dependencies(dep_str: &str) -> Result<Vec<Atom>, InvalidData> {
    parse_dependencies_with_use(dep_str, &std::collections::HashMap::new())
}

pub fn parse_dependencies_with_use(dep_str: &str, use_flags: &std::collections::HashMap<String, bool>) -> Result<Vec<Atom>, InvalidData> {
    let mut atoms = Vec::new();

    if dep_str.trim().is_empty() {
        return Ok(atoms);
    }

    // Expand USE flag conditionals first
    let expanded_dep_str = expand_use_flags(dep_str, use_flags);

    // Tokenize by whitespace first
    let tokens: Vec<&str> = expanded_dep_str.split_whitespace().collect();
    
    // Process tokens, handling groups and conditionals
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        
        // Handle OR groups: || ( pkg1 pkg2 pkg3 )
        // Portage's behavior: pick the first INSTALLED alternative, or skip if none installed
        // For simplicity, we'll skip OR groups entirely - they're usually already satisfied
        if token == "||" {
            i += 1; // skip ||
            if i < tokens.len() && tokens[i] == "(" {
                i += 1; // skip opening paren
                // Skip all atoms in the OR group
                let mut depth = 1;
                while i < tokens.len() && depth > 0 {
                    if tokens[i] == "(" {
                        depth += 1;
                    } else if tokens[i] == ")" {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    i += 1;
                }
            }
            i += 1;
            continue;
        }
        
        // Skip standalone parentheses
        if token == "(" || token == ")" {
            i += 1;
            continue;
        }
        
        // Handle USE conditionals: flag? ( ... )
        if token.ends_with('?') {
            let flag = &token[..token.len() - 1];
            
            // Skip test dependencies by default (FEATURES=-test in Gentoo)
            // Also skip doc, examples, and other optional build-time features
            let skip_flags = ["test", "doc", "examples", "gtk-doc"];
            let should_skip = skip_flags.contains(&flag);
            
            // This is a USE flag conditional
            i += 1;
            if i < tokens.len() && tokens[i] == "(" {
                i += 1; // skip opening paren
                // Parse or skip atoms until closing paren
                let mut depth = 1;
                while i < tokens.len() && depth > 0 {
                    if tokens[i] == "(" {
                        depth += 1;
                    } else if tokens[i] == ")" {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    } else if tokens[i] != "||" && !should_skip {
                        // Only parse the atom if we're not skipping this flag
                        if let Ok(atom) = Atom::new(tokens[i]) {
                            atoms.push(atom);
                        }
                    }
                    i += 1;
                }
            }
            i += 1;
            continue;
        }
        
        // Regular atom
        match Atom::new(token) {
            Ok(atom) => atoms.push(atom),
            Err(_) => {
                // Skip invalid atoms (might be partial syntax)
            }
        }
        
        i += 1;
    }

    Ok(atoms)
}

/// Parse a single atom string, handling USE conditionals
fn parse_atom_string(atom_str: &str) -> Result<Vec<Atom>, InvalidData> {
    let atom_str = atom_str.trim();

    if atom_str.is_empty() {
        return Ok(vec![]);
    }

    // Skip dependency operators and grouping syntax
    if atom_str == "||" || atom_str == "(" || atom_str == ")" || atom_str == "]" {
        return Ok(vec![]);
    }

    // Handle USE conditionals: flag? ( dep )
    if let Some(question_pos) = atom_str.find('?') {
        let flag_part = &atom_str[..question_pos];
        let rest = &atom_str[question_pos + 1..].trim();

        if rest.starts_with('(') && rest.ends_with(')') {
            let deps = &rest[1..rest.len() - 1];
            let mut atoms = parse_dependencies(deps)?;

            // Add USE conditional to each atom
            for atom in &mut atoms {
                atom.use_deps.push(flag_part.to_string());
            }

            return Ok(atoms);
        }
    }

    // Parse the atom (blockers are handled in Atom::new)
    match Atom::new(atom_str) {
        Ok(atom) => Ok(vec![atom]),
        Err(e) => Err(InvalidData::new(&format!("Invalid atom '{}': {}", atom_str, e), None)),
    }
}