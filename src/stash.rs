use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub name: String,
    pub pid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub process_info: ProcessInfo,
    pub operation: String,
    pub file_content: Option<Vec<u8>>,
    pub metadata: StashMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashMetadata {
    pub file_size: u64,
    pub file_hash: String,
    pub is_deletion: bool,
}

#[derive(Clone)]
pub struct StashManager {
    stash_dir: PathBuf,
    active_stashes: HashMap<String, StashEntry>,
}

impl StashManager {
    pub fn new(stash_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&stash_dir)?;
        Ok(Self {
            stash_dir,
            active_stashes: HashMap::new(),
        })
    }
    
    pub fn create_stash(
        &self,
        path: &Path,
        content: &[u8],
        process_info: &ProcessInfo,
        operation: &str,
    ) -> Result<String> {
        let stash_id = self.generate_stash_id();
        let timestamp = Utc::now();
        
        // Create stash subdirectory
        let stash_subdir = self.stash_dir.join(&stash_id);
        fs::create_dir_all(&stash_subdir)?;
        
        // Save the file content
        let filename = path.file_name()
            .context("Invalid file path")?
            .to_string_lossy()
            .to_string();
        let backup_path = stash_subdir.join(&filename);
        fs::write(&backup_path, content)?;
        
        // Calculate file hash
        let file_hash = self.calculate_hash(content);
        
        // Create stash entry
        let entry = StashEntry {
            id: stash_id.clone(),
            timestamp,
            original_path: path.to_path_buf(),
            backup_path: backup_path.clone(),
            process_info: process_info.clone(),
            operation: operation.to_string(),
            file_content: Some(content.to_vec()),
            metadata: StashMetadata {
                file_size: content.len() as u64,
                file_hash,
                is_deletion: false,
            },
        };
        
        // Save metadata
        let metadata_path = stash_subdir.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&entry)?;
        fs::write(metadata_path, metadata_json)?;
        
        Ok(stash_id)
    }
    
    pub fn create_deletion_stash(
        &self,
        path: &Path,
        process_info: &ProcessInfo,
    ) -> Result<String> {
        let stash_id = self.generate_stash_id();
        let timestamp = Utc::now();
        
        // Create stash subdirectory
        let stash_subdir = self.stash_dir.join(&stash_id);
        fs::create_dir_all(&stash_subdir)?;
        
        // For deletions, we just record the metadata
        let entry = StashEntry {
            id: stash_id.clone(),
            timestamp,
            original_path: path.to_path_buf(),
            backup_path: PathBuf::new(),
            process_info: process_info.clone(),
            operation: "delete".to_string(),
            file_content: None,
            metadata: StashMetadata {
                file_size: 0,
                file_hash: String::new(),
                is_deletion: true,
            },
        };
        
        // Save metadata
        let metadata_path = stash_subdir.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&entry)?;
        fs::write(metadata_path, metadata_json)?;
        
        Ok(stash_id)
    }
    
    pub fn get_stash(&self, stash_id: &str) -> Result<Option<StashEntry>> {
        let stash_subdir = self.stash_dir.join(stash_id);
        let metadata_path = stash_subdir.join("metadata.json");
        
        if !metadata_path.exists() {
            return Ok(None);
        }
        
        let metadata_content = fs::read_to_string(metadata_path)?;
        let entry: StashEntry = serde_json::from_str(&metadata_content)?;
        
        Ok(Some(entry))
    }
    
    pub fn list_stashes(&self) -> Result<Vec<StashEntry>> {
        let mut stashes = Vec::new();
        
        for entry in fs::read_dir(&self.stash_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let metadata_path = entry.path().join("metadata.json");
                if metadata_path.exists() {
                    let content = fs::read_to_string(metadata_path)?;
                    if let Ok(stash_entry) = serde_json::from_str::<StashEntry>(&content) {
                        stashes.push(stash_entry);
                    }
                }
            }
        }
        
        // Sort by timestamp (newest first)
        stashes.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        Ok(stashes)
    }
    
    pub fn apply_stash(&self, stash_id: &str) -> Result<()> {
        let stash = self.get_stash(stash_id)?
            .context("Stash not found")?;
        
        if stash.metadata.is_deletion {
            return Err(anyhow::anyhow!("Cannot apply deletion stash"));
        }
        
        // Read the stashed content
        let content = fs::read(&stash.backup_path)?;
        
        // Apply to original location
        if let Some(parent) = stash.original_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&stash.original_path, content)?;
        
        Ok(())
    }
    
    pub fn delete_stash(&self, stash_id: &str) -> Result<()> {
        let stash_subdir = self.stash_dir.join(stash_id);
        if stash_subdir.exists() {
            fs::remove_dir_all(stash_subdir)?;
        }
        Ok(())
    }
    
    pub fn clear_old_stashes(&self, max_age_days: i64) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::days(max_age_days);
        let stashes = self.list_stashes()?;
        let mut deleted_count = 0;
        
        for stash in stashes {
            if stash.timestamp < cutoff {
                self.delete_stash(&stash.id)?;
                deleted_count += 1;
            }
        }
        
        Ok(deleted_count)
    }
    
    fn generate_stash_id(&self) -> String {
        use std::time::SystemTime;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        format!("stash_{}", now.as_millis())
    }
    
    fn calculate_hash(&self, content: &[u8]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
    
    pub fn get_stash_diff(&self, stash_id: &str) -> Result<String> {
        let stash = self.get_stash(stash_id)?
            .context("Stash not found")?;
        
        if stash.metadata.is_deletion {
            return Ok(format!("Deletion attempt of: {}", stash.original_path.display()));
        }
        
        // For now, return basic info
        // In a real implementation, we'd compare with current file
        Ok(format!(
            "Stash: {}\nFile: {}\nOperation: {}\nProcess: {} (PID: {})\nTime: {}\nSize: {} bytes",
            stash.id,
            stash.original_path.display(),
            stash.operation,
            stash.process_info.name,
            stash.process_info.pid,
            stash.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            stash.metadata.file_size
        ))
    }
}