use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub is_expanded: bool,
    pub is_locked: bool,
    pub allow_create_in_locked: bool,
    pub children: Vec<TreeNode>,
    pub depth: usize,
}

impl TreeNode {
    pub fn new(path: PathBuf, name: String, is_dir: bool, depth: usize) -> Self {
        Self {
            path,
            name,
            is_dir,
            is_expanded: false,
            is_locked: false,
            allow_create_in_locked: false,
            children: Vec::new(),
            depth,
        }
    }

    pub fn toggle_lock(&mut self) {
        self.is_locked = !self.is_locked;
        if self.is_locked {
            self.allow_create_in_locked = false;
            self.lock_all_children();
        } else {
            self.unlock_all_children();
        }
    }
    
    pub fn toggle_create_in_locked(&mut self) {
        if self.is_dir && self.is_locked {
            self.allow_create_in_locked = !self.allow_create_in_locked;
        }
    }

    pub fn lock_all_children(&mut self) {
        self.is_locked = true;
        self.allow_create_in_locked = false;
        for child in &mut self.children {
            child.lock_all_children();
        }
    }

    fn unlock_all_children(&mut self) {
        self.is_locked = false;
        self.allow_create_in_locked = false;
        for child in &mut self.children {
            child.unlock_all_children();
        }
    }

    pub fn toggle_expand(&mut self) {
        if self.is_dir {
            self.is_expanded = !self.is_expanded;
        }
    }

    pub fn get_locked_files(&self) -> Vec<PathBuf> {
        let mut locked = Vec::new();
        if self.is_locked && !self.is_dir {
            locked.push(self.path.clone());
        }
        for child in &self.children {
            locked.extend(child.get_locked_files());
        }
        locked
    }
}

pub fn build_tree(root_path: &Path, ignore_patterns: &[String]) -> Result<TreeNode> {
    let root_name = root_path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    
    let mut root = TreeNode::new(root_path.to_path_buf(), root_name, true, 0);
    
    let mut stack = vec![(root_path.to_path_buf(), &mut root as *mut TreeNode)];
    
    for entry in WalkDir::new(root_path)
        .min_depth(1)
        .sort_by_file_name()
    {
        let entry = entry?;
        let path = entry.path();
        
        if should_ignore(path, ignore_patterns) {
            continue;
        }
        
        let depth = entry.depth();
        let _parent_path = path.parent().unwrap().to_path_buf();
        
        while stack.len() > depth {
            stack.pop();
        }
        
        let node = TreeNode::new(
            path.to_path_buf(),
            path.file_name().unwrap().to_string_lossy().to_string(),
            entry.file_type().is_dir(),
            depth,
        );
        
        unsafe {
            let parent = &mut *stack.last().unwrap().1;
            parent.children.push(node);
            
            if entry.file_type().is_dir() {
                let last_child = parent.children.last_mut().unwrap();
                stack.push((path.to_path_buf(), last_child as *mut TreeNode));
            }
        }
    }
    
    Ok(root)
}

fn should_ignore(path: &Path, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    
    if path_str.contains("/.git/") || path_str.contains("/target/") || 
       path_str.contains("/node_modules/") || path_str.contains("/.idea/") {
        return true;
    }
    
    for pattern in patterns {
        if path_str.contains(pattern) {
            return true;
        }
    }
    
    false
}