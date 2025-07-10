use anyhow::{Context, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use fs_usage_sys::{FsEvent, FsUsageMonitorBuilder};
use std::collections::HashSet;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use std::thread;
use chrono::{DateTime, Utc};
use sha2::{Sha256, Digest};

use crate::stash::{StashManager, ProcessInfo};

fn is_write_or_delete_op(operation: &str) -> bool {
    operation.contains("write") || operation.contains("WrData") || operation.contains("WrMeta") || 
    operation.contains("rename") || operation.contains("create") || operation.contains("truncate") ||
    operation.contains("chmod_extended") || operation.contains("unlink") || operation.contains("rmdir")
}

fn log_to_file(message: &str) {
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let log_message = format!("[{}] {}\n", timestamp, message);
    
    // Create logs directory if it doesn't exist
    if let Err(_) = std::fs::create_dir_all("logs") {
        return; // Can't create directory, can't log
    }
    
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("logs/unified.log")
    {
        let _ = file.write_all(log_message.as_bytes());
        let _ = file.flush();
    }
}

#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub enabled: bool,
    pub block_timeout: Duration,
    pub auto_stash: bool,
    pub monitored_processes: Vec<String>,
    pub block_mode: BlockMode,
    pub verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockMode {
    Revert,      // Immediately revert changes
    NotifyOnly,  // Just notify, don't block
    StashOnly,   // Stash changes without reverting
}

#[derive(Debug, Clone)]
pub enum GuardianEvent {
    BlockedWrite { 
        path: PathBuf, 
        process: String, 
        pid: u32,
        timestamp: DateTime<Utc>,
    },
    BlockedDelete { 
        path: PathBuf, 
        process: String, 
        pid: u32,
        timestamp: DateTime<Utc>,
    },
    StashedChange { 
        stash_id: String, 
        path: PathBuf,
        operation: String,
    },
    TimeoutExpired { 
        path: PathBuf 
    },
    MonitorStarted,
    MonitorStopped,
    MonitorError(String),
}

pub struct FsGuardianMonitor {
    monitor: Option<fs_usage_sys::FsUsageMonitor>,
    locked_paths: Arc<RwLock<HashSet<PathBuf>>>,
    config: Arc<RwLock<MonitorConfig>>,
    event_tx: Sender<GuardianEvent>,
    event_rx: Receiver<GuardianEvent>,
    stash_manager: StashManager,
    file_backups: Arc<RwLock<std::collections::HashMap<PathBuf, Vec<u8>>>>,
    is_running: Arc<RwLock<bool>>,
}

impl FsGuardianMonitor {
    pub fn new(config: MonitorConfig, root_path: PathBuf) -> Result<Self> {
        let (event_tx, event_rx) = unbounded();
        
        // Create a unique hash for this project path
        let path_hash = Self::hash_path(&root_path);
        
        // Store stashes in user's home directory
        let home_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let stash_dir = home_dir.join(".icaros").join("stashes").join(&path_hash);
        
        // Create stash directory if it doesn't exist
        fs::create_dir_all(&stash_dir)?;
        
        // Create a metadata file to track which project this is
        let metadata_path = stash_dir.join("project_info.json");
        if !metadata_path.exists() {
            let metadata = serde_json::json!({
                "project_path": root_path.to_string_lossy(),
                "created": Utc::now().to_rfc3339(),
                "hash": path_hash
            });
            fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)?;
        }
        
        log_to_file(&format!("Stash directory: {:?} (hash: {})", stash_dir, path_hash));
        
        Ok(Self {
            monitor: None,
            locked_paths: Arc::new(RwLock::new(HashSet::new())),
            config: Arc::new(RwLock::new(config)),
            event_tx,
            event_rx,
            stash_manager: StashManager::new(stash_dir)?,
            file_backups: Arc::new(RwLock::new(std::collections::HashMap::new())),
            is_running: Arc::new(RwLock::new(false)),
        })
    }
    
    pub fn start(&mut self, root_path: &Path) -> Result<()> {
        log_to_file("MONITOR: start() called");
        
        if *self.is_running.read().unwrap() {
            log_to_file("MONITOR: Already running, returning");
            return Ok(());
        }
        
        // OPTION 2: Dynamic monitoring - backup files for potential revert
        log_to_file("MONITOR: Creating backups of locked files");
        self.backup_locked_files()?;
        
        log_to_file(&format!("MONITOR: Building monitor for path: {}", root_path.display()));
        
        // Build the monitor with our configuration
        let watch_pattern1 = format!("{}/**/*", root_path.display());
        let watch_pattern2 = format!("{}/*", root_path.display());
        log_to_file(&format!("MONITOR: Watch patterns: {} and {}", watch_pattern1, watch_pattern2));
        
        let builder = FsUsageMonitorBuilder::new()
            .watch_path(watch_pattern1)
            .watch_path(watch_pattern2)
            // Remove write_only to catch all file operations including renames, moves, etc.
            .exclude_process("mds")
            .exclude_process("mdworker")
            .exclude_process("Spotlight");
        
        log_to_file("MONITOR: Building fs_usage monitor");
        let mut monitor = builder.build()
            .context("Failed to build fs_usage monitor")?;
        
        log_to_file("MONITOR: Starting fs_usage monitor");
        monitor.start()
            .context("Failed to start fs_usage monitor - ensure you're running with sudo")?;
        
        log_to_file("MONITOR: fs_usage monitor started successfully");
        *self.is_running.write().unwrap() = true;
        self.event_tx.send(GuardianEvent::MonitorStarted)?;
        
        // Start monitoring thread
        log_to_file("MONITOR: Starting monitoring thread");
        let monitor_rx = monitor.events().clone();
        let event_tx = self.event_tx.clone();
        let locked_paths = self.locked_paths.clone();
        let file_backups = self.file_backups.clone();
        let config = self.config.clone();
        let is_running = self.is_running.clone();
        let stash_manager = self.stash_manager.clone();
        
        thread::spawn(move || {
            log_to_file("MONITOR: Monitoring thread started");
            let mut event_count = 0;
            let mut loop_count = 0;
            
            while *is_running.read().unwrap() {
                loop_count += 1;
                if loop_count % 100 == 0 {  // Every 10 seconds (100 * 100ms)
                    let verbose = config.read().unwrap().verbose;
                    if verbose {
                        log_to_file(&format!("MONITOR: Thread alive, loop #{}, {} events so far", loop_count, event_count));
                    }
                }
                match monitor_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(event) => {
                        event_count += 1;
                        if event.process_name != "icaros" && event.process_name != "Finder" {
                            let verbose = config.read().unwrap().verbose;
                            if verbose || is_write_or_delete_op(&event.operation) {
                                log_to_file(&format!("MONITOR: Received event #{}: {} {} {}", 
                                                     event_count, event.process_name, event.operation, event.path));
                            }
                        }
                        
                        if let Err(e) = Self::handle_fs_event(
                            &event, 
                            &locked_paths, 
                            &file_backups,
                            &config,
                            &event_tx,
                            &stash_manager,
                        ) {
                            log_to_file(&format!("MONITOR: Error handling fs event: {}", e));
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                    Err(e) => {
                        log_to_file(&format!("MONITOR: Channel error: {}", e));
                        break;
                    }
                }
            }
            log_to_file("MONITOR: Monitoring thread ended");
        });
        
        self.monitor = Some(monitor);
        Ok(())
    }
    
    pub fn stop(&mut self) -> Result<()> {
        *self.is_running.write().unwrap() = false;
        
        if let Some(mut monitor) = self.monitor.take() {
            monitor.stop()?;
        }
        
        self.event_tx.send(GuardianEvent::MonitorStopped)?;
        Ok(())
    }
    
    pub fn update_locked_paths(&self, paths: Vec<PathBuf>) -> Result<()> {
        let mut locked = self.locked_paths.write().unwrap();
        locked.clear();
        locked.extend(paths);
        Ok(())
    }
    
    pub fn update_monitored_processes(&self, processes: Vec<String>) -> Result<()> {
        let mut config = self.config.write().unwrap();
        log_to_file(&format!("MONITOR: Updating monitored processes from {:?} to {:?}", 
                           config.monitored_processes, processes));
        config.monitored_processes = processes;
        Ok(())
    }
    
    pub fn events(&self) -> &Receiver<GuardianEvent> {
        &self.event_rx
    }
    
    pub fn is_running(&self) -> bool {
        *self.is_running.read().unwrap()
    }
    
    fn handle_fs_event(
        event: &FsEvent,
        locked_paths: &Arc<RwLock<HashSet<PathBuf>>>,
        file_backups: &Arc<RwLock<std::collections::HashMap<PathBuf, Vec<u8>>>>,
        config: &Arc<RwLock<MonitorConfig>>,
        event_tx: &Sender<GuardianEvent>,
        stash_manager: &StashManager,
    ) -> Result<()> {
        // Get config
        let config_guard = config.read().unwrap();
        let verbose = config_guard.verbose;
        
        // Log events based on mode (except icaros and Finder)
        if event.process_name != "icaros" && event.process_name != "Finder" {
            if verbose || is_write_or_delete_op(&event.operation) {
                log_to_file(&format!("FS_EVENT: {} [{}] {} (PID: {})", 
                          event.process_name, event.operation, event.path, event.pid));
            }
        }
        
        // Check if this is from a monitored process
        let is_monitored = config_guard.monitored_processes.is_empty() || 
            config_guard.monitored_processes.iter().any(|p| event.process_name.contains(p));
        
        // Log write operations from potential editors for debugging
        if event.process_name != "icaros" && event.process_name != "Finder" && 
           (is_write_or_delete_op(&event.operation) || verbose) {
            log_to_file(&format!("DEBUG: Process '{}' operation='{}' monitored={}", 
                              event.process_name, event.operation, is_monitored));
        }
        
        if !is_monitored {
            if event.process_name != "icaros" && event.process_name != "Finder" {
                if verbose {
                    log_to_file(&format!("DEBUG: Process '{}' not in monitored list - allowing", event.process_name));
                }
            }
            drop(config_guard); // Drop the guard before returning
            
            // IMPORTANT: Update backup for non-monitored processes (allowed writes)
            // This ensures vim reverts to the latest allowed version, not the initial backup
            if event.operation.contains("write") || event.operation.contains("WrData") || 
               event.operation.contains("close") || event.operation.contains("truncate") {
                
                let event_path = PathBuf::from(&event.path);
                
                // Wait a bit for the write to complete
                thread::sleep(Duration::from_millis(50));
                
                // Update the backup with the new content
                if event_path.exists() && event_path.is_file() {
                    if let Ok(new_content) = fs::read(&event_path) {
                        let mut backups = file_backups.write().unwrap();
                        let old_size = backups.get(&event_path).map(|b| b.len()).unwrap_or(0);
                        backups.insert(event_path.clone(), new_content.clone());
                        if event.process_name != "icaros" && event.process_name != "Finder" {
                            // Re-check verbose flag after dropping guard
                            let verbose = config.read().unwrap().verbose;
                            if verbose {
                                log_to_file(&format!("BACKUP: Updated backup for {:?} after non-monitored write ({} -> {} bytes)", 
                                                   event_path, old_size, new_content.len()));
                            }
                        }
                    }
                }
            }
            
            return Ok(());
        }
        
        let path = PathBuf::from(&event.path);
        
        // Check if this path is locked
        let is_locked = {
            let locked = locked_paths.read().unwrap();
            if verbose {
                log_to_file(&format!("DEBUG: Checking if {:?} is locked against {} paths", path, locked.len()));
            }
            if verbose {
                for locked_path in locked.iter() {
                    log_to_file(&format!("  - comparing against {:?}", locked_path));
                }
            }
            Self::is_path_locked(&path, &locked)
        };
        
        if !is_locked {
            if verbose {
                log_to_file(&format!("✅ ALLOWED: Path {:?} is not locked, allowing {} by {}", 
                                   path, event.operation, event.process_name));
            }
            return Ok(());
        }
        
        if verbose {
            log_to_file(&format!("DEBUG: Path {:?} is LOCKED, processing event", path));
        }
        
        // Handle different operations - with our updated fs_usage_sys, we now properly detect all write operations
        match event.operation.as_str() {
            op if op.contains("write") || op.contains("WrData") || op.contains("WrMeta") || 
                  op.contains("rename") || op.contains("create") || op.contains("truncate") ||
                  op.contains("chmod_extended") => {
                if verbose {
                    log_to_file(&format!("DEBUG: Handling write-like operation: {}", op));
                }
                Self::handle_blocked_write(event, &path, &*config_guard, event_tx, file_backups, stash_manager)?;
            }
            op if op.contains("unlink") || op.contains("rmdir") => {
                if verbose {
                    log_to_file(&format!("DEBUG: Handling delete operation: {}", op));
                }
                Self::handle_blocked_delete(event, &path, &*config_guard, event_tx, stash_manager)?;
            }
            _ => {
                if verbose {
                    log_to_file(&format!("DEBUG: Ignoring operation: {} (read-only)", event.operation));
                }
            }
        }
        
        Ok(())
    }
    
    fn is_path_locked(path: &Path, locked_paths: &HashSet<PathBuf>) -> bool {
        // Check if the path itself or any of its parents are locked
        locked_paths.iter().any(|locked| {
            path.starts_with(locked) || path == locked
        })
    }
    
    fn handle_blocked_write(
        event: &FsEvent,
        path: &Path,
        config: &MonitorConfig,
        event_tx: &Sender<GuardianEvent>,
        file_backups: &Arc<RwLock<std::collections::HashMap<PathBuf, Vec<u8>>>>,
        stash_manager: &StashManager,
    ) -> Result<()> {
        let timestamp = Utc::now();
        
        // For vim, we need to handle the actual file, not just swap files
        let actual_path = if event.process_name.contains("vim") {
            // Check if this is a swap file (.swp, .swx, .swo, etc.)
            let path_str = path.to_string_lossy();
            if let Some(file_name) = path.file_name() {
                let name_str = file_name.to_string_lossy();
                // Vim swap files start with . and end with .swp/.swx/.swo etc.
                if name_str.starts_with('.') && (name_str.ends_with(".swp") || 
                                                 name_str.ends_with(".swx") || 
                                                 name_str.ends_with(".swo")) {
                    // Extract the original filename from swap file
                    // .filename.swp -> filename
                    let original_name = name_str.trim_start_matches('.')
                                               .trim_end_matches(".swp")
                                               .trim_end_matches(".swx")
                                               .trim_end_matches(".swo");
                    
                    if let Some(parent) = path.parent() {
                        parent.join(original_name)
                    } else {
                        path.to_path_buf()
                    }
                } else if path_str.ends_with("~") {
                    // Handle vim backup files (filename~)
                    PathBuf::from(path_str.trim_end_matches('~'))
                } else if path_str.contains("/4913") || path_str.matches(|c: char| c.is_numeric()).count() == path_str.len() {
                    // Handle vim temporary files (numeric names)
                    // This is tricky - we need to track what file vim is editing
                    // For now, we'll just use the path as-is
                    path.to_path_buf()
                } else {
                    path.to_path_buf()
                }
            } else {
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        };
        
        // Send blocked event
        event_tx.send(GuardianEvent::BlockedWrite {
            path: actual_path.clone(),
            process: event.process_name.clone(),
            pid: event.pid,
            timestamp,
        })?;
        
        match config.block_mode {
            BlockMode::Revert => {
                // For rename operations (common with vim), we need to wait for the operation to complete
                if event.operation.contains("rename") || event.process_name.contains("vim") {
                    thread::sleep(Duration::from_millis(200));
                }
                
                // Try to get backup content
                let backup_content = {
                    let backups = file_backups.read().unwrap();
                    backups.get(&actual_path).cloned()
                };
                
                if let Some(content) = backup_content {
                    log_to_file(&format!("REVERT: Restoring file {:?} from backup", actual_path));
                    
                    // First, read the current (modified) content for stashing
                    let modified_content = fs::read(&actual_path).ok();
                    
                    // Revert the file to backup
                    if let Err(e) = fs::write(&actual_path, &content) {
                        log_to_file(&format!("REVERT: Failed to restore file: {}", e));
                    } else {
                        log_to_file(&format!("REVERT: Successfully restored file {:?}", actual_path));
                        
                        // If auto-stash is enabled and we have modified content, save it
                        if config.auto_stash {
                            if let Some(modified) = modified_content {
                                if modified != content {  // Only stash if actually different
                                    let process_info = ProcessInfo {
                                        name: event.process_name.clone(),
                                        pid: event.pid,
                                    };
                                    let stash_id = stash_manager.create_stash(
                                        &actual_path,
                                        &modified,
                                        &process_info,
                                        &event.operation,
                                    )?;
                                    
                                    event_tx.send(GuardianEvent::StashedChange {
                                        stash_id,
                                        path: actual_path.clone(),
                                        operation: event.operation.clone(),
                                    })?;
                                }
                            }
                        }
                    }
                } else {
                    log_to_file(&format!("REVERT: No backup found for {:?}", actual_path));
                }
            }
            BlockMode::StashOnly => {
                // Read current content and stash it
                if let Ok(content) = fs::read(path) {
                    let process_info = ProcessInfo {
                        name: event.process_name.clone(),
                        pid: event.pid,
                    };
                    let stash_id = stash_manager.create_stash(
                        path,
                        &content,
                        &process_info,
                        &event.operation,
                    )?;
                    
                    event_tx.send(GuardianEvent::StashedChange {
                        stash_id,
                        path: path.to_path_buf(),
                        operation: event.operation.clone(),
                    })?;
                }
            }
            BlockMode::NotifyOnly => {
                // Just notify, no action taken
            }
        }
        
        Ok(())
    }
    
    fn handle_blocked_delete(
        event: &FsEvent,
        path: &Path,
        config: &MonitorConfig,
        event_tx: &Sender<GuardianEvent>,
        stash_manager: &StashManager,
    ) -> Result<()> {
        let timestamp = Utc::now();
        
        event_tx.send(GuardianEvent::BlockedDelete {
            path: path.to_path_buf(),
            process: event.process_name.clone(),
            pid: event.pid,
            timestamp,
        })?;
        
        // For deletes, we can't revert easily, but we can stash the info
        if config.auto_stash {
            let process_info = ProcessInfo {
                name: event.process_name.clone(),
                pid: event.pid,
            };
            
            // Create a stash entry for the deletion attempt
            let stash_id = stash_manager.create_deletion_stash(
                path,
                &process_info,
            )?;
            
            event_tx.send(GuardianEvent::StashedChange {
                stash_id,
                path: path.to_path_buf(),
                operation: "delete".to_string(),
            })?;
        }
        
        Ok(())
    }
    
    pub fn pre_backup_file(&self, path: &Path) -> Result<()> {
        // Create a backup of the file before any changes
        if path.exists() && path.is_file() {
            let content = fs::read(path)?;
            let mut backups = self.file_backups.write().unwrap();
            backups.insert(path.to_path_buf(), content);
        }
        Ok(())
    }

    // OPTION 2: Dynamic backup and revert
    fn backup_locked_files(&self) -> Result<()> {
        let locked_paths = self.locked_paths.read().unwrap();
        log_to_file(&format!("BACKUP: Creating backups for {} locked paths", locked_paths.len()));
        
        for path in locked_paths.iter() {
            if let Err(e) = self.backup_path_recursively(path) {
                log_to_file(&format!("BACKUP: Failed to backup {:?}: {}", path, e));
            }
        }
        
        let backup_count = self.file_backups.read().unwrap().len();
        log_to_file(&format!("BACKUP: Created {} file backups", backup_count));
        Ok(())
    }

    fn backup_path_recursively(&self, path: &Path) -> Result<()> {
        if path.is_file() {
            self.backup_single_file(path)?;
        } else if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let entry_path = entry.path();
                // Skip hidden files and directories
                if let Some(name) = entry_path.file_name() {
                    if name.to_string_lossy().starts_with('.') {
                        continue;
                    }
                }
                self.backup_path_recursively(&entry_path)?;
            }
        }
        Ok(())
    }

    fn backup_single_file(&self, path: &Path) -> Result<()> {
        if let Ok(content) = fs::read(path) {
            let mut backups = self.file_backups.write().unwrap();
            backups.insert(path.to_path_buf(), content.clone());
            log_to_file(&format!("BACKUP: Backed up file {:?} ({} bytes)", path, content.len()));
        }
        Ok(())
    }

    // OPTION 1: File Permission Protection (kept for reference but not used)
    fn apply_file_protection(&self) -> Result<()> {
        let locked_paths = self.locked_paths.read().unwrap();
        log_to_file(&format!("OPTION1: Protecting {} locked paths", locked_paths.len()));
        
        for path in locked_paths.iter() {
            if let Err(e) = self.protect_path_recursively(path) {
                log_to_file(&format!("OPTION1: Failed to protect {:?}: {}", path, e));
            }
        }
        Ok(())
    }

    fn protect_path_recursively(&self, path: &Path) -> Result<()> {
        if path.is_file() {
            self.make_file_readonly(path)?;
        } else if path.is_dir() {
            // Recursively protect all files in directory
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                self.protect_path_recursively(&entry.path())?;
            }
        }
        Ok(())
    }

    fn make_file_readonly(&self, path: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        
        let metadata = fs::metadata(path)?;
        let mut perms = metadata.permissions();
        
        // Store original permissions
        let original_mode = perms.mode();
        let mut backups = self.file_backups.write().unwrap();
        // Use a special key to store permissions
        let perm_key = format!("__PERMS__{}", path.display());
        backups.insert(PathBuf::from(perm_key), original_mode.to_le_bytes().to_vec());
        
        // Make read-only (remove write permissions)
        let readonly_mode = original_mode & !0o200; // Remove owner write
        perms.set_mode(readonly_mode);
        fs::set_permissions(path, perms)?;
        
        log_to_file(&format!("OPTION1: Protected file {:?} (mode: {:o} -> {:o})", 
                             path, original_mode, readonly_mode));
        Ok(())
    }

    fn restore_file_protection(&self) -> Result<()> {
        let locked_paths = self.locked_paths.read().unwrap();
        log_to_file(&format!("OPTION1: Restoring {} locked paths", locked_paths.len()));
        
        for path in locked_paths.iter() {
            if let Err(e) = self.restore_path_recursively(path) {
                log_to_file(&format!("OPTION1: Failed to restore {:?}: {}", path, e));
            }
        }
        Ok(())
    }

    fn restore_path_recursively(&self, path: &Path) -> Result<()> {
        if path.is_file() {
            self.restore_file_permissions(path)?;
        } else if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                self.restore_path_recursively(&entry.path())?;
            }
        }
        Ok(())
    }

    fn restore_file_permissions(&self, path: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        
        let backups = self.file_backups.read().unwrap();
        let perm_key = format!("__PERMS__{}", path.display());
        
        if let Some(perm_bytes) = backups.get(&PathBuf::from(perm_key)) {
            if perm_bytes.len() >= 4 {
                let mode_bytes: [u8; 4] = perm_bytes[0..4].try_into().unwrap_or([0; 4]);
                let original_mode = u32::from_le_bytes(mode_bytes);
                
                let mut perms = fs::metadata(path)?.permissions();
                perms.set_mode(original_mode);
                fs::set_permissions(path, perms)?;
                
                log_to_file(&format!("OPTION1: Restored file {:?} to mode {:o}", path, original_mode));
            }
        }
        Ok(())
    }

    fn hash_path(path: &Path) -> String {
        let mut hasher = Sha256::new();
        hasher.update(path.to_string_lossy().as_bytes());
        let result = hasher.finalize();
        
        // Use first 16 chars of hex for reasonable length
        hex::encode(&result[..8])
    }
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            block_timeout: Duration::from_secs(30),
            auto_stash: true,
            monitored_processes: vec![
                "vim".to_string(),
                "nvim".to_string(),
            ],
            block_mode: BlockMode::Revert,
            verbose: false,
        }
    }
}