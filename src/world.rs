// world.rs - World file management for emerge

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use crate::exception::InvalidData;

/// World file manager for handling the @world set
pub struct WorldManager {
    root: String,
    world_file: PathBuf,
}

impl WorldManager {
    /// Create a new WorldManager for the given root
    pub fn new(root: &str) -> Self {
        let world_file = Path::new(root).join("var/lib/portage/world");
        WorldManager {
            root: root.to_string(),
            world_file,
        }
    }

    /// Load the world file and return the set of atoms
    pub fn load(&self) -> Result<HashSet<String>, InvalidData> {
        if !self.world_file.exists() {
            return Ok(HashSet::new());
        }

        let content = fs::read_to_string(&self.world_file)
            .map_err(|e| InvalidData::new(
                &format!("Failed to read world file: {}", e),
                Some(self.world_file.to_string_lossy().to_string())
            ))?;

        let mut atoms = HashSet::new();
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                atoms.insert(line.to_string());
            }
        }

        Ok(atoms)
    }

    /// Save the world file with the given atoms
    pub fn save(&self, atoms: &HashSet<String>) -> Result<(), InvalidData> {
        // Ensure the directory exists
        if let Some(parent) = self.world_file.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| InvalidData::new(
                    &format!("Failed to create world file directory: {}", e),
                    Some(parent.to_string_lossy().to_string())
                ))?;
        }

        let mut content = String::new();
        let mut sorted_atoms: Vec<_> = atoms.iter().collect();
        sorted_atoms.sort();

        for atom in sorted_atoms {
            content.push_str(atom);
            content.push('\n');
        }

        fs::write(&self.world_file, content)
            .map_err(|e| InvalidData::new(
                &format!("Failed to write world file: {}", e),
                Some(self.world_file.to_string_lossy().to_string())
            ))?;

        Ok(())
    }

    /// Add an atom to the world file
    pub fn add_atom(&self, atom: &str) -> Result<(), InvalidData> {
        let mut atoms = self.load()?;
        atoms.insert(atom.to_string());
        self.save(&atoms)
    }

    /// Remove an atom from the world file
    pub fn remove_atom(&self, atom: &str) -> Result<(), InvalidData> {
        let mut atoms = self.load()?;
        atoms.remove(atom);
        self.save(&atoms)
    }

    /// Check if an atom is in the world file
    pub fn contains(&self, atom: &str) -> Result<bool, InvalidData> {
        let atoms = self.load()?;
        Ok(atoms.contains(atom))
    }

    /// Clean up the world file by removing invalid atoms and duplicates
    pub fn clean(&self) -> Result<(), InvalidData> {
        let atoms = self.load()?;
        // For now, just save back (removes duplicates due to HashSet)
        // In the future, we could validate atoms against the portage tree
        self.save(&atoms)
    }

    /// Get the world file path
    pub fn world_file_path(&self) -> &Path {
        &self.world_file
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_world_manager() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_str().unwrap();
        let manager = WorldManager::new(root);

        // Test empty world file
        let atoms = manager.load().unwrap();
        assert!(atoms.is_empty());

        // Add some atoms
        manager.add_atom("app-editors/vim").unwrap();
        manager.add_atom("sys-apps/util-linux").unwrap();

        // Check they were added
        assert!(manager.contains("app-editors/vim").unwrap());
        assert!(manager.contains("sys-apps/util-linux").unwrap());
        assert!(!manager.contains("app-misc/foo").unwrap());

        // Load and verify
        let atoms = manager.load().unwrap();
        assert_eq!(atoms.len(), 2);
        assert!(atoms.contains("app-editors/vim"));
        assert!(atoms.contains("sys-apps/util-linux"));

        // Remove an atom
        manager.remove_atom("app-editors/vim").unwrap();
        assert!(!manager.contains("app-editors/vim").unwrap());
        assert!(manager.contains("sys-apps/util-linux").unwrap());

        // Clean (should work without issues)
        manager.clean().unwrap();
    }
}