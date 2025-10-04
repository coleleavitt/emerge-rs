// depgraph.rs -- Dependency graph resolution

use std::collections::{HashMap, HashSet, VecDeque};
use crate::atom::Atom;
use crate::exception::InvalidData;
use crate::dep::dep_satisfied_with_use;
use crate::porttree::PortTree;

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
    pub backtrack_count: usize,
    pub resolution_time_ms: u128,
}

#[derive(Debug, Clone)]
struct BacktrackState {
    assignments: HashMap<String, String>, // cp -> cpv
    conflicts: HashSet<String>, // packages that caused conflicts
    depth: usize,
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



    pub async fn resolve(&self, targets: &[String], porttree: &mut PortTree) -> Result<ResolutionResult, InvalidData> {
        self.resolve_with_backtracking(targets, porttree).await
    }

    pub async fn resolve_with_backtracking(&self, targets: &[String], porttree: &mut PortTree) -> Result<ResolutionResult, InvalidData> {
        let start_time = std::time::Instant::now();

        let mut backtrack_count = 0;
        let initial_state = BacktrackState {
            assignments: HashMap::new(),
            conflicts: HashSet::new(),
            depth: 0,
        };

        match self.backtrack_resolve(targets, porttree, initial_state, &mut backtrack_count).await {
            Ok(assignments) => {
                let elapsed = start_time.elapsed();
                let elapsed_ms = elapsed.as_millis() as f64 + elapsed.subsec_nanos() as f64 / 1_000_000.0;

                // Convert assignments to resolved list
                let resolved: Vec<String> = assignments.values().cloned().collect();

                Ok(ResolutionResult {
                    resolved,
                    blocked: vec![],
                    circular: vec![],
                    backtrack_count,
                    resolution_time_ms: (elapsed_ms * 1000.0) as u128, // Store as microseconds
                })
            }
            Err(e) => Err(e)
        }
    }

    async fn backtrack_resolve(&self, targets: &[String], porttree: &mut PortTree, mut state: BacktrackState, backtrack_count: &mut usize) -> Result<HashMap<String, String>, InvalidData> {
        // If we've processed all targets, check for conflicts
        if state.depth >= targets.len() {
            return self.check_final_conflicts(&state.assignments, porttree).await;
        }

        let current_target = &targets[state.depth];
        let cp = if current_target.contains('/') {
            current_target.clone()
        } else {
            // Assume it's a package name, try to find it
            // For now, just use as-is
            current_target.clone()
        };

        // Get available versions for this package
        let versions = porttree.get_package_versions(&cp);
        if versions.is_empty() {
            // Skip packages with no available versions
            state.depth += 1;
            return Box::pin(self.backtrack_resolve(targets, porttree, state, backtrack_count)).await;
        }

        // Sort versions (prefer higher versions)
        let mut sorted_versions = versions;
        sorted_versions.sort_by(|a, b| {
            // Simple version comparison - in real implementation, use proper version comparison
            b.cmp(a)
        });

        for version in sorted_versions {
            *backtrack_count += 1;

            // Try assigning this version
            if self.can_assign(&cp, &version, &state.assignments, porttree).await? {
                state.assignments.insert(cp.clone(), version.clone());

                // Recurse to next target
                state.depth += 1;
                match Box::pin(self.backtrack_resolve(targets, porttree, state.clone(), backtrack_count)).await {
                    Ok(result) => return Ok(result),
                    Err(_) => {
                        // Conflict, try next version
                        state.depth -= 1;
                        state.assignments.remove(&cp);
                        continue;
                    }
                }
            }
        }

        // No version worked
        Err(InvalidData::new(&format!("No suitable version found for {}", cp), None))
    }

    async fn can_assign(&self, cp: &str, cpv: &str, assignments: &HashMap<String, String>, porttree: &mut PortTree) -> Result<bool, InvalidData> {
        // Check blockers
        if let Some(node) = self.nodes.get(cp) {
            for blocker in &node.blockers {
                for (_, assigned_cpv) in assignments {
                    if blocker.matches(assigned_cpv) {
                        return Ok(false);
                    }
                }
            }
        }

        Ok(true)
    }

    async fn check_final_conflicts(&self, assignments: &HashMap<String, String>, porttree: &mut PortTree) -> Result<HashMap<String, String>, InvalidData> {
        // Check all dependencies are satisfied
        for (cp, cpv) in assignments {
            if let Some(metadata) = porttree.get_metadata(cpv).await {
                // Check runtime dependencies
                if let Some(depend_str) = metadata.get("DEPEND") {
                    if let Ok(deps) = crate::dep::parse_dependencies(depend_str) {
                        for dep_atom in deps {
                            let dep_cp = dep_atom.cp();
                            if !assignments.contains_key(&dep_cp) {
                                return Err(InvalidData::new(&format!("Unsatisfied dependency {} for {}", dep_cp, cpv), None));
                            }
                        }
                    }
                }
            }
        }

        Ok(assignments.clone())
    }



    /// Advanced dependency resolution with SLOT and version conflict handling
    pub fn resolve_advanced(&self, targets: &[String]) -> Result<ResolutionResult, InvalidData> {
        let start_time = std::time::Instant::now();
        let mut resolved: HashMap<String, String> = HashMap::new(); // cp:slot -> cpv
        let mut blocked: Vec<String> = Vec::new();
        let mut to_process: VecDeque<String> = VecDeque::new();

        // Convert CP targets to CPV targets by finding them in the graph
        for target in targets {
            // If target is already a CPV (contains version), use it directly
            if target.split('-').count() >= 3 {
                to_process.push_back(target.clone());
            } else {
                // Target is a CP, find the corresponding CPV in the graph
                let mut found = false;
                for node_key in self.nodes.keys() {
                    if node_key.starts_with(&format!("{}-", target)) {
                        to_process.push_back(node_key.clone());
                        found = true;
                        break;
                    }
                }
                if !found {
                    // If not found in graph, add the CP anyway - it might get resolved later
                    to_process.push_back(target.clone());
                }
            }
        }

        eprintln!("DEBUG: Starting resolution with {} targets, graph has {} nodes", to_process.len(), self.nodes.len());
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
        let resolved_vec: Vec<String> = resolved.values().cloned().collect();

        let elapsed = start_time.elapsed();
        let elapsed_ms = elapsed.as_millis() as f64 + elapsed.subsec_nanos() as f64 / 1_000_000.0;

        eprintln!("DEBUG: Resolution completed in {:.3} ms, found {} resolved packages, {} blocked, {} circular",
                 elapsed_ms, resolved_vec.len(), blocked.len(), circular.len());

        Ok(ResolutionResult {
            resolved: resolved_vec,
            blocked,
            circular,
            backtrack_count: 0,
            resolution_time_ms: elapsed_ms as u128,
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

    pub async fn get_install_order(&self, targets: &[String], porttree: &mut PortTree) -> Result<Vec<String>, InvalidData> {
        let resolution = self.resolve(targets, porttree).await?;

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