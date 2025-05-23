use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use uuid::Uuid;

/// Returns a unique identifier for this application instance.
/// This ID is generated once per installation and used to isolate
/// settings, history, and state between multiple instances of the application.
pub fn instance_id() -> &'static str {
    static INSTANCE_ID: OnceLock<String> = OnceLock::new();
    INSTANCE_ID.get_or_init(|| {
        // Try to read existing ID from disk
        if let Some(id) = read_instance_id() {
            return id;
        }
        
        // Generate a new ID
        let id = Uuid::new_v4().to_string();
        
        // Save to disk for future runs
        let _ = write_instance_id(&id);
        id
    })
}

/// Returns the path where the instance ID is stored
fn instance_id_path() -> PathBuf {
    let dir = if cfg!(target_os = "macos") {
        dirs_next::home_dir()
            .map(|h| h.join("Library").join("Application Support"))
            .unwrap_or_else(|| PathBuf::from("/tmp"))
    } else if cfg!(target_os = "windows") {
        dirs_next::data_local_dir().unwrap_or_else(|| PathBuf::from("C:\\Temp"))
    } else {
        dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("/tmp"))
    };
    
    dir.join(".editor-instance-id")
}

/// Read instance ID from disk if it exists
fn read_instance_id() -> Option<String> {
    let path = instance_id_path();
    std::fs::read_to_string(path).ok()
}

/// Write instance ID to disk
fn write_instance_id(id: &str) -> std::io::Result<()> {
    let path = instance_id_path();
    std::fs::write(path, id)
}

/// Returns the instance-specific configuration directory
pub fn config_dir() -> PathBuf {
    let id = instance_id();
    
    if cfg!(target_os = "macos") {
        dirs_next::home_dir()
            .map(|h| h.join("Library").join("Application Support").join(format!("Editor-{}", id)))
            .unwrap_or_else(|| PathBuf::from("/tmp").join(format!("Editor-{}", id)))
    } else if cfg!(target_os = "windows") {
        dirs_next::data_local_dir()
            .map(|d| d.join(format!("Editor-{}", id)))
            .unwrap_or_else(|| PathBuf::from(format!("C:\\Temp\\Editor-{}", id)))
    } else {
        dirs_next::config_dir()
            .map(|d| d.join(format!("editor-{}", id)))
            .unwrap_or_else(|| PathBuf::from("/tmp").join(format!("editor-{}", id)))
    }
}

/// Returns the instance-specific cache directory
pub fn cache_dir() -> PathBuf {
    let id = instance_id();
    
    if cfg!(target_os = "macos") {
        dirs_next::cache_dir()
            .map(|d| d.join(format!("Editor-{}", id)))
            .unwrap_or_else(|| PathBuf::from("/tmp").join(format!("Editor-{}-cache", id)))
    } else if cfg!(target_os = "windows") {
        dirs_next::cache_dir()
            .map(|d| d.join(format!("Editor-{}", id)))
            .unwrap_or_else(|| PathBuf::from(format!("C:\\Temp\\Editor-{}-cache", id)))
    } else {
        dirs_next::cache_dir()
            .map(|d| d.join(format!("editor-{}", id)))
            .unwrap_or_else(|| PathBuf::from("/tmp").join(format!("editor-{}-cache", id)))
    }
}

/// Returns the instance-specific data directory
pub fn data_dir() -> PathBuf {
    let id = instance_id();
    
    if cfg!(target_os = "macos") {
        dirs_next::data_dir()
            .map(|d| d.join(format!("Editor-{}", id)))
            .unwrap_or_else(|| PathBuf::from("/tmp").join(format!("Editor-{}-data", id)))
    } else if cfg!(target_os = "windows") {
        dirs_next::data_dir()
            .map(|d| d.join(format!("Editor-{}", id)))
            .unwrap_or_else(|| PathBuf::from(format!("C:\\Temp\\Editor-{}-data", id)))
    } else {
        dirs_next::data_dir()
            .map(|d| d.join(format!("editor-{}", id)))
            .unwrap_or_else(|| PathBuf::from("/tmp").join(format!("editor-{}-data", id)))
    }
}

/// Returns instance-specific subfolder of a base directory
pub fn instance_path(base_dir: impl AsRef<Path>, subfolder: &str) -> PathBuf {
    let id = instance_id();
    base_dir.as_ref().join(format!("{}-{}", subfolder, id))
}

/// Ensures all instance directories exist
pub fn ensure_directories() -> std::io::Result<()> {
    for dir in [config_dir(), cache_dir(), data_dir()] {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_instance_id_is_consistent() {
        let id1 = instance_id();
        let id2 = instance_id();
        assert_eq!(id1, id2);
    }
    
    #[test]
    fn test_instance_paths_include_id() {
        let id = instance_id();
        let config = config_dir();
        assert!(config.to_string_lossy().contains(id));
    }
}