use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use crate::exception::InvalidData;

/// Represents a Gentoo news item
#[derive(Debug, Clone)]
pub struct NewsItem {
    pub name: String,           // e.g., "20231001-1"
    pub title: String,
    pub author: String,
    pub posted: String,         // Date posted
    pub revised: Option<String>, // Date revised
    pub display_if_uninstalled: bool,
    pub display_if_installed: bool,
    pub content: String,
}

/// News system manager for handling Gentoo news
pub struct NewsManager {
    root: String,
    news_dir: PathBuf,
    status_file: PathBuf,
}

impl NewsManager {
    /// Create a new news manager
    pub fn new(root: &str) -> Self {
        let root_path = Path::new(root);
        Self {
            root: root.to_string(),
            news_dir: root_path.join("var/lib/gentoo/news"),
            status_file: root_path.join("var/lib/gentoo/news/news-gentoo.eselect"),
        }
    }

    /// Get all available news items
    pub fn get_news_items(&self) -> Result<Vec<NewsItem>, InvalidData> {
        if !self.news_dir.exists() {
            return Ok(Vec::new());
        }

        let mut items = Vec::new();

        for entry in fs::read_dir(&self.news_dir)
            .map_err(|e| InvalidData::new(&format!("Failed to read news directory: {}", e), None))? {

            let entry = entry
                .map_err(|e| InvalidData::new(&format!("Failed to read directory entry: {}", e), None))?;
            let path = entry.path();

            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    // Skip the status file
                    if filename == "news-gentoo.eselect" {
                        continue;
                    }

                    if let Ok(item) = self.parse_news_item(&path) {
                        items.push(item);
                    }
                }
            }
        }

        // Sort by name (which includes date)
        items.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(items)
    }

    /// Parse a news item from file
    fn parse_news_item(&self, path: &Path) -> Result<NewsItem, InvalidData> {
        let content = fs::read_to_string(path)
            .map_err(|e| InvalidData::new(&format!("Failed to read news item {}: {}", path.display(), e), None))?;

        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| InvalidData::new("Invalid news item filename", None))?;

        let mut title = String::new();
        let mut author = String::new();
        let mut posted = String::new();
        let mut revised = None;
        let mut display_if_uninstalled = false;
        let mut display_if_installed = false;
        let mut body_start = 0;

        for (i, line) in content.lines().enumerate() {
            if line.starts_with("Title: ") {
                title = line[7..].trim().to_string();
            } else if line.starts_with("Author: ") {
                author = line[8..].trim().to_string();
            } else if line.starts_with("Posted: ") {
                posted = line[8..].trim().to_string();
            } else if line.starts_with("Revised: ") {
                revised = Some(line[9..].trim().to_string());
            } else if line.starts_with("Display-If-Uninstalled: ") {
                display_if_uninstalled = line[24..].trim().to_lowercase() == "yes";
            } else if line.starts_with("Display-If-Installed: ") {
                display_if_installed = line[22..].trim().to_lowercase() == "yes";
            } else if line.trim().is_empty() && !title.is_empty() {
                // Empty line after headers, body starts next
                body_start = i + 1;
                break;
            }
        }

        let news_content = content.lines().skip(body_start).collect::<Vec<&str>>().join("\n");

        Ok(NewsItem {
            name: filename.to_string(),
            title,
            author,
            posted,
            revised,
            display_if_uninstalled,
            display_if_installed,
            content: news_content,
        })
    }

    /// Get unread news items
    pub fn get_unread_news(&self) -> Result<Vec<NewsItem>, InvalidData> {
        let all_items = self.get_news_items()?;
        let read_items = self.get_read_news_names()?;

        Ok(all_items.into_iter()
            .filter(|item| !read_items.contains(&item.name))
            .collect())
    }

    /// Get names of read news items from status file
    fn get_read_news_names(&self) -> Result<HashSet<String>, InvalidData> {
        if !self.status_file.exists() {
            return Ok(HashSet::new());
        }

        let content = fs::read_to_string(&self.status_file)
            .map_err(|e| InvalidData::new(&format!("Failed to read news status file: {}", e), None))?;

        let mut read_names = HashSet::new();

        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                read_names.insert(line.to_string());
            }
        }

        Ok(read_names)
    }

    /// Check if a news item has been read
    pub fn is_read(&self, news_name: &str) -> Result<bool, InvalidData> {
        let read_names = self.get_read_news_names()?;
        Ok(read_names.contains(news_name))
    }

    /// Mark a news item as read
    pub fn mark_as_read(&self, news_name: &str) -> Result<(), InvalidData> {
        let mut read_names = self.get_read_news_names()?;
        read_names.insert(news_name.to_string());

        self.write_status_file(&read_names)
    }

    /// Mark a news item as unread
    pub fn mark_as_unread(&self, news_name: &str) -> Result<(), InvalidData> {
        let mut read_names = self.get_read_news_names()?;
        read_names.remove(news_name);

        self.write_status_file(&read_names)
    }

    /// Write the status file with read news names
    fn write_status_file(&self, read_names: &HashSet<String>) -> Result<(), InvalidData> {
        // Ensure the directory exists
        if let Some(parent) = self.status_file.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| InvalidData::new(&format!("Failed to create news directory: {}", e), None))?;
        }

        let mut content = String::from("# News items that have been read\n");
        let mut sorted_names: Vec<&String> = read_names.iter().collect();
        sorted_names.sort();

        for name in sorted_names {
            content.push_str(name);
            content.push('\n');
        }

        fs::write(&self.status_file, content)
            .map_err(|e| InvalidData::new(&format!("Failed to write news status file: {}", e), None))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_news_manager_creation() {
        let manager = NewsManager::new("/");
        assert_eq!(manager.root, "/");
    }

    #[tokio::test]
    async fn test_parse_news_item() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let news_file = temp_dir.path().join("20231001-1");

        let content = r#"Title: Test News Item
Author: Test Author
Posted: 2023-10-01
Display-If-Uninstalled: yes
Display-If-Installed: no

This is the content of the news item.
It can span multiple lines."#;

        fs::write(&news_file, content).unwrap();

        let manager = NewsManager::new("/");
        let item = manager.parse_news_item(&news_file).unwrap();

        assert_eq!(item.name, "20231001-1");
        assert_eq!(item.title, "Test News Item");
        assert_eq!(item.author, "Test Author");
        assert_eq!(item.posted, "2023-10-01");
        assert_eq!(item.display_if_uninstalled, true);
        assert_eq!(item.display_if_installed, false);
        assert!(item.content.contains("This is the content"));
    }

    #[tokio::test]
    async fn test_read_unread_status() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let manager = NewsManager::new(temp_path);

        // Initially no read items
        let read_names = manager.get_read_news_names().unwrap();
        assert!(read_names.is_empty());

        // Mark an item as read
        manager.mark_as_read("20231001-1").unwrap();

        let read_names = manager.get_read_news_names().unwrap();
        assert!(read_names.contains("20231001-1"));

        // Mark as unread
        manager.mark_as_unread("20231001-1").unwrap();

        let read_names = manager.get_read_news_names().unwrap();
        assert!(!read_names.contains("20231001-1"));
    }

    #[tokio::test]
    async fn test_get_unread_news() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let manager = NewsManager::new(temp_path);

        // Create news directory and files
        let news_dir = temp_dir.path().join("var/lib/gentoo/news");
        fs::create_dir_all(&news_dir).unwrap();

        // Create two news items
        let news1_path = news_dir.join("20231001-1");
        let news2_path = news_dir.join("20231002-1");

        let content1 = r#"Title: First News
Author: Author1
Posted: 2023-10-01

First news content."#;

        let content2 = r#"Title: Second News
Author: Author2
Posted: 2023-10-02

Second news content."#;

        fs::write(&news1_path, content1).unwrap();
        fs::write(&news2_path, content2).unwrap();

        // Initially both should be unread
        let unread = manager.get_unread_news().unwrap();
        assert_eq!(unread.len(), 2);
        assert!(unread.iter().any(|n| n.name == "20231001-1"));
        assert!(unread.iter().any(|n| n.name == "20231002-1"));

        // Mark first as read
        manager.mark_as_read("20231001-1").unwrap();

        let unread = manager.get_unread_news().unwrap();
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].name, "20231002-1");
    }

    #[tokio::test]
    async fn test_parse_news_item_with_revised() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let news_file = temp_dir.path().join("20231001-1");

        let content = r#"Title: Test News
Author: Test Author
Posted: 2023-10-01
Revised: 2023-10-02
Display-If-Uninstalled: no
Display-If-Installed: yes

This is revised content."#;

        fs::write(&news_file, content).unwrap();

        let manager = NewsManager::new("/");
        let item = manager.parse_news_item(&news_file).unwrap();

        assert_eq!(item.title, "Test News");
        assert_eq!(item.author, "Test Author");
        assert_eq!(item.posted, "2023-10-01");
        assert_eq!(item.revised, Some("2023-10-02".to_string()));
        assert_eq!(item.display_if_uninstalled, false);
        assert_eq!(item.display_if_installed, true);
    }

    #[tokio::test]
    async fn test_news_sorting() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let manager = NewsManager::new(temp_path);

        // Create news directory and files in reverse order
        let news_dir = temp_dir.path().join("var/lib/gentoo/news");
        fs::create_dir_all(&news_dir).unwrap();

        let news1_path = news_dir.join("20231002-1");
        let news2_path = news_dir.join("20231001-1");

        let content1 = "Title: Later News\nAuthor: Author\nPosted: 2023-10-02\n\nContent1.";
        let content2 = "Title: Earlier News\nAuthor: Author\nPosted: 2023-10-01\n\nContent2.";

        fs::write(&news1_path, content1).unwrap();
        fs::write(&news2_path, content2).unwrap();

        let items = manager.get_news_items().unwrap();
        assert_eq!(items.len(), 2);
        // Should be sorted by name (date)
        assert_eq!(items[0].name, "20231001-1");
        assert_eq!(items[1].name, "20231002-1");
    }

    #[tokio::test]
    async fn test_empty_news_directory() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let manager = NewsManager::new(temp_path);

        // No news directory exists
        let items = manager.get_news_items().unwrap();
        assert!(items.is_empty());

        let unread = manager.get_unread_news().unwrap();
        assert!(unread.is_empty());
    }

    #[tokio::test]
    async fn test_malformed_news_file() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let manager = NewsManager::new(temp_path);

        // Create news directory
        let news_dir = temp_dir.path().join("var/lib/gentoo/news");
        fs::create_dir_all(&news_dir).unwrap();

        // Create malformed news file (missing title)
        let news_file = news_dir.join("20231001-1");
        let content = "Author: Author\nPosted: 2023-10-01\n\nContent.";
        fs::write(&news_file, content).unwrap();

        // Should still parse (with empty title)
        let items = manager.get_news_items().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "");
        assert_eq!(items[0].author, "Author");
    }
}