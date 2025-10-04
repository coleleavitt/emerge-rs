// depgraph.rs -- Dependency graph resolution

use std::collections::{HashMap, HashSet, VecDeque};
use crate::atom::Atom;
use crate::exception::InvalidData;
use crate::dep::dep_satisfied_with_use;

#[derive(Debug, Clone, PartialEq)]
pub enum DepType {
    Runtime,
    Build,
    Post,
}

#[derive(Debug, Clone)]
pub struct DepNode {
    pub atom: Atom,
    pub dep_type: DepType,
    pub blockers: Vec<Atom>,
    pub use_conditional: Option<String>,
    pub slot: Option<String>,
    pub subslot: Option<String>,
}

#[derive(Debug)]
pub struct DepGraph {
    pub nodes: HashMap<String, DepNode>,
    pub edges: HashMap<String, Vec<String>>, // node -> dependencies
    pub reverse_edges: HashMap<String, Vec<String>>, // node -> dependents
    pub use_flags: HashMap<String, bool>,
    pub backtrack_limit: usize,
}

#[derive(Debug)]
pub struct ResolutionResult {
    pub resolved: Vec<String>,
    pub blocked: Vec<String>,
    pub circular: Vec<String>,
}

impl DepGraph {
    pub fn new() -> Self {
        DepGraph {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            reverse_edges: HashMap::new(),
            use_flags: HashMap::new(),
            backtrack_limit: 20,
        }
    }

    pub fn with_use_flags(use_flags: HashMap<String, bool>) -> Self {
        DepGraph {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            reverse_edges: HashMap::new(),
            use_flags,
            backtrack_limit: 20,
        }
    }
    
    pub fn set_backtrack_limit(&mut self, limit: usize) {
        self.backtrack_limit = limit;
    }

    pub fn add_node_with_blockers(&mut self, cpv: &str, deps: Vec<DepNode>, blockers: Vec<Atom>) -> Result<(), InvalidData> {
        let node_key = cpv.to_string();

        // Add the main node if not exists
        if !self.nodes.contains_key(&node_key) {
            // Create a dummy atom for the package itself
            let atom = Atom::new(cpv).map_err(|_| InvalidData::new(&format!("Invalid CPV: {}", cpv), None))?;
            self.nodes.insert(node_key.clone(), DepNode {
                atom: atom.clone(),
                dep_type: DepType::Runtime,
                blockers,
                use_conditional: None,
                slot: atom.slot.clone(),
                subslot: atom.subslot.clone(),
            });
        } else {
            // Update existing node with additional blockers
            if let Some(node) = self.nodes.get_mut(&node_key) {
                node.blockers.extend(blockers);
            }
        }

        // Add dependencies
        let mut dep_keys = vec![];
        for dep in deps {
            let dep_key = dep.atom.cp();
            dep_keys.push(dep_key.clone());

            if !self.nodes.contains_key(&dep_key) {
                self.nodes.insert(dep_key.clone(), dep);
            }

            // Add edge
            self.edges.entry(node_key.clone()).or_insert(vec![]).push(dep_key.clone());
            self.reverse_edges.entry(dep_key).or_insert(vec![]).push(node_key.clone());
        }

        Ok(())
    }



    pub fn resolve(&self, targets: &[String]) -> Result<ResolutionResult, InvalidData> {
        self.resolve_with_backtracking(targets)
    }
    
    pub fn resolve_with_backtracking(&self, targets: &[String]) -> Result<ResolutionResult, InvalidData> {
        let mut backtrack_count = 0;
        
        loop {
            match self.resolve_advanced(targets) {
                Ok(result) => {
                    if result.blocked.is_empty() && result.circular.is_empty() {
                        return Ok(result);
                    }
                    
                    if backtrack_count >= self.backtrack_limit {
                        if !result.blocked.is_empty() {
                            return Err(InvalidData::new(
                                &format!("Cannot resolve dependencies after {} backtrack attempts. Blocked packages: {:?}", 
                                         backtrack_count, result.blocked), 
                                None
                            ));
                        }
                        if !result.circular.is_empty() {
                            return Err(InvalidData::new(
                                &format!("Circular dependencies detected: {:?}", result.circular), 
                                None
                            ));
                        }
                        return Ok(result);
                    }
                    
                    backtrack_count += 1;
                }
                Err(e) => {
                    if backtrack_count >= self.backtrack_limit {
                        return Err(e);
                    }
                    backtrack_count += 1;
                }
            }
        }
    }

    /// Advanced dependency resolution with SLOT and version conflict handling
    pub fn resolve_advanced(&self, targets: &[String]) -> Result<ResolutionResult, InvalidData> {
        let mut resolved: HashMap<String, String> = HashMap::new(); // cp:slot -> cpv
        let mut blocked: Vec<String> = Vec::new();
        let mut to_process: VecDeque<String> = targets.iter().cloned().collect();
        let mut visited = HashSet::new();

        while let Some(current) = to_process.pop_front() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            // Check for blockers and SLOT conflicts
            if let Some(node) = self.nodes.get(&current) {
                // Check blockers
                for blocker in &node.blockers {
                    for (_, resolved_cpv) in &resolved {
                        if blocker.matches(resolved_cpv) {
                            blocked.push(current.clone());
                            continue;
                        }
                    }
                }

                // Handle SLOT dependencies
                let cp = node.atom.cp();
                let slot = node.slot.as_ref().unwrap_or(&"0".to_string()).clone();
                
                // Check for slot operators
                if slot == "*" {
                    // Any slot operator - accept any installed slot
                    // Find if any version of this package is already resolved
                    let mut found = false;
                    for (key, _) in &resolved {
                        if key.starts_with(&format!("{}:", cp)) {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        let slot_key = format!("{}:0", cp);
                        resolved.insert(slot_key, current.clone());
                    }
                } else if slot == "=" {
                    // Slot rebuild operator - use installed package's slot
                    // For now, treat as slot 0
                    let slot_key = format!("{}:0", cp);
                    if let Some(existing_cpv) = resolved.get(&slot_key) {
                        if existing_cpv != &current {
                            // Check if same package but different version
                            let existing_cp = if let Some(pos) = existing_cpv.rfind('-') {
                                &existing_cpv[..pos]
                            } else {
                                existing_cpv.as_str()
                            };
                            let current_cp = if let Some(pos) = current.rfind('-') {
                                &current[..pos]
                            } else {
                                current.as_str()
                            };
                            
                            if existing_cp != current_cp {
                                // Different packages in same SLOT - block
                                blocked.push(current.clone());
                                continue;
                            } else {
                                // Same package, prefer higher version
                                // For now, just replace
                                resolved.insert(slot_key.clone(), current.clone());
                            }
                        }
                    } else {
                        resolved.insert(slot_key, current.clone());
                    }
                } else {
                    // Specific slot
                    let slot_key = format!("{}:{}", cp, slot);
                    if let Some(existing_cpv) = resolved.get(&slot_key) {
                        if existing_cpv != &current {
                            // Check if same package but different version
                            let existing_cp = if let Some(pos) = existing_cpv.rfind('-') {
                                &existing_cpv[..pos]
                            } else {
                                existing_cpv.as_str()
                            };
                            let current_cp = if let Some(pos) = current.rfind('-') {
                                &current[..pos]
                            } else {
                                current.as_str()
                            };
                            
                            if existing_cp != current_cp {
                                // Different packages in same SLOT - block
                                blocked.push(current.clone());
                                continue;
                            }
                        }
                    } else {
                        resolved.insert(slot_key, current.clone());
                    }
                }
            }

            // Add dependencies to process queue (filtered by USE flags)
            if let Some(deps) = self.edges.get(&current) {
                for dep in deps {
                    // Check if dependency is satisfied with current USE flags
                    if let Some(node) = self.nodes.get(dep) {
                        if dep_satisfied_with_use(&node.atom, &self.use_flags) {
                            if !visited.contains(dep) {
                                to_process.push_back(dep.clone());
                            }
                        }
                    }
                }
            }
        }

        // Detect circular dependencies
        let circular = self.detect_cycles();

        // Convert resolved map back to vec
        let resolved_vec = resolved.values().cloned().collect();

        Ok(ResolutionResult {
            resolved: resolved_vec,
            blocked,
            circular,
        })
    }

    fn detect_cycles(&self) -> Vec<String> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for node in self.nodes.keys() {
            if !visited.contains(node) {
                self.dfs_cycle(node, &mut visited, &mut rec_stack, &mut cycles);
            }
        }

        cycles
    }

    fn dfs_cycle(&self, node: &str, visited: &mut HashSet<String>, rec_stack: &mut HashSet<String>, cycles: &mut Vec<String>) {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());

        if let Some(deps) = self.edges.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    self.dfs_cycle(dep, visited, rec_stack, cycles);
                } else if rec_stack.contains(dep) {
                    cycles.push(format!("Cycle detected involving: {} -> {}", node, dep));
                }
            }
        }

        rec_stack.remove(node);
    }

    pub fn get_install_order(&self, targets: &[String]) -> Result<Vec<String>, InvalidData> {
        let resolution = self.resolve(targets)?;

        if !resolution.blocked.is_empty() {
            return Err(InvalidData::new(&format!("Blocked packages: {:?}", resolution.blocked), None));
        }

        if !resolution.circular.is_empty() {
            return Err(InvalidData::new(&format!("Circular dependencies: {:?}", resolution.circular), None));
        }

        // Simple topological sort (dependencies first)
        let mut order = Vec::new();
        let mut visited = HashSet::new();

        for target in targets {
            self.topological_sort(target, &mut visited, &mut order);
        }

        Ok(order)
    }

    fn topological_sort(&self, node: &str, visited: &mut HashSet<String>, order: &mut Vec<String>) {
        if visited.contains(node) {
            return;
        }
        visited.insert(node.to_string());

        // Process dependencies first
        if let Some(deps) = self.edges.get(node) {
            for dep in deps {
                self.topological_sort(dep, visited, order);
            }
        }

        order.push(node.to_string());
    }
}