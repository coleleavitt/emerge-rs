// dep_check.rs -- Dependency satisfaction validation

use crate::atom::Atom;
use crate::exception::InvalidData;
use crate::vartree::VarTree;
use crate::bintree::BinTree;
use crate::porttree::PortTree;

#[derive(Debug)]
pub struct DepCheckResult {
    pub satisfied: Vec<String>,
    pub missing: Vec<String>,
    pub conflicts: Vec<String>,
}

pub struct DepChecker {
    pub vartree: VarTree,
    pub bintree: BinTree,
    pub porttree: PortTree,
}

impl DepChecker {
    pub fn new(root: &str) -> Self {
        DepChecker {
            vartree: VarTree::new(root),
            bintree: BinTree::new(root),
            porttree: PortTree::new(root),
        }
    }

    pub async fn check_dependencies(&mut self, atoms: &[Atom]) -> Result<DepCheckResult, InvalidData> {
        let mut satisfied = Vec::new();
        let mut missing = Vec::new();
        let mut conflicts = Vec::new();

        for atom in atoms {
            match self.check_atom(atom).await {
                Ok(true) => satisfied.push(atom.cp()),
                Ok(false) => missing.push(atom.cp()),
                Err(e) => conflicts.push(format!("{}: {}", atom.cp(), e)),
            }
        }

        Ok(DepCheckResult {
            satisfied,
            missing,
            conflicts,
        })
    }

    async fn check_atom(&mut self, atom: &Atom) -> Result<bool, String> {
        // Check installed packages first
        let installed = self.vartree.get_all_installed().await.map_err(|e| e.to_string())?;
        for cpv in installed {
            if atom.matches(&cpv) {
                return Ok(true);
            }
        }

        // Check binary packages
        let binaries = self.bintree.get_all_binpkgs().await.map_err(|e| e.to_string())?;
        for cpv in binaries {
            if atom.matches(&cpv) {
                return Ok(true);
            }
        }

        // Check available ebuilds (simplified - would need to scan ebuilds)
        // For now, assume if it's in porttree metadata, it's available
        if let Some(_) = self.porttree.get_metadata(&atom.cp()).await {
            return Ok(true);
        }

        Ok(false)
    }

    pub async fn check_blockers(&self, atoms: &[Atom]) -> Result<Vec<String>, InvalidData> {
        let mut blockers = Vec::new();

        for atom in atoms {
            if let Some(blocker) = &atom.blocker {
                // Check if blocked package is installed
                let installed = self.vartree.get_all_installed().await?;
                for cpv in installed {
                    if atom.matches(&cpv) {
                        blockers.push(format!("{} blocks installed {}", blocker, cpv));
                    }
                }
            }
        }

        Ok(blockers)
    }

    pub async fn validate_installation(&mut self, targets: &[String]) -> Result<DepCheckResult, InvalidData> {
        let mut all_deps = Vec::new();

        // For each target, get its dependencies (placeholder - would parse ebuild)
        for target in targets {
            // Placeholder: in real implementation, parse ebuild dependencies
            let deps = self.get_package_deps(target)?;
            all_deps.extend(deps);
        }

        self.check_dependencies(&all_deps).await
    }

    fn get_package_deps(&self, cpv: &str) -> Result<Vec<Atom>, InvalidData> {
        // Placeholder: parse ebuild file to get dependencies
        // For now, return empty vec
        Ok(vec![])
    }
}