// OPTION 2: Process Monitoring with File Locking
// This approach uses file locking and process detection to prevent access

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use chrono::Utc;

#[cfg(target_os = "macos")]
use std::os::unix::io::AsRawFd;

fn log_to_file(message: &str) {
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let log_message = format!("[{}] {}\n", timestamp, message);
    
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("logs/unified.log")
    {
        let _ = file.write_all(log_message.as_bytes());
        let _ = file.flush();
    }
}

pub struct ProcessFileLocker {
    locked_paths: Arc<RwLock<Vec<PathBuf>>>,
    file_locks: Arc<RwLock<HashMap<PathBuf, File>>>,
    monitored_processes: Vec<String>,
    monitoring_thread: Option<thread::JoinHandle<()>>,
    should_stop: Arc<RwLock<bool>>,
}

impl ProcessFileLocker {
    pub fn new(monitored_processes: Vec<String>) -> Self {
        Self {
            locked_paths: Arc::new(RwLock::new(Vec::new())),
            file_locks: Arc::new(RwLock::new(HashMap::new())),
            monitored_processes,
            monitoring_thread: None,
            should_stop: Arc::new(RwLock::new(false)),
        }
    }

    pub fn start(&mut self, locked_paths: Vec<PathBuf>) -> Result<()> {
        log_to_file("OPTION2: Starting process-based file locking");
        
        // Store locked paths
        {
            let mut paths = self.locked_paths.write().unwrap();
            *paths = locked_paths.clone();
        }

        // Apply file locks
        self.apply_file_locks(&locked_paths)?;

        // Start process monitoring
        self.start_process_monitoring()?;

        log_to_file("OPTION2: Process file locker started successfully");
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        log_to_file("OPTION2: Stopping process file locker");
        
        // Signal monitoring thread to stop
        *self.should_stop.write().unwrap() = true;
        
        // Wait for monitoring thread to finish
        if let Some(handle) = self.monitoring_thread.take() {
            let _ = handle.join();
        }

        // Release file locks
        self.release_file_locks()?;

        log_to_file("OPTION2: Process file locker stopped");
        Ok(())
    }

    fn apply_file_locks(&self, paths: &[PathBuf]) -> Result<()> {
        let mut locks = self.file_locks.write().unwrap();
        
        for path in paths {
            if let Err(e) = self.lock_path_recursively(path, &mut locks) {
                log_to_file(&format!("OPTION2: Failed to lock {:?}: {}", path, e));
            }
        }
        
        log_to_file(&format!("OPTION2: Applied locks to {} files", locks.len()));
        Ok(())
    }

    fn lock_path_recursively(&self, path: &Path, locks: &mut HashMap<PathBuf, File>) -> Result<()> {
        if path.is_file() {
            self.lock_single_file(path, locks)?;
        } else if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                self.lock_path_recursively(&entry.path(), locks)?;
            }
        }
        Ok(())
    }

    fn lock_single_file(&self, path: &Path, locks: &mut HashMap<PathBuf, File>) -> Result<()> {
        // Open file for reading and apply advisory lock
        match File::open(path) {
            Ok(file) => {
                #[cfg(target_os = "macos")]
                {
                    // Apply advisory lock using flock
                    let fd = file.as_raw_fd();
                    let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
                    
                    if result == 0 {
                        locks.insert(path.to_path_buf(), file);
                        log_to_file(&format!("OPTION2: Locked file {:?}", path));
                    } else {
                        log_to_file(&format!("OPTION2: Failed to lock {:?} (already locked?)", path));
                    }
                }
                
                #[cfg(not(target_os = "macos"))]
                {
                    // For non-macOS, just track the file handle
                    locks.insert(path.to_path_buf(), file);
                    log_to_file(&format!("OPTION2: Tracked file {:?}", path));
                }
            }
            Err(e) => {
                log_to_file(&format!("OPTION2: Cannot open file {:?}: {}", path, e));
            }
        }
        Ok(())
    }

    fn release_file_locks(&self) -> Result<()> {
        let mut locks = self.file_locks.write().unwrap();
        
        #[cfg(target_os = "macos")]
        {
            // Release flock locks
            for (path, file) in locks.iter() {
                let fd = file.as_raw_fd();
                let result = unsafe { libc::flock(fd, libc::LOCK_UN) };
                if result == 0 {
                    log_to_file(&format!("OPTION2: Released lock on {:?}", path));
                } else {
                    log_to_file(&format!("OPTION2: Failed to release lock on {:?}", path));
                }
            }
        }
        
        locks.clear();
        log_to_file("OPTION2: All file locks released");
        Ok(())
    }

    fn start_process_monitoring(&mut self) -> Result<()> {
        let monitored_processes = self.monitored_processes.clone();
        let should_stop = self.should_stop.clone();
        
        let handle = thread::spawn(move || {
            log_to_file("OPTION2: Process monitoring thread started");
            
            while !*should_stop.read().unwrap() {
                // Check for monitored processes
                for process_name in &monitored_processes {
                    if let Err(e) = Self::check_and_kill_process(process_name) {
                        log_to_file(&format!("OPTION2: Error checking process {}: {}", process_name, e));
                    }
                }
                
                thread::sleep(Duration::from_millis(500)); // Check every 500ms
            }
            
            log_to_file("OPTION2: Process monitoring thread stopped");
        });
        
        self.monitoring_thread = Some(handle);
        Ok(())
    }

    fn check_and_kill_process(process_name: &str) -> Result<()> {
        // Use pgrep to find processes
        let output = Command::new("pgrep")
            .arg("-f") // Match full command line
            .arg(process_name)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        
        match output {
            Ok(result) if result.status.success() => {
                let pids_str = String::from_utf8_lossy(&result.stdout);
                let pids: Vec<&str> = pids_str.trim().lines().collect();
                
                if !pids.is_empty() {
                    log_to_file(&format!("OPTION2: Found {} {} processes: {:?}", 
                                         pids.len(), process_name, pids));
                    
                    // Check if these processes are trying to access locked files
                    for pid in pids {
                        if let Err(e) = Self::check_process_file_access(pid, process_name) {
                            log_to_file(&format!("OPTION2: Error checking process {}: {}", pid, e));
                        }
                    }
                }
            }
            Ok(_) => {
                // No processes found, which is fine
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to run pgrep: {}", e));
            }
        }
        
        Ok(())
    }

    fn check_process_file_access(pid: &str, process_name: &str) -> Result<()> {
        // Use lsof to check what files the process has open
        let output = Command::new("lsof")
            .arg("-p")
            .arg(pid)
            .arg("-Fn") // Only output file names
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        
        match output {
            Ok(result) if result.status.success() => {
                let files_str = String::from_utf8_lossy(&result.stdout);
                
                // Look for locked file patterns
                for line in files_str.lines() {
                    if line.starts_with('n') && (line.contains("/src/") || line.contains("/config/")) {
                        let file_path = &line[1..]; // Remove 'n' prefix
                        log_to_file(&format!("OPTION2: Process {} (PID: {}) accessing potentially locked file: {}", 
                                             process_name, pid, file_path));
                        
                        // Send SIGSTOP to pause the process
                        if let Err(e) = Self::pause_process(pid) {
                            log_to_file(&format!("OPTION2: Failed to pause process {}: {}", pid, e));
                        } else {
                            log_to_file(&format!("OPTION2: Paused process {} (PID: {})", process_name, pid));
                        }
                    }
                }
            }
            Ok(_) => {
                // Process might have exited or lsof failed
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to run lsof: {}", e));
            }
        }
        
        Ok(())
    }

    fn pause_process(pid: &str) -> Result<()> {
        Command::new("kill")
            .arg("-STOP")
            .arg(pid)
            .status()
            .context("Failed to send SIGSTOP")?;
        Ok(())
    }

    #[allow(dead_code)]
    fn resume_process(pid: &str) -> Result<()> {
        Command::new("kill")
            .arg("-CONT")
            .arg(pid)
            .status()
            .context("Failed to send SIGCONT")?;
        Ok(())
    }
}