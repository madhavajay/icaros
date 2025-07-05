use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppState {
    pub root_path: PathBuf,
    #[serde(default)]
    pub locked_patterns: Vec<String>,
    #[serde(default)]
    pub unlocked_patterns: Vec<String>,
    #[serde(default)]
    pub allow_create_patterns: Vec<String>,
    #[serde(default)]
    pub expanded_dirs: Vec<PathBuf>,
}

impl AppState {
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            root_path,
            locked_patterns: Vec::new(),
            unlocked_patterns: vec!["**".to_string()],
            allow_create_patterns: Vec::new(),
            expanded_dirs: Vec::new(),
        }
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let state = serde_json::from_str(&content)?;
        Ok(state)
    }

    pub fn update_expanded_dirs(&mut self, expanded_dirs: Vec<PathBuf>) {
        self.expanded_dirs = expanded_dirs;
    }
    
    pub fn update_from_tree(&mut self, root: &crate::file_tree::TreeNode) {
        self.locked_patterns.clear();
        self.allow_create_patterns.clear();
        self.unlocked_patterns.clear();
        
        let mut locked_info = Vec::new();
        let mut allow_create_info = Vec::new();
        let mut unlocked_info = Vec::new();
        
        collect_lock_info(root, &mut locked_info, &mut allow_create_info, &mut unlocked_info);
        
        self.locked_patterns = optimize_patterns(&locked_info, &self.root_path);
        self.allow_create_patterns = optimize_patterns(&allow_create_info, &self.root_path);
        
        // If we have explicit unlocked patterns, use them
        if !unlocked_info.is_empty() {
            self.unlocked_patterns = optimize_patterns(&unlocked_info, &self.root_path);
        } else {
            // Otherwise calculate based on what's locked
            self.unlocked_patterns = calculate_unlocked_patterns(&self.locked_patterns);
        }
    }
}

#[derive(Clone)]
struct LockInfo {
    path: PathBuf,
    is_dir: bool,
    _all_children_locked: bool,  // Used for optimization logic
}

fn collect_lock_info(
    node: &crate::file_tree::TreeNode, 
    locked: &mut Vec<LockInfo>, 
    allow_create: &mut Vec<LockInfo>,
    unlocked: &mut Vec<LockInfo>
) {
    collect_lock_info_impl(node, locked, allow_create, unlocked, false);
}

fn collect_lock_info_impl(
    node: &crate::file_tree::TreeNode, 
    locked: &mut Vec<LockInfo>, 
    allow_create: &mut Vec<LockInfo>,
    unlocked: &mut Vec<LockInfo>,
    parent_is_locked: bool
) {
    // Track unlocked nodes that are children of locked parents
    // Only track directories or files if their parent directory is locked
    if !node.is_locked && parent_is_locked {
        // Only add directories or files whose immediate parent is locked
        // This prevents adding individual files when their parent directory is already unlocked
        if node.is_dir {
            // For directories, always add them
            let all_children_unlocked = node.children.is_empty() || 
                node.children.iter().all(|c| !c.is_locked);
            
            unlocked.push(LockInfo {
                path: node.path.clone(),
                is_dir: true,
                _all_children_locked: all_children_unlocked,  // Reusing this field to mean "all children unlocked"
            });
        }
        // Skip individual files - we only track directory-level unlocks
    }
    
    // Only add to locked list if this node is explicitly locked and parent isn't locked
    // (to avoid duplication when parent dir is already locked)
    if node.is_locked && !parent_is_locked {
        let all_children_locked = node.is_dir && 
            (node.children.is_empty() || node.children.iter().all(|c| c.is_locked));
        
        locked.push(LockInfo {
            path: node.path.clone(),
            is_dir: node.is_dir,
            _all_children_locked: all_children_locked,
        });
    }
    
    // Always collect allow_create info, even for children of locked directories
    if node.is_dir && node.allow_create_in_locked && node.is_locked {
        let all_children_locked = node.is_dir && 
            (node.children.is_empty() || node.children.iter().all(|c| c.is_locked));
            
        allow_create.push(LockInfo {
            path: node.path.clone(),
            is_dir: true,
            _all_children_locked: all_children_locked,
        });
    }
    
    // Traverse children, passing along whether this node is locked
    for child in &node.children {
        collect_lock_info_impl(child, locked, allow_create, unlocked, node.is_locked || parent_is_locked);
    }
}

fn optimize_patterns(lock_infos: &[LockInfo], root: &Path) -> Vec<String> {
    if lock_infos.is_empty() {
        return Vec::new();
    }
    
    let mut patterns = Vec::new();
    let mut sorted_infos: Vec<_> = lock_infos.to_vec();
    sorted_infos.sort_by(|a, b| a.path.cmp(&b.path));
    
    let mut skip_until = None;
    
    for info in sorted_infos.iter() {
        if let Some(skip_path) = &skip_until {
            if info.path.starts_with(skip_path) {
                continue;
            } else {
                skip_until = None;
            }
        }
        
        let relative = info.path.strip_prefix(root).unwrap_or(&info.path);
        
        let pattern = if &info.path == root {
            // Special case: if the root directory itself is locked
            "**".to_string()
        } else if info.is_dir {
            // For directories, always use /** pattern
            format!("{}/**", relative.display())
        } else {
            // For files, just use the path
            relative.display().to_string()
        };
        
        patterns.push(pattern.clone());
        
        // If we just added a directory with /** pattern, skip all its children
        if info.is_dir && pattern.ends_with("/**") {
            skip_until = Some(info.path.clone());
        }
    }
    
    patterns
}

pub fn calculate_unlocked_patterns(locked_patterns: &[String]) -> Vec<String> {
    if locked_patterns.is_empty() {
        // Nothing is locked, so everything is unlocked
        return vec!["**".to_string()];
    }
    
    // Check if everything is locked
    if locked_patterns.contains(&"**".to_string()) {
        // Everything is locked, so nothing is unlocked
        return vec![];
    }
    
    // For now, we'll return a simple representation
    // In a more complex implementation, we could calculate the inverse of locked patterns
    // But for the current use case, we'll just indicate if there are unlocked areas
    let mut unlocked = Vec::new();
    
    // If only specific paths are locked, then other paths are unlocked
    // This is a simplified representation - in reality, calculating the exact
    // complement of glob patterns is complex
    if !locked_patterns.iter().any(|p| p == "**" || p == "/**") {
        // Some specific paths are locked, so indicate that other paths are unlocked
        unlocked.push("**".to_string());
    }
    
    unlocked
}