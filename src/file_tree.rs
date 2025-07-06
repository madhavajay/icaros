use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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
        if !self.is_locked {
            self.allow_create_in_locked = false;
        }
        // Don't automatically lock/unlock children - that's handled by the UI
    }

    pub fn toggle_create_in_locked(&mut self) {
        if self.is_dir && self.is_locked {
            self.allow_create_in_locked = !self.allow_create_in_locked;
        }
    }

    pub fn lock_all_children(&mut self) {
        for child in &mut self.children {
            child.is_locked = true;
            child.allow_create_in_locked = false;
            child.lock_all_children();
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

pub fn build_tree(
    root_path: &Path,
    custom_ignore_patterns: &[String],
    show_hidden: bool,
) -> Result<TreeNode> {
    let root_name = root_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut root = TreeNode::new(root_path.to_path_buf(), root_name, true, 0);

    let mut stack = vec![(root_path.to_path_buf(), &mut root as *mut TreeNode)];

    for entry in WalkDir::new(root_path)
        .min_depth(1)
        .sort_by_file_name()
        .follow_links(false)
    // Don't follow symlinks to avoid issues
    {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                // Log the error but continue processing other files
                eprintln!("Warning: Skipping entry due to IO error: {}", err);
                continue;
            }
        };
        let path = entry.path();

        if should_ignore(path, custom_ignore_patterns, show_hidden) {
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

fn should_ignore(path: &Path, patterns: &[String], show_hidden: bool) -> bool {
    let path_str = path.to_string_lossy();

    // Check if it's a hidden file (starts with .)
    if !show_hidden {
        if let Some(file_name) = path.file_name() {
            if let Some(name_str) = file_name.to_str() {
                if name_str.starts_with('.') && name_str != "." && name_str != ".." {
                    return true;
                }
            }
        }
    }

    // Check against ignore patterns
    for pattern in patterns {
        if pattern.contains('*') {
            // Simple glob pattern matching for * wildcards
            if pattern.ends_with("*") {
                let prefix = &pattern[..pattern.len() - 1];
                if let Some(file_name) = path.file_name() {
                    if file_name.to_string_lossy().starts_with(prefix) {
                        return true;
                    }
                }
            } else if let Some(extension) = pattern.strip_prefix("*.") {
                if let Some(ext) = path.extension() {
                    if ext.to_string_lossy() == extension {
                        return true;
                    }
                }
            }
        } else if pattern.ends_with('/') {
            // Directory pattern - check if path contains this directory
            if path_str.contains(&format!("/{}", pattern)) || path_str.contains(pattern) {
                return true;
            }
        } else {
            // Exact file name match
            if let Some(file_name) = path.file_name() {
                if file_name.to_string_lossy() == *pattern {
                    return true;
                }
            }
            // Also check if the pattern is contained in the path
            if path_str.contains(pattern) {
                return true;
            }
        }
    }

    false
}
