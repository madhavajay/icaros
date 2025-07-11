use crate::animations::AnimationEngine;
use crate::file_tree::TreeNode;
use crate::git::{GitFile, GitHunk, GitManager};
use crate::log_debug;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use notify::{Event as NotifyEvent, RecursiveMode, Watcher};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::collections::HashSet;
use std::io;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

pub struct App {
    pub tree: TreeNode,
    pub list_state: ListState,
    pub items: Vec<(TreeNode, usize)>,
    pub selected: usize,
    pub state_file: std::path::PathBuf,
    pub root_path: std::path::PathBuf,
    pub frame_count: u64,
    pub _last_update: Instant,
    pub animations_enabled: bool,
    pub llama_x: f32,
    pub day_night_cycle: f32,
    pub wave_offset: f32,
    pub needs_refresh: bool,
    pub last_refresh: Instant,
    pub explicitly_locked_paths: Vec<std::path::PathBuf>,
    pub explicitly_unlocked_paths: Vec<std::path::PathBuf>,
    pub show_hidden: bool,
    // Tab support
    pub active_tab: TabIndex,
    // Git support
    pub git_manager: Option<GitManager>,
    pub git_files: Vec<GitFile>,
    pub git_file_list_state: ListState,
    pub git_selected_file: usize,
    pub git_diff_hunks: Vec<GitHunk>,
    pub git_diff_scroll: u16,
    pub git_selected_hunk: usize,
    pub git_pane: GitPane,
    pub show_help: bool,
    // Profile system
    pub profile_list_state: ListState,
    pub profile_names: Vec<String>,
    pub active_profile_name: Option<String>,
    pub profile_input_mode: bool,
    pub profile_input_buffer: String,
    pub profile_action: ProfileAction,
    // Animation system
    pub profile_switching: bool,
    // Simple animation engine
    pub animation_engine: AnimationEngine,
    // Delayed profile switch
    pub pending_profile_switch: Option<String>,
    // Image to display
    pub current_image_path: Option<String>,
    // Stateful image protocol for better rendering
    pub image_state: Option<Box<dyn ratatui_image::protocol::Protocol>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TabIndex {
    FileGuardian,
    GitStage,
    Profiles,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProfileAction {
    None,
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GitPane {
    FileList,
    DiffView,
}

const NATIVE_PATTERNS: &[&str] = &[
    "◇", "◈", "◊", "⟡", "✦", "✧", "◉", "◎", "▲", "▼", "◆", "♦", "⬟", "⬢", "⬣", "⬡",
];

const DESERT_ELEMENTS: &[&str] = &["🌵", "🏜️", "⛰️", "🪨", "🌄", "🌅"];

const TIME_ELEMENTS: &[(&str, &str)] = &[
    ("☀️", "Dawn"),
    ("🌞", "Day"),
    ("🌅", "Dusk"),
    ("🌙", "Night"),
    ("⭐", "Midnight"),
];

const EARTH_COLORS: &[Color] = &[
    Color::Rgb(184, 134, 11),  // Dark goldenrod
    Color::Rgb(210, 105, 30),  // Chocolate
    Color::Rgb(205, 133, 63),  // Peru
    Color::Rgb(222, 184, 135), // Burlywood
    Color::Rgb(160, 82, 45),   // Sienna
    Color::Rgb(139, 69, 19),   // Saddle brown
    Color::Rgb(255, 140, 0),   // Dark orange
    Color::Rgb(218, 165, 32),  // Goldenrod
];

const SUNSET_GRADIENT: &[Color] = &[
    Color::Rgb(255, 94, 77),  // Sunset red
    Color::Rgb(255, 140, 0),  // Dark orange
    Color::Rgb(255, 206, 84), // Sunset yellow
    Color::Rgb(237, 117, 56), // Sunset orange
    Color::Rgb(95, 39, 205),  // Sunset purple
    Color::Rgb(52, 31, 151),  // Deep purple
    Color::Rgb(0, 0, 70),     // Night blue
];

impl App {
    pub fn new(
        tree: TreeNode,
        state_file: std::path::PathBuf,
        root_path: std::path::PathBuf,
    ) -> Self {
        // Try to initialize Git manager
        let git_manager = GitManager::new(&root_path).ok();
        let git_files = if let Some(ref git) = git_manager {
            git.get_status_files().unwrap_or_else(|_| Vec::new())
        } else {
            Vec::new()
        };

        let mut app = Self {
            tree,
            list_state: ListState::default(),
            items: Vec::new(),
            selected: 0,
            state_file,
            root_path,
            frame_count: 0,
            _last_update: Instant::now(),
            animations_enabled: true,
            llama_x: 0.0,
            day_night_cycle: 0.0,
            wave_offset: 0.0,
            needs_refresh: false,
            last_refresh: Instant::now(),
            explicitly_locked_paths: Vec::new(),
            explicitly_unlocked_paths: Vec::new(),
            show_hidden: false,
            active_tab: TabIndex::FileGuardian,
            git_manager,
            git_files,
            git_file_list_state: ListState::default(),
            git_selected_file: 0,
            git_diff_hunks: Vec::new(),
            git_diff_scroll: 0,
            git_selected_hunk: 0,
            git_pane: GitPane::FileList,
            show_help: false,
            profile_list_state: ListState::default(),
            profile_names: Vec::new(),
            active_profile_name: None,
            profile_input_mode: false,
            profile_input_buffer: String::new(),
            profile_action: ProfileAction::None,
            profile_switching: false,
            animation_engine: AnimationEngine::new(),
            pending_profile_switch: None,
            current_image_path: None,
            image_state: None,
        };
        app.update_items();
        app.list_state.select(Some(0));
        if !app.git_files.is_empty() {
            app.git_file_list_state.select(Some(0));
        }

        // Load animation spells
        log_debug!("UI: Loading animation spells");
        if let Err(e) = app.animation_engine.load_spells() {
            log_debug!("UI: ERROR - Failed to load animation spells: {}", e);
            eprintln!("Warning: Failed to load animation spells: {e}");
        } else {
            log_debug!("UI: Animation spells loaded successfully");
        }

        app
    }

    fn update_animations(&mut self, width: u16) {
        // Update llama position (slow wandering)
        self.llama_x += 0.3;
        if self.llama_x > width as f32 + 10.0 {
            self.llama_x = -5.0;
        }

        // Update day/night cycle
        self.day_night_cycle += 0.005;
        if self.day_night_cycle > 1.0 {
            self.day_night_cycle = 0.0;
        }

        // Update wave offset for gradient effects
        self.wave_offset += 0.1;
    }

    pub fn update_items(&mut self) {
        self.items.clear();
        let tree_clone = self.tree.clone();
        self.collect_visible_nodes(&tree_clone, 0);
    }

    fn collect_visible_nodes(&mut self, node: &TreeNode, indent: usize) {
        // Skip hidden files unless show_hidden is true
        if !self.show_hidden && node.name.starts_with('.') && indent > 0 {
            return;
        }

        self.items.push((node.clone(), indent));

        if node.is_dir && node.is_expanded {
            for child in &node.children {
                self.collect_visible_nodes(child, indent + 1);
            }
        }
    }

    pub fn toggle_selected(&mut self) {
        if self.selected < self.items.len() {
            let path = self.items[self.selected].0.path.clone();
            let is_dir = self.items[self.selected].0.is_dir;

            // Determine the effective lock state of this path
            let was_locked = self.is_path_effectively_locked(&path);

            if std::env::var("ICAROS_DEBUG").is_ok() {
                eprintln!("Toggle: {path:?}, was_locked: {was_locked}");
                eprintln!("  Explicitly locked: {:?}", self.explicitly_locked_paths);
                eprintln!(
                    "  Explicitly unlocked: {:?}",
                    self.explicitly_unlocked_paths
                );
            }

            if !was_locked {
                // LOCKING a node
                self.explicitly_locked_paths.push(path.clone());

                // Trigger lock animation
                self.animation_engine.trigger("file_locked");

                // Remove this path from explicitly unlocked if it was there
                self.explicitly_unlocked_paths.retain(|p| p != &path);

                // If locking a directory, clean up redundant child states
                if is_dir {
                    // Remove child locks (they're now redundant)
                    self.explicitly_locked_paths
                        .retain(|p| !p.starts_with(&path) || p == &path);
                    // Remove child unlocks (they're overridden by the lock)
                    self.explicitly_unlocked_paths
                        .retain(|p| !p.starts_with(&path));
                }
            } else {
                // UNLOCKING a node
                // First check if this is an explicit lock
                let is_explicitly_locked = self.explicitly_locked_paths.contains(&path);

                if is_explicitly_locked {
                    // Remove the explicit lock
                    self.explicitly_locked_paths.retain(|p| p != &path);

                    // Trigger unlock animation
                    self.animation_engine.trigger("file_unlocked");

                    // If unlocking a directory that was explicitly locked,
                    // remove redundant child states
                    if is_dir {
                        // Remove child locks (parent is now unlocked)
                        self.explicitly_locked_paths
                            .retain(|p| !p.starts_with(&path));
                        // Remove child unlocks (they're redundant now)
                        self.explicitly_unlocked_paths
                            .retain(|p| !p.starts_with(&path));
                    }
                } else {
                    // This is an inherited lock, check if we need to explicitly unlock
                    let has_locked_parent = self.has_locked_ancestor(&path);

                    if has_locked_parent {
                        // Add explicit unlock
                        self.explicitly_unlocked_paths.push(path.clone());

                        // If unlocking a directory, remove child states
                        if is_dir {
                            // Remove child locks
                            self.explicitly_locked_paths
                                .retain(|p| !p.starts_with(&path) || p == &path);
                            // Remove child unlocks
                            self.explicitly_unlocked_paths
                                .retain(|p| !p.starts_with(&path) || p == &path);
                        }
                    }
                }
            }

            // Clean up redundant entries
            self.cleanup_lock_lists();

            // Reapply all locks to ensure correct state
            self.reapply_explicit_locks();

            self.update_items();

            // Ensure selection stays on the same path
            for (i, (item_node, _)) in self.items.iter().enumerate() {
                if item_node.path == path {
                    self.selected = i;
                    self.list_state.select(Some(i));
                    break;
                }
            }

            self.save_state();
        }
    }

    fn is_path_effectively_locked(&self, path: &std::path::Path) -> bool {
        // First check if this exact path is explicitly unlocked
        if self.explicitly_unlocked_paths.contains(&path.to_path_buf()) {
            return false;
        }

        // Then check if this exact path is explicitly locked
        if self.explicitly_locked_paths.contains(&path.to_path_buf()) {
            return true;
        }

        // Now check parent paths from most specific to least specific
        let mut current = path;
        while let Some(parent) = current.parent() {
            // Check if any parent is explicitly unlocked
            if self.explicitly_unlocked_paths.iter().any(|p| p == parent) {
                return false;
            }

            // Check if any parent is explicitly locked
            if self.explicitly_locked_paths.iter().any(|p| p == parent) {
                // Before returning true, check if there's an unlock between this parent and our path
                let locked_parent = parent;
                for unlock_path in &self.explicitly_unlocked_paths {
                    // If unlock_path is between locked_parent and path
                    if path.starts_with(unlock_path) && unlock_path.starts_with(locked_parent) {
                        return false;
                    }
                }
                return true;
            }

            current = parent;
        }

        // No explicit lock or unlock found in the hierarchy
        false
    }

    fn has_locked_ancestor(&self, path: &std::path::Path) -> bool {
        // Check if any ancestor is locked (excluding the path itself)
        let mut current = path;
        while let Some(parent) = current.parent() {
            if self.explicitly_locked_paths.iter().any(|p| p == parent) {
                // Check if there's an unlock between this parent and our path
                for unlock_path in &self.explicitly_unlocked_paths {
                    if path.starts_with(unlock_path) && unlock_path.starts_with(parent) {
                        return false;
                    }
                }
                return true;
            }
            current = parent;
        }
        false
    }

    fn _has_unlocked_ancestor(&self, path: &std::path::Path) -> bool {
        for unlocked_path in &self.explicitly_unlocked_paths {
            if unlocked_path != path && path.starts_with(unlocked_path) {
                return true;
            }
        }
        false
    }

    pub fn cleanup_lock_lists(&mut self) {
        // Remove duplicates
        let mut seen_locked = HashSet::new();
        self.explicitly_locked_paths
            .retain(|path| seen_locked.insert(path.clone()));

        let mut seen_unlocked = HashSet::new();
        self.explicitly_unlocked_paths
            .retain(|path| seen_unlocked.insert(path.clone()));

        // Remove any paths that are in both lists (unlocked takes precedence)
        let unlocked_set: HashSet<_> = self.explicitly_unlocked_paths.iter().cloned().collect();
        self.explicitly_locked_paths
            .retain(|path| !unlocked_set.contains(path));

        // Remove redundant unlocks (unlocks without a parent lock)
        let locked_paths = self.explicitly_locked_paths.clone();
        self.explicitly_unlocked_paths.retain(|unlock_path| {
            // Check if this unlock path has a locked ancestor
            for locked_path in &locked_paths {
                if locked_path != unlock_path && unlock_path.starts_with(locked_path) {
                    return true; // Keep this unlock
                }
            }
            false // Remove this unlock (no locked ancestor)
        });
    }

    pub fn toggle_expand_selected(&mut self) {
        if self.selected < self.items.len() {
            let path = self.items[self.selected].0.path.clone();
            toggle_expand_at_path(&mut self.tree, &path);
            self.update_items();
            self.save_state();
        }
    }

    pub fn toggle_create_in_locked_selected(&mut self) {
        if self.selected < self.items.len() {
            let path = self.items[self.selected].0.path.clone();
            toggle_create_in_locked_at_path(&mut self.tree, &path);
            self.update_items();
            self.save_state();
        }
    }

    fn save_state(&self) {
        // Load existing state to preserve profiles, or create new one if it doesn't exist
        let mut state = crate::state::AppState::load_from_file(&self.state_file)
            .unwrap_or_else(|_| crate::state::AppState::new(self.root_path.clone()));

        if std::env::var("ICAROS_DEBUG").is_ok() {
            eprintln!(
                "save_state: loaded {} profiles, active_profile: {:?}",
                state.profiles.len(),
                state.active_profile
            );
        }

        state.update_expanded_dirs(self.get_expanded_dirs());

        // Convert explicit paths to patterns with deduplication
        let mut locked_patterns = std::collections::HashSet::new();
        for path in &self.explicitly_locked_paths {
            if let Ok(relative) = path.strip_prefix(&self.root_path) {
                if relative.as_os_str().is_empty() {
                    locked_patterns.insert("**".to_string());
                } else {
                    let pattern = if path.is_dir() {
                        format!("{}/**", relative.display())
                    } else {
                        relative.display().to_string()
                    };
                    locked_patterns.insert(pattern);
                }
            }
        }

        // Save explicitly unlocked patterns with deduplication
        let mut unlocked_patterns = std::collections::HashSet::new();
        for path in &self.explicitly_unlocked_paths {
            if let Ok(relative) = path.strip_prefix(&self.root_path) {
                let pattern = if path.is_dir() {
                    format!("{}/**", relative.display())
                } else {
                    relative.display().to_string()
                };
                unlocked_patterns.insert(pattern);
            }
        }

        // Remove any patterns that appear in both locked and unlocked
        // (unlocked takes precedence)
        for pattern in &unlocked_patterns {
            locked_patterns.remove(pattern);
        }

        // Convert to vectors
        let mut locked_vec: Vec<String> = locked_patterns.into_iter().collect();
        let mut unlocked_vec: Vec<String> = unlocked_patterns.into_iter().collect();

        // Optimize patterns - remove redundant ones
        // For locked patterns, we need to consider unlocked patterns too
        locked_vec = optimize_patterns_with_context(locked_vec, &unlocked_vec);
        unlocked_vec = optimize_patterns(unlocked_vec);

        // Sort for consistent output
        locked_vec.sort();
        unlocked_vec.sort();

        state.locked_patterns = locked_vec.clone();
        state.unlocked_patterns = unlocked_vec.clone();

        if std::env::var("ICAROS_DEBUG").is_ok() {
            eprintln!("Saving patterns:");
            eprintln!("  Locked: {locked_vec:?}");
            eprintln!("  Unlocked: {unlocked_vec:?}");
        }

        if std::env::var("ICAROS_DEBUG").is_ok() {
            eprintln!(
                "save_state: saving {} profiles, active_profile: {:?}",
                state.profiles.len(),
                state.active_profile
            );
        }

        if let Err(e) = state.save_to_file(&self.state_file) {
            eprintln!("Error saving state: {e}");
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.list_state.select(Some(self.selected));
        }
    }

    pub fn move_down(&mut self) {
        if self.selected < self.items.len() - 1 {
            self.selected += 1;
            self.list_state.select(Some(self.selected));
        }
    }

    pub fn get_locked_files(&self) -> Vec<std::path::PathBuf> {
        // Return only explicitly locked paths, not inherited ones
        self.explicitly_locked_paths.clone()
    }

    pub fn get_unlocked_files(&self) -> Vec<std::path::PathBuf> {
        self.explicitly_unlocked_paths.clone()
    }

    pub fn get_expanded_dirs(&self) -> Vec<std::path::PathBuf> {
        let mut expanded = Vec::new();
        self.collect_expanded_dirs(&self.tree, &mut expanded);
        expanded
    }

    pub fn refresh_tree(&mut self) -> Result<()> {
        // Save current state
        let expanded_dirs = self.get_expanded_dirs();
        let current_selected_path = if self.selected < self.items.len() {
            Some(self.items[self.selected].0.path.clone())
        } else {
            None
        };

        // Load ignore patterns from state or use defaults
        let ignore_patterns =
            if let Ok(state) = crate::state::AppState::load_from_file(&self.state_file) {
                state.ignore_patterns
            } else {
                crate::state::default_ignore_patterns()
            };

        // Rebuild tree with ignore patterns and hidden file filter
        self.tree =
            crate::file_tree::build_tree(&self.root_path, &ignore_patterns, self.show_hidden)?;

        // Reapply all explicit locks
        self.reapply_explicit_locks();

        // Restore expanded state
        for expanded_path in &expanded_dirs {
            restore_expanded_state(&mut self.tree, expanded_path);
        }

        // Update items
        self.update_items();

        // Try to restore selection
        if let Some(selected_path) = current_selected_path {
            for (i, (node, _)) in self.items.iter().enumerate() {
                if node.path == selected_path {
                    self.selected = i;
                    self.list_state.select(Some(i));
                    break;
                }
            }
        }

        self.save_state();
        self.needs_refresh = false;
        self.last_refresh = Instant::now();
        Ok(())
    }

    pub fn reapply_explicit_locks(&mut self) {
        // First, unlock everything
        unlock_all_recursive(&mut self.tree);

        // Sort paths by depth (parent paths first)
        let mut sorted_locked = self.explicitly_locked_paths.clone();
        sorted_locked.sort_by_key(|p| p.components().count());

        let mut sorted_unlocked = self.explicitly_unlocked_paths.clone();
        sorted_unlocked.sort_by_key(|p| p.components().count());

        // Apply locks and unlocks in order of depth
        let mut all_paths: Vec<(std::path::PathBuf, bool)> = Vec::new();
        for path in sorted_locked {
            all_paths.push((path, true)); // true = lock
        }
        for path in sorted_unlocked {
            all_paths.push((path, false)); // false = unlock
        }

        // Sort by depth, then by lock/unlock (locks before unlocks at same depth)
        all_paths.sort_by(|a, b| {
            let depth_a = a.0.components().count();
            let depth_b = b.0.components().count();
            match depth_a.cmp(&depth_b) {
                std::cmp::Ordering::Equal => {
                    // At same depth, apply unlocks first, then locks
                    // This ensures specific locks can override general unlocks
                    b.1.cmp(&a.1)
                }
                other => other,
            }
        });

        // Apply in order
        for (path, is_lock) in all_paths {
            if is_lock {
                lock_path_and_children(&mut self.tree, &path);
            } else {
                // For unlocks, check if there are any explicit locks that should be preserved
                let child_locks: Vec<_> = self
                    .explicitly_locked_paths
                    .iter()
                    .filter(|p| p.starts_with(&path) && *p != &path)
                    .cloned()
                    .collect();

                unlock_path(&mut self.tree, &path);

                // Reapply any child locks that should be preserved
                for child_lock in child_locks {
                    lock_path_and_children(&mut self.tree, &child_lock);
                }
            }
        }
    }

    fn collect_expanded_dirs(&self, node: &TreeNode, expanded: &mut Vec<std::path::PathBuf>) {
        let _ = self; // Suppress warning about self not being used
        if node.is_dir && node.is_expanded {
            expanded.push(node.path.clone());
        }
        for child in &node.children {
            self.collect_expanded_dirs(child, expanded);
        }
    }

    // Git-related methods
    pub fn refresh_git_status(&mut self) {
        if let Some(ref git) = self.git_manager {
            if let Ok(files) = git.get_status_files() {
                self.git_files = files;
                // Reset selection if list is not empty
                if !self.git_files.is_empty() && self.git_selected_file >= self.git_files.len() {
                    self.git_selected_file = 0;
                    self.git_file_list_state.select(Some(0));
                }
            }
        }
    }

    pub fn load_git_diff(&mut self) {
        if let Some(ref git) = self.git_manager {
            if self.git_selected_file < self.git_files.len() {
                let file = &self.git_files[self.git_selected_file];
                if let Ok(hunks) = git.get_file_diff(&file.path, file.staged) {
                    self.git_diff_hunks = hunks;
                    self.git_diff_scroll = 0;
                    self.git_selected_hunk = 0;
                }
            }
        }
    }

    pub fn toggle_git_file_stage(&mut self) {
        if let Some(ref git) = self.git_manager {
            if self.git_selected_file < self.git_files.len() {
                let file = &self.git_files[self.git_selected_file];
                let result = if file.staged {
                    git.unstage_file(&file.path)
                } else {
                    git.stage_file(&file.path)
                };

                if result.is_ok() {
                    self.refresh_git_status();
                    self.load_git_diff();
                }
            }
        }
    }

    pub fn move_git_file_up(&mut self) {
        if self.git_selected_file > 0 {
            self.git_selected_file -= 1;
            self.git_file_list_state
                .select(Some(self.git_selected_file));
            self.load_git_diff();
        }
    }

    pub fn move_git_file_down(&mut self) {
        if self.git_selected_file < self.git_files.len().saturating_sub(1) {
            self.git_selected_file += 1;
            self.git_file_list_state
                .select(Some(self.git_selected_file));
            self.load_git_diff();
        }
    }

    pub fn move_git_hunk_up(&mut self) {
        if self.git_selected_hunk > 0 {
            self.git_selected_hunk -= 1;
            // TODO: Adjust scroll to ensure hunk is visible
        }
    }

    pub fn move_git_hunk_down(&mut self) {
        if self.git_selected_hunk < self.git_diff_hunks.len().saturating_sub(1) {
            self.git_selected_hunk += 1;
            // TODO: Adjust scroll to ensure hunk is visible
        }
    }

    pub fn scroll_git_diff_up(&mut self) {
        self.git_diff_scroll = self.git_diff_scroll.saturating_sub(1);
    }

    pub fn scroll_git_diff_down(&mut self) {
        // TODO: Add max scroll based on content
        self.git_diff_scroll += 1;
    }

    // Profile management methods
    pub fn load_profiles(&mut self) {
        if let Ok(state) = crate::state::AppState::load_from_file(&self.state_file) {
            self.profile_names = state.get_profile_names();
            self.active_profile_name = state.get_active_profile_name().cloned();
            if !self.profile_names.is_empty() {
                self.profile_list_state.select(Some(0));
            }
        }
    }

    pub fn move_profile_up(&mut self) {
        if let Some(selected) = self.profile_list_state.selected() {
            if selected > 0 {
                self.profile_list_state.select(Some(selected - 1));
            }
        }
    }

    pub fn move_profile_down(&mut self) {
        if let Some(selected) = self.profile_list_state.selected() {
            if selected < self.profile_names.len().saturating_sub(1) {
                self.profile_list_state.select(Some(selected + 1));
            }
        }
    }

    pub fn load_selected_profile(&mut self) {
        log_debug!("UI: load_selected_profile called");
        if let Some(selected) = self.profile_list_state.selected() {
            log_debug!("UI: Selected profile index: {}", selected);
            if selected < self.profile_names.len() {
                let profile_name = self.profile_names[selected].clone();
                log_debug!("UI: Loading profile: '{}'", profile_name);
                log_debug!("UI: Animations enabled: {}", self.animations_enabled);

                if self.animations_enabled {
                    log_debug!("UI: Starting profile switch animation");
                    self.start_profile_switch_animation();
                    // Delay the actual profile switch until animation shows
                    self.pending_profile_switch = Some(profile_name.clone());
                    log_debug!("UI: Set pending profile switch to: '{}'", profile_name);
                } else {
                    log_debug!("UI: No animation, switching immediately");
                    // No animation, switch immediately
                    self.switch_to_profile(&profile_name);
                }
            } else {
                log_debug!(
                    "UI: ERROR - Selected index {} out of bounds ({})",
                    selected,
                    self.profile_names.len()
                );
            }
        } else {
            log_debug!("UI: ERROR - No profile selected");
        }
    }

    pub fn delete_selected_profile(&mut self) {
        if let Some(selected) = self.profile_list_state.selected() {
            if selected < self.profile_names.len() {
                let profile_name = self.profile_names[selected].clone();
                if let Ok(mut state) = crate::state::AppState::load_from_file(&self.state_file) {
                    if state.delete_profile(&profile_name) {
                        let _ = state.save_to_file(&self.state_file);
                        self.load_profiles();
                        // Adjust selection
                        if self.profile_names.is_empty() {
                            self.profile_list_state.select(None);
                        } else if selected >= self.profile_names.len() {
                            self.profile_list_state
                                .select(Some(self.profile_names.len() - 1));
                        }
                    }
                }
            }
        }
    }

    pub fn handle_profile_input(&mut self) {
        if !self.profile_input_buffer.trim().is_empty()
            && self.profile_action == ProfileAction::Save
        {
            // Load existing state and add the profile
            if let Ok(mut state) = crate::state::AppState::load_from_file(&self.state_file) {
                let description = format!(
                    "Saved on {}",
                    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
                );

                // Get current patterns from UI state, not the loaded file
                let current_locked = self.get_current_locked_patterns();
                let current_unlocked = self.get_current_unlocked_patterns();

                // Create profile from current UI state
                let profile = crate::state::LockProfile {
                    locked_patterns: current_locked,
                    unlocked_patterns: current_unlocked,
                    allow_create_patterns: vec![], // TODO: implement if needed
                    description,
                };

                state
                    .profiles
                    .insert(self.profile_input_buffer.clone(), profile);
                state.active_profile = Some(self.profile_input_buffer.clone());
                self.active_profile_name = Some(self.profile_input_buffer.clone());

                if std::env::var("ICAROS_DEBUG").is_ok() {
                    eprintln!(
                        "Saving profile '{}' with {} profiles total",
                        self.profile_input_buffer,
                        state.profiles.len()
                    );
                }

                if let Err(e) = state.save_to_file(&self.state_file) {
                    eprintln!("Error saving profile: {e}");
                } else if std::env::var("ICAROS_DEBUG").is_ok() {
                    eprintln!("Profile saved successfully");
                }

                self.load_profiles();
            }
        }
        self.profile_input_mode = false;
        self.profile_input_buffer.clear();
        self.profile_action = ProfileAction::None;
    }

    pub fn switch_to_profile(&mut self, name: &str) {
        if let Ok(mut state) = crate::state::AppState::load_from_file(&self.state_file) {
            if state.switch_to_profile(name) {
                let _ = state.save_to_file(&self.state_file);
                self.active_profile_name = Some(name.to_string());

                // Apply the new patterns to the tree
                self.explicitly_locked_paths.clear();
                self.explicitly_unlocked_paths.clear();

                // Convert patterns back to paths
                for pattern in &state.locked_patterns {
                    if let Some(path) = pattern_to_path(&self.root_path, pattern) {
                        self.explicitly_locked_paths.push(path);
                    }
                }

                for pattern in &state.unlocked_patterns {
                    if let Some(path) = pattern_to_path(&self.root_path, pattern) {
                        self.explicitly_unlocked_paths.push(path);
                    }
                }

                // Reapply locks to the tree
                self.reapply_explicit_locks();
                self.update_items();
            }
        }
    }

    pub fn start_profile_switch_animation(&mut self) {
        log_debug!("UI: start_profile_switch_animation called");
        self.animation_engine.trigger("profile_switch");
        self.profile_switching = true;
        log_debug!("UI: Profile switching flag set to true");
    }

    pub fn update_profile_animation(&mut self) {
        // Check if we need to execute the delayed profile switch
        if self.profile_switching {
            log_debug!("UI: update_profile_animation - profile_switching=true");
            if let Some(ref profile_name) = self.pending_profile_switch.clone() {
                log_debug!("UI: Pending profile switch: '{}'", profile_name);
                if self.animation_engine.is_active() {
                    log_debug!("UI: Animation is active");
                    // Animation is active, wait a bit for it to show
                    if let Some(ref active) = self.animation_engine.active_animation {
                        let elapsed = active.start_time.elapsed().as_millis();
                        log_debug!("UI: Animation elapsed: {}ms", elapsed);
                        if elapsed > 200 {
                            log_debug!("UI: Animation delay complete, switching to profile");
                            // Now do the actual profile switch
                            self.switch_to_profile(profile_name);
                            self.pending_profile_switch = None;
                        }
                    }
                } else {
                    log_debug!("UI: Animation not active, switching immediately");
                    // Animation failed to start or already ended, switch immediately
                    self.switch_to_profile(profile_name);
                    self.pending_profile_switch = None;
                    self.profile_switching = false;
                }
            }

            // Clear the switching flag when animation ends
            if !self.animation_engine.is_active() && self.pending_profile_switch.is_none() {
                log_debug!("UI: Animation ended, clearing profile_switching flag");
                self.profile_switching = false;
            }
        }
    }

    fn get_current_locked_patterns(&self) -> Vec<String> {
        let mut patterns = Vec::new();

        // If no explicit locks, check if everything is locked via tree state
        if self.explicitly_locked_paths.is_empty() {
            // Check if the root is locked in the tree
            if self.tree.is_locked {
                patterns.push("**".to_string());
            }
        } else {
            for path in &self.explicitly_locked_paths {
                if let Ok(relative) = path.strip_prefix(&self.root_path) {
                    if relative.as_os_str().is_empty() {
                        patterns.push("**".to_string());
                    } else {
                        let pattern = if path.is_dir() {
                            format!("{}/**", relative.display())
                        } else {
                            relative.display().to_string()
                        };
                        patterns.push(pattern);
                    }
                }
            }
        }

        patterns.sort();
        patterns.dedup();
        patterns
    }

    fn get_current_unlocked_patterns(&self) -> Vec<String> {
        let mut patterns = Vec::new();
        for path in &self.explicitly_unlocked_paths {
            if let Ok(relative) = path.strip_prefix(&self.root_path) {
                let pattern = if path.is_dir() {
                    format!("{}/**", relative.display())
                } else {
                    relative.display().to_string()
                };
                patterns.push(pattern);
            }
        }
        patterns.sort();
        patterns.dedup();
        patterns
    }
}

fn toggle_expand_at_path(node: &mut TreeNode, target_path: &std::path::Path) -> bool {
    if node.path == target_path {
        node.toggle_expand();
        return true;
    }

    for child in &mut node.children {
        if toggle_expand_at_path(child, target_path) {
            return true;
        }
    }
    false
}

fn toggle_create_in_locked_at_path(node: &mut TreeNode, target_path: &std::path::Path) -> bool {
    if node.path == target_path {
        node.toggle_create_in_locked();
        return true;
    }

    for child in &mut node.children {
        if toggle_create_in_locked_at_path(child, target_path) {
            return true;
        }
    }
    false
}

fn unlock_all_recursive(node: &mut TreeNode) {
    node.is_locked = false;
    node.allow_create_in_locked = false;
    for child in &mut node.children {
        unlock_all_recursive(child);
    }
}

fn lock_path_and_children(node: &mut TreeNode, path: &std::path::Path) {
    if node.path == *path {
        node.is_locked = true;
        // Lock all children recursively
        lock_all_children_recursive(node);
        return;
    }

    // If this node is an ancestor of the target path, keep searching
    if path.starts_with(&node.path) {
        for child in &mut node.children {
            lock_path_and_children(child, path);
        }
    }
}

fn lock_all_children_recursive(node: &mut TreeNode) {
    for child in &mut node.children {
        child.is_locked = true;
        child.allow_create_in_locked = false;
        lock_all_children_recursive(child);
    }
}

fn unlock_path(node: &mut TreeNode, path: &std::path::Path) {
    if node.path == *path {
        node.is_locked = false;
        node.allow_create_in_locked = false;
        // Also unlock all children recursively
        unlock_all_recursive(node);
        return;
    }

    for child in &mut node.children {
        unlock_path(child, path);
    }
}

fn restore_expanded_state(node: &mut TreeNode, path: &std::path::Path) {
    if node.path == *path {
        node.is_expanded = true;
    }
    for child in &mut node.children {
        restore_expanded_state(child, path);
    }
}

fn optimize_patterns(patterns: Vec<String>) -> Vec<String> {
    if patterns.is_empty() {
        return patterns;
    }

    let mut optimized: Vec<String> = Vec::new();
    let mut sorted = patterns;
    sorted.sort();

    for pattern in sorted {
        let mut is_redundant = false;

        // Check if this pattern is covered by any existing pattern
        for existing in &optimized {
            if is_pattern_covered(&pattern, existing) {
                is_redundant = true;
                break;
            }
        }

        if !is_redundant {
            // Remove any patterns that this one covers
            optimized.retain(|existing| !is_pattern_covered(existing, &pattern));
            optimized.push(pattern);
        }
    }

    optimized
}

fn optimize_patterns_with_context(
    locked_patterns: Vec<String>,
    unlocked_patterns: &[String],
) -> Vec<String> {
    if locked_patterns.is_empty() {
        return locked_patterns;
    }

    let mut optimized: Vec<String> = Vec::new();
    let mut sorted = locked_patterns;
    sorted.sort();

    for pattern in sorted {
        let mut is_redundant = false;

        // Check if this pattern is covered by any existing pattern
        for existing in &optimized {
            if is_pattern_covered(&pattern, existing) {
                // Before marking as redundant, check if this pattern is needed
                // to override an unlocked pattern
                let mut needed_for_override = false;
                for unlocked in unlocked_patterns {
                    if is_pattern_covered(&pattern, unlocked) {
                        // This pattern is within an unlocked area, so we need it
                        needed_for_override = true;
                        break;
                    }
                }

                if !needed_for_override {
                    is_redundant = true;
                }
                break;
            }
        }

        if !is_redundant {
            // Remove any patterns that this one covers, unless they're needed for overrides
            optimized.retain(|existing| {
                if is_pattern_covered(existing, &pattern) {
                    // Check if the existing pattern is needed to override an unlock
                    for unlocked in unlocked_patterns {
                        if is_pattern_covered(existing, unlocked) {
                            return true; // Keep it
                        }
                    }
                    return false; // Remove it
                }
                true // Keep patterns not covered by the new one
            });
            optimized.push(pattern);
        }
    }

    optimized
}

fn is_pattern_covered(specific: &str, general: &str) -> bool {
    // Check if 'specific' is covered by 'general'
    if general == "**" {
        return true;
    }

    if let Some(general_prefix) = general.strip_suffix("/**") {
        // Check if specific is under this directory
        if let Some(remainder) = specific.strip_prefix(general_prefix) {
            // It's covered if it's the exact directory or a child
            return remainder.is_empty() || remainder.starts_with('/');
        }
    }

    false
}

fn pattern_to_path(root: &std::path::Path, pattern: &str) -> Option<std::path::PathBuf> {
    if pattern == "**" {
        return Some(root.to_path_buf());
    }

    if let Some(dir_pattern) = pattern.strip_suffix("/**") {
        return Some(root.join(dir_pattern));
    }

    Some(root.join(pattern))
}

fn render_file_guardian(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .items
        .iter()
        .map(|(node, indent)| {
            let mut spans = vec![Span::raw("  ".repeat(*indent))];

            if node.is_dir {
                spans.push(Span::raw(if node.is_expanded { "▼ " } else { "▶ " }));
            } else {
                spans.push(Span::raw("  "));
            }

            if node.is_locked {
                spans.push(Span::styled(
                    "🔒 ",
                    Style::default().fg(Color::Rgb(255, 107, 53)),
                ));
                if node.is_dir && node.allow_create_in_locked {
                    spans.push(Span::styled(
                        "➕ ",
                        Style::default().fg(Color::Rgb(0, 206, 209)),
                    ));
                } else {
                    spans.push(Span::raw("   "));
                }
            } else {
                spans.push(Span::raw("   "));
                spans.push(Span::raw("   "));
            }

            let style = if node.is_locked {
                Style::default()
                    .fg(Color::Rgb(255, 127, 80)) // Coral
                    .add_modifier(Modifier::BOLD)
            } else if node.is_dir {
                Style::default()
                    .fg(Color::Rgb(0, 206, 209)) // Static cyan for directories
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(255, 215, 0)) // Gold
            };

            spans.push(Span::styled(&node.name, style));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Rgb(138, 43, 226))) // Static violet
                .title(" 🦙 File Guardian 🦙 ")
                .style(Style::default().bg(Color::Rgb(0, 0, 0))),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(138, 43, 226))
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_git_stage(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    // Split the area into two panes
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage(40), // File list
                Constraint::Percentage(60), // Diff view
            ]
            .as_ref(),
        )
        .split(area);

    // Render file list
    let file_items: Vec<ListItem> = app
        .git_files
        .iter()
        .map(|file| {
            let status_color = file.status.color();
            let status_str = file.status.to_str();
            let staged_indicator = if file.staged { "●" } else { "○" };

            let spans = vec![
                Span::styled(
                    staged_indicator,
                    Style::default().fg(if file.staged {
                        Color::Green
                    } else {
                        Color::Gray
                    }),
                ),
                Span::raw(" "),
                Span::styled(status_str, Style::default().fg(status_color)),
                Span::raw(" "),
                Span::styled(
                    file.path.display().to_string(),
                    Style::default().fg(Color::White),
                ),
            ];

            ListItem::new(Line::from(spans))
        })
        .collect();

    let file_list = List::new(file_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if app.git_pane == GitPane::FileList {
                    Color::Yellow
                } else {
                    Color::Gray
                }))
                .title(" Changed Files ")
                .style(Style::default().bg(Color::Rgb(0, 0, 0))),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(50, 50, 50))
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(file_list, chunks[0], &mut app.git_file_list_state);

    // Render diff view
    let mut diff_lines = Vec::new();
    let mut _current_line = 0;

    for (hunk_idx, hunk) in app.git_diff_hunks.iter().enumerate() {
        // Add hunk header
        let hunk_style = if hunk_idx == app.git_selected_hunk {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Blue)
        };
        diff_lines.push(Line::from(Span::styled(&hunk.header, hunk_style)));
        _current_line += 1;

        // Add hunk lines
        for line in &hunk.lines {
            let (style, prefix) = match line.origin {
                '+' => (Style::default().fg(Color::Green), "+"),
                '-' => (Style::default().fg(Color::Red), "-"),
                _ => (Style::default().fg(Color::Gray), " "),
            };

            let content = format!("{}{}", prefix, line.content);
            diff_lines.push(Line::from(Span::styled(content, style)));
            _current_line += 1;
        }

        // Add empty line between hunks
        if hunk_idx < app.git_diff_hunks.len() - 1 {
            diff_lines.push(Line::from(""));
            _current_line += 1;
        }
    }

    let diff_widget = Paragraph::new(diff_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if app.git_pane == GitPane::DiffView {
                    Color::Yellow
                } else {
                    Color::Gray
                }))
                .title(" Diff ")
                .style(Style::default().bg(Color::Rgb(0, 0, 0))),
        )
        .scroll((app.git_diff_scroll, 0));

    f.render_widget(diff_widget, chunks[1]);
}

fn render_profiles(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    // Check if profile switching animation is active
    if app.profile_switching {
        log_debug!("RENDER: Profile switching active, checking for animation frame");
        if let Some(frame_content) = app.animation_engine.get_current_frame() {
            log_debug!(
                "RENDER: Got frame content (first 50 chars): {}",
                &frame_content.chars().take(50).collect::<String>()
            );

            // Check if this is an image marker
            if frame_content.starts_with("IMAGE:") {
                let image_path = frame_content.trim_start_matches("IMAGE:");
                log_debug!("RENDER: Detected image frame, path: {}", image_path);
                app.current_image_path = Some(image_path.to_string());

                // Create layout to get the proper inner pane area
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(
                        [
                            Constraint::Min(5),    // Profile list area
                            Constraint::Length(3), // Input area (when active)
                        ]
                        .as_ref(),
                    )
                    .split(area);

                // Use the FULL profile list area for maximum size
                let image_area = chunks[0];

                // Render the image at maximum size
                render_image_frame(f, image_area, image_path);

                // Always check for overlay text during profile switching, regardless of frame content
                if let Some(overlay_text) = app.animation_engine.get_overlay_frame() {
                    render_text_overlay(f, image_area, &overlay_text);
                }
                return;
            } else {
                app.current_image_path = None;
                render_animation_frame(f, area, &frame_content);
                return;
            }
        } else {
            log_debug!("RENDER: No frame content available");
            app.current_image_path = None;
        }

        // Even if no frame content, check for overlay text during profile switching
        if let Some(overlay_text) = app.animation_engine.get_overlay_frame() {
            // Create layout to get the proper inner pane area
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Min(5),    // Profile list area
                        Constraint::Length(3), // Input area (when active)
                    ]
                    .as_ref(),
                )
                .split(area);

            render_text_overlay(f, chunks[0], &overlay_text);
        }
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Min(5),    // Profile list
                Constraint::Length(3), // Input area (when active)
            ]
            .as_ref(),
        )
        .split(area);

    // Profile list
    let profile_items: Vec<ListItem> = app
        .profile_names
        .iter()
        .map(|name| {
            let mut spans = vec![Span::raw("  ")];

            // Active profile indicator
            if Some(name) == app.active_profile_name.as_ref() {
                spans.push(Span::styled("● ", Style::default().fg(Color::Green)));
            } else {
                spans.push(Span::raw("  "));
            }

            spans.push(Span::styled(name, Style::default().fg(Color::Cyan)));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let active_profile_text = app
        .active_profile_name
        .as_ref()
        .map(|name| format!(" Active: {name} "))
        .unwrap_or_else(|| " No Active Profile ".to_string());

    let list = List::new(profile_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta))
                .title(format!(" 🏜️ Lock Profiles 🏜️{active_profile_text}"))
                .style(Style::default().bg(Color::Rgb(0, 0, 0))),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(list, chunks[0], &mut app.profile_list_state);

    // Input area (when in input mode)
    if app.profile_input_mode {
        let input_text = match app.profile_action {
            ProfileAction::Save => format!("Save current profile as: {}", app.profile_input_buffer),
            _ => app.profile_input_buffer.clone(),
        };

        let input_paragraph = Paragraph::new(input_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .title(" Enter Profile Name (Enter to save, Esc to cancel) "),
            )
            .style(Style::default().fg(Color::White));

        f.render_widget(input_paragraph, chunks[1]);
    } else {
        // Help text
        let help_text = vec![Line::from(vec![Span::raw(
            "Enter: Load | s: Save current | d: Delete | r: Refresh | Up/Down: Navigate",
        )])];

        let help_paragraph = Paragraph::new(help_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Gray))
                    .title(" Commands "),
            )
            .style(Style::default().fg(Color::Gray));

        f.render_widget(help_paragraph, chunks[1]);
    }
}

fn render_animation_frame(f: &mut ratatui::Frame, area: Rect, content: &str) {
    // Clear the entire area first
    let clear_widget = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(clear_widget, area);

    // For regular ASCII art
    let cleaned = content
        .trim_start_matches("\x1b[?25l") // Remove cursor hide
        .trim_end_matches("\x1b[?25h") // Remove cursor show
        .trim_end_matches("\x1b[0m"); // Remove reset

    // Split into lines and render with basic color
    let lines: Vec<Line> = cleaned
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            // For ANSI art, use green theme
            if line.contains("\x1b[") {
                // This has ANSI codes - for now just strip them and show in green
                let stripped = strip_ansi_escapes::strip(line);
                let text = String::from_utf8_lossy(&stripped);
                Line::from(Span::styled(
                    text.to_string(),
                    Style::default().fg(Color::Green),
                ))
            } else {
                // Plain text - show in cyan
                Line::from(Span::styled(line, Style::default().fg(Color::Cyan)))
            }
        })
        .collect();

    let animation_widget = Paragraph::new(lines)
        .style(Style::default().bg(Color::Black))
        .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(animation_widget, area);
}

fn render_image_frame(f: &mut ratatui::Frame, area: Rect, image_path: &str) {
    use ratatui_image::{picker::Picker, Image};

    // Clear the entire area first
    let clear_widget = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(clear_widget, area);

    // Use the full area for maximum size
    let image_area = area;

    // Try to load and display the image (embedded first, then filesystem)
    let image_result =
        if let Some(embedded_bytes) = crate::animations::get_embedded_image(image_path) {
            log_debug!("Using embedded image for: {}", image_path);
            image::ImageReader::new(std::io::Cursor::new(embedded_bytes))
                .with_guessed_format()
                .map_err(|e| format!("Failed to create reader: {e}"))
                .and_then(|reader| {
                    reader
                        .decode()
                        .map_err(|e| format!("Failed to decode: {e}"))
                })
        } else {
            log_debug!("Using filesystem image for: {}", image_path);
            image::ImageReader::open(image_path)
                .map_err(|e| format!("Failed to open: {e}"))
                .and_then(|reader| {
                    reader
                        .decode()
                        .map_err(|e| format!("Failed to decode: {e}"))
                })
        };

    match image_result {
        Ok(dyn_img) => {
            // Force using a very small block size for maximum resolution
            // This gives us 2x2 pixels per character with unicode blocks
            let mut picker = Picker::new((1, 2));

            // Calculate dimensions to make image width = 100% of panel width
            let img_width = dyn_img.width() as f64;
            let img_height = dyn_img.height() as f64;
            let panel_width = image_area.width as f64;

            // Scale based on width to fill 100% of panel width
            let width_scale = panel_width / img_width;
            let new_width = panel_width as u32;
            let new_height = (img_height * width_scale) as u32;

            // Resize the image to exactly match panel width
            let resized_img =
                dyn_img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3);

            // Use the resized image with Fit mode
            match picker.new_protocol(resized_img, image_area, ratatui_image::Resize::Fit(None)) {
                Ok(protocol) => {
                    // Create and render the image widget
                    let image = Image::new(&*protocol);
                    f.render_widget(image, image_area);
                }
                Err(e) => {
                    log_debug!("Failed to create protocol: {}", e);
                    show_image_error(f, area, &format!("Failed to create protocol: {e}"));
                }
            }
        }
        Err(e) => {
            log_debug!("Failed to load image {}: {}", image_path, e);
            show_image_error(f, area, &e);
        }
    }
}

fn show_image_error(f: &mut ratatui::Frame, area: Rect, error: &str) {
    let error_msg = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Image Error",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(error, Style::default().fg(Color::Gray))),
    ];
    let error_widget = Paragraph::new(error_msg)
        .style(Style::default().bg(Color::Black))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(error_widget, area);
}

fn render_text_overlay(f: &mut ratatui::Frame, area: Rect, text: &str) {
    // Use a larger fixed area for ASCII art display
    let text_width = 50u16;
    let text_height = 8u16;

    // Create a centered area for the text
    let text_area = Rect {
        x: area.x + (area.width.saturating_sub(text_width)) / 2,
        y: area.y + (area.height.saturating_sub(text_height)) / 2,
        width: text_width.min(area.width),
        height: text_height.min(area.height),
    };

    // Create lines from the text with bright yellow color
    let lines: Vec<Line> = text
        .lines()
        .map(|line| {
            Line::from(Span::styled(
                line,
                Style::default()
                    .fg(Color::Rgb(255, 255, 0))
                    .add_modifier(Modifier::BOLD),
            ))
        })
        .collect();

    let text_widget = Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center);

    f.render_widget(text_widget, text_area);
}

pub fn run_ui(mut app: App) -> Result<App> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Get ignore patterns for the file watcher
    let watcher_ignore_patterns =
        if let Ok(state) = crate::state::AppState::load_from_file(&app.state_file) {
            state.ignore_patterns
        } else {
            crate::state::default_ignore_patterns()
        };

    // Set up file watcher
    let (tx, rx) = channel();
    let mut watcher =
        notify::recommended_watcher(move |res: Result<NotifyEvent, notify::Error>| {
            if let Ok(event) = res {
                // Filter out events from ignored directories
                let should_process = event.paths.iter().any(|path| {
                    let path_str = path.to_string_lossy();

                    // Check against ignore patterns
                    let ignored = watcher_ignore_patterns.iter().any(|pattern| {
                        if pattern.ends_with('/') {
                            path_str.contains(&format!("/{pattern}")) || path_str.contains(pattern)
                        } else {
                            path_str.contains(pattern)
                        }
                    });

                    !ignored
                });

                if should_process {
                    let _ = tx.send(());
                }
            }
        })?;

    // Watch the root path
    watcher.watch(&app.root_path, RecursiveMode::Recursive)?;

    let result = run_app(&mut terminal, &mut app, rx);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result?;
    Ok(app)
}

fn get_gradient_color(position: f32, offset: f32, colors: &[Color]) -> Color {
    let wave = ((position + offset).sin() + 1.0) / 2.0;
    let index = (wave * (colors.len() - 1) as f32) as usize;
    let next_index = (index + 1).min(colors.len() - 1);
    let t = wave * (colors.len() - 1) as f32 - index as f32;

    interpolate_color(colors[index], colors[next_index], t)
}

fn interpolate_color(c1: Color, c2: Color, t: f32) -> Color {
    match (c1, c2) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => Color::Rgb(
            (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8,
            (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8,
            (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8,
        ),
        _ => c1,
    }
}

fn get_sky_color(day_night: f32) -> Color {
    let index = (day_night * (SUNSET_GRADIENT.len() - 1) as f32) as usize;
    let next_index = (index + 1).min(SUNSET_GRADIENT.len() - 1);
    let t = day_night * (SUNSET_GRADIENT.len() - 1) as f32 - index as f32;

    interpolate_color(SUNSET_GRADIENT[index], SUNSET_GRADIENT[next_index], t)
}

fn get_time_emoji(day_night: f32) -> (&'static str, &'static str) {
    let index = (day_night * TIME_ELEMENTS.len() as f32) as usize;
    let index = index.min(TIME_ELEMENTS.len() - 1);
    TIME_ELEMENTS[index]
}

fn get_native_pattern(frame: u64, offset: usize) -> &'static str {
    NATIVE_PATTERNS[(frame / 15 + offset as u64) as usize % NATIVE_PATTERNS.len()]
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    fs_events: Receiver<()>,
) -> Result<()> {
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(50);
    let debounce_duration = Duration::from_millis(1500); // Increased to handle rapid .venv changes

    loop {
        terminal.draw(|f| {
            // Update animations
            if app.animations_enabled {
                app.frame_count += 1;
                app.update_animations(f.area().width);
                // Update animation engine to clear expired animations
                app.animation_engine.update();
            }

            // Update profile animation if active
            if app.profile_switching {
                app.update_profile_animation();
            }

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Length(3), // Top title bar
                        Constraint::Min(0),    // Main content area
                    ]
                    .as_ref(),
                )
                .split(f.area());

            // Create animated title with gradient background
            let mut title_spans = Vec::new();
            let width = chunks[0].width as usize;

            if app.animations_enabled {
                // Create gradient background
                let sky_color = get_sky_color(app.day_night_cycle);
                let (time_emoji, time_name) = get_time_emoji(app.day_night_cycle);

                // Build the title line with gradient effect
                for i in 0..width {
                    let x = i as f32 / width as f32;
                    let gradient_color =
                        get_gradient_color(x * 10.0, app.wave_offset, EARTH_COLORS);

                    // Place llama
                    if i as f32 >= app.llama_x && (i as f32) < app.llama_x + 2.0 {
                        title_spans.push(Span::styled(
                            "🦙",
                            Style::default().fg(Color::White).bg(sky_color),
                        ));
                    }
                    // Place desert elements
                    else if i % 15 == 0 && i > 0 && i < width - 5 {
                        let desert_elem = DESERT_ELEMENTS[(i / 15) % DESERT_ELEMENTS.len()];
                        title_spans.push(Span::styled(
                            desert_elem,
                            Style::default().fg(gradient_color).bg(sky_color),
                        ));
                    }
                    // Place time emoji
                    else if i == width - 10 {
                        title_spans.push(Span::styled(
                            time_emoji,
                            Style::default().fg(Color::White).bg(sky_color),
                        ));
                    }
                    // Native patterns
                    else if i % 8 == 0 {
                        let pattern = get_native_pattern(app.frame_count, i);
                        title_spans.push(Span::styled(
                            pattern,
                            Style::default().fg(gradient_color).bg(sky_color),
                        ));
                    } else {
                        title_spans.push(Span::styled(" ", Style::default().bg(sky_color)));
                    }
                }

                // Title text overlay
                let title_text = "◈ I C A R O S ◈";
                let title_start = (width / 2).saturating_sub(title_text.len() / 2);
                for (i, ch) in title_text.chars().enumerate() {
                    if title_start + i < title_spans.len() {
                        let pulse =
                            ((app.frame_count as f32 * 0.05 + i as f32 * 0.3).sin() + 1.0) / 2.0;
                        let text_color = interpolate_color(
                            Color::Rgb(255, 255, 255),
                            Color::Rgb(255, 215, 0),
                            pulse,
                        );
                        title_spans[title_start + i] = Span::styled(
                            ch.to_string(),
                            Style::default()
                                .fg(text_color)
                                .bg(sky_color)
                                .add_modifier(Modifier::BOLD),
                        );
                    }
                }

                // Add subtitle
                let subtitle = format!(" {time_name} Journey ");
                let _subtitle_start = (width / 2).saturating_sub(subtitle.len() / 2);
                let _subtitle_y = 2; // This would need to be on a second line
            } else {
                // Static title
                let static_line = format!(
                    "{:^width$}",
                    "◈ I C A R O S ◈ - Stop AI Agents from Going on Vision Quests",
                    width = width
                );
                title_spans.push(Span::styled(
                    static_line,
                    Style::default().fg(Color::Rgb(255, 215, 0)),
                ));
            }

            let title = vec![Line::from(title_spans)];
            let title_widget = Paragraph::new(title)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(if app.animations_enabled {
                            get_gradient_color(0.0, app.wave_offset * 2.0, EARTH_COLORS)
                        } else {
                            Color::Rgb(210, 105, 30) // Static chocolate
                        }))
                        .style(Style::default()),
                )
                .alignment(ratatui::layout::Alignment::Center);
            f.render_widget(title_widget, chunks[0]);

            // No more floating emojis in the main area

            // Check if help overlay should be shown
            if app.show_help {
                render_help_overlay(f, app, chunks[1]);
            } else {
                // Split main area for tabs
                let main_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(
                        [
                            Constraint::Length(1), // Compact tab bar
                            Constraint::Min(0),    // Content
                        ]
                        .as_ref(),
                    )
                    .split(chunks[1]);

                // Render compact tabs
                render_compact_tabs(f, app, main_chunks[0]);

                // Render content based on active tab
                match app.active_tab {
                    TabIndex::FileGuardian => {
                        render_file_guardian(f, app, main_chunks[1]);
                    }
                    TabIndex::GitStage => {
                        render_git_stage(f, app, main_chunks[1]);
                    }
                    TabIndex::Profiles => {
                        render_profiles(f, app, main_chunks[1]);
                    }
                }

                // Render simple animations on top (but not profile switch - that's handled in render_profiles)
                if app.animations_enabled
                    && app.animation_engine.is_active()
                    && !app.profile_switching
                {
                    if let Some(frame_content) = app.animation_engine.get_current_frame() {
                        // Create a larger overlay that covers most of the content area
                        let overlay_height = std::cmp::min(20, chunks[1].height);
                        let overlay_width = std::cmp::min(50, chunks[1].width);
                        let overlay_area = Rect {
                            x: chunks[1].x + chunks[1].width / 2 - overlay_width / 2,
                            y: chunks[1].y + chunks[1].height / 2 - overlay_height / 2,
                            width: overlay_width,
                            height: overlay_height,
                        };
                        render_animation_frame(f, overlay_area, &frame_content);
                    }
                }
            }
        })?;

        // Check for file system events (non-blocking)
        if fs_events.try_recv().is_ok() {
            // Set flag to refresh, but debounce to avoid too many updates
            if app.last_refresh.elapsed() > debounce_duration {
                app.needs_refresh = true;
            }
        }

        // Refresh tree if needed
        if app.needs_refresh && app.last_refresh.elapsed() > debounce_duration {
            match app.refresh_tree() {
                Ok(_) => {
                    // Refresh successful
                }
                Err(e) => {
                    // More graceful error handling - just log and reset flag
                    eprintln!("Warning: Tree refresh encountered issues: {e}");
                    app.needs_refresh = false;
                    app.last_refresh = Instant::now();
                }
            }
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // Global keys
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('?') => app.show_help = !app.show_help,
                    KeyCode::Tab => {
                        app.active_tab = match app.active_tab {
                            TabIndex::FileGuardian => {
                                // Initialize Git view when switching to it
                                app.refresh_git_status();
                                if !app.git_files.is_empty()
                                    && app.git_selected_file < app.git_files.len()
                                {
                                    app.load_git_diff();
                                }
                                TabIndex::GitStage
                            }
                            TabIndex::GitStage => {
                                app.load_profiles();
                                TabIndex::Profiles
                            }
                            TabIndex::Profiles => TabIndex::FileGuardian,
                        };
                    }
                    KeyCode::BackTab => {
                        app.active_tab = match app.active_tab {
                            TabIndex::FileGuardian => {
                                app.load_profiles();
                                TabIndex::Profiles
                            }
                            TabIndex::GitStage => TabIndex::FileGuardian,
                            TabIndex::Profiles => {
                                // Initialize Git view when switching to it
                                app.refresh_git_status();
                                if !app.git_files.is_empty()
                                    && app.git_selected_file < app.git_files.len()
                                {
                                    app.load_git_diff();
                                }
                                TabIndex::GitStage
                            }
                        };
                    }
                    _ => {
                        // Tab-specific keys
                        match app.active_tab {
                            TabIndex::FileGuardian => match key.code {
                                KeyCode::Up => app.move_up(),
                                KeyCode::Down => app.move_down(),
                                KeyCode::Char(' ') => app.toggle_selected(),
                                KeyCode::Enter => app.toggle_expand_selected(),
                                KeyCode::Char('c') => app.toggle_create_in_locked_selected(),
                                KeyCode::Char('a') => {
                                    app.animations_enabled = !app.animations_enabled
                                }
                                KeyCode::Char('r') => app.needs_refresh = true,
                                KeyCode::Char('h') => {
                                    app.show_hidden = !app.show_hidden;
                                    app.update_items();
                                }
                                _ => {}
                            },
                            TabIndex::GitStage => {
                                match key.code {
                                    KeyCode::Left => app.git_pane = GitPane::FileList,
                                    KeyCode::Right => {
                                        if !app.git_files.is_empty() {
                                            app.git_pane = GitPane::DiffView;
                                        }
                                    }
                                    KeyCode::Up => match app.git_pane {
                                        GitPane::FileList => app.move_git_file_up(),
                                        GitPane::DiffView => app.scroll_git_diff_up(),
                                    },
                                    KeyCode::Down => match app.git_pane {
                                        GitPane::FileList => app.move_git_file_down(),
                                        GitPane::DiffView => app.scroll_git_diff_down(),
                                    },
                                    KeyCode::Char(' ') => {
                                        if app.git_pane == GitPane::FileList {
                                            app.toggle_git_file_stage();
                                        }
                                    }
                                    KeyCode::Char('n') => {
                                        if app.git_pane == GitPane::DiffView {
                                            app.move_git_hunk_down();
                                        }
                                    }
                                    KeyCode::Char('p') => {
                                        if app.git_pane == GitPane::DiffView {
                                            app.move_git_hunk_up();
                                        }
                                    }
                                    KeyCode::Char('s') => {
                                        // Stage current hunk (not implemented yet)
                                        if app.git_pane == GitPane::DiffView
                                            && !app.git_diff_hunks.is_empty()
                                        {
                                            // TODO: Implement hunk staging
                                        }
                                    }
                                    KeyCode::Char('u') => {
                                        // Unstage current hunk (not implemented yet)
                                        if app.git_pane == GitPane::DiffView
                                            && !app.git_diff_hunks.is_empty()
                                        {
                                            // TODO: Implement hunk unstaging
                                        }
                                    }
                                    KeyCode::Char('r') => {
                                        app.refresh_git_status();
                                        if !app.git_files.is_empty() {
                                            app.load_git_diff();
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            TabIndex::Profiles => {
                                if app.profile_input_mode {
                                    match key.code {
                                        KeyCode::Enter => {
                                            app.handle_profile_input();
                                        }
                                        KeyCode::Esc => {
                                            app.profile_input_mode = false;
                                            app.profile_input_buffer.clear();
                                        }
                                        KeyCode::Backspace => {
                                            app.profile_input_buffer.pop();
                                        }
                                        KeyCode::Char(c) => {
                                            app.profile_input_buffer.push(c);
                                        }
                                        _ => {}
                                    }
                                } else {
                                    match key.code {
                                        KeyCode::Up => app.move_profile_up(),
                                        KeyCode::Down => app.move_profile_down(),
                                        KeyCode::Enter => {
                                            log_debug!("UI: Enter key pressed in profiles tab");
                                            app.load_selected_profile();
                                        }
                                        KeyCode::Char('s') => {
                                            app.profile_action = ProfileAction::Save;
                                            app.profile_input_mode = true;
                                            app.profile_input_buffer.clear();
                                        }
                                        KeyCode::Char('d') => {
                                            app.delete_selected_profile();
                                        }
                                        KeyCode::Char('r') => {
                                            app.load_profiles();
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
    Ok(())
}

fn render_compact_tabs(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let tab_titles = match app.active_tab {
        TabIndex::FileGuardian => " 🦙 File Guardian | Git Stage | Profiles ",
        TabIndex::GitStage => " File Guardian | 🔧 Git Stage | Profiles ",
        TabIndex::Profiles => " File Guardian | Git Stage | 🏜️ Profiles ",
    };

    let tab_line = Line::from(vec![
        Span::styled(
            "◈ I C A R O S ◈",
            Style::default()
                .fg(Color::Rgb(255, 215, 0))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" - "),
        Span::styled(
            tab_titles,
            Style::default()
                .fg(Color::Rgb(0, 206, 209))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" [? for help]", Style::default().fg(Color::Gray)),
    ]);

    let tab_widget = Paragraph::new(vec![tab_line]).style(Style::default().bg(Color::Rgb(0, 0, 0)));

    f.render_widget(tab_widget, area);
}

fn render_help_overlay(f: &mut ratatui::Frame, app: &App, area: Rect) {
    // Create a centered popup
    let popup_area = centered_rect(80, 80, area);

    let help_content = match app.active_tab {
        TabIndex::FileGuardian => vec![
            Line::from(Span::styled(
                "🦙 File Guardian Help",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("  ↑↓        Navigate files"),
            Line::from("  Tab       Switch to Git Stage"),
            Line::from("  Enter     Expand/collapse directories"),
            Line::from(""),
            Line::from("Actions:"),
            Line::from("  Space     Lock/unlock file or directory"),
            Line::from("  c         Toggle 'allow create' in locked dirs"),
            Line::from("  h         Show/hide hidden files"),
            Line::from("  r         Refresh file tree"),
            Line::from("  a         Toggle animations"),
            Line::from(""),
            Line::from("Visual Indicators:"),
            Line::from("  🔒        Locked file/directory"),
            Line::from("  🔒 ➕      Locked dir with create allowed"),
            Line::from("  ▶▼        Collapsed/expanded directory"),
            Line::from(""),
            Line::from("Global:"),
            Line::from("  ?         Toggle this help"),
            Line::from("  q         Quit"),
        ],
        TabIndex::GitStage => vec![
            Line::from(Span::styled(
                "🔧 Git Stage Help",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("  ←→        Switch between file list and diff"),
            Line::from("  ↑↓        Navigate files or scroll diff"),
            Line::from("  Tab       Switch to Profiles"),
            Line::from(""),
            Line::from("File List Actions:"),
            Line::from("  Space     Stage/unstage file"),
            Line::from("  r         Refresh Git status"),
            Line::from(""),
            Line::from("Diff View Actions:"),
            Line::from("  n/p       Next/previous hunk"),
            Line::from("  s         Stage hunk (TODO)"),
            Line::from("  u         Unstage hunk (TODO)"),
            Line::from(""),
            Line::from("File Status Indicators:"),
            Line::from("  M         Modified file"),
            Line::from("  A         Added (new) file"),
            Line::from("  D         Deleted file"),
            Line::from("  R         Renamed file"),
            Line::from("  ??        Untracked file"),
            Line::from("  ●○        Staged/unstaged indicator"),
            Line::from(""),
            Line::from("Global:"),
            Line::from("  ?         Toggle this help"),
            Line::from("  q         Quit"),
        ],
        TabIndex::Profiles => vec![
            Line::from(Span::styled(
                "🏜️ Profile Management Help",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("  ↑↓        Navigate profiles"),
            Line::from("  Tab       Switch to File Guardian"),
            Line::from(""),
            Line::from("Profile Actions:"),
            Line::from("  Enter     Load selected profile"),
            Line::from("  s         Save current patterns as new profile"),
            Line::from("  d         Delete selected profile"),
            Line::from("  r         Refresh profile list"),
            Line::from(""),
            Line::from("Profile Input Mode:"),
            Line::from("  Enter     Confirm profile name"),
            Line::from("  Esc       Cancel operation"),
            Line::from("  Text      Type profile name"),
            Line::from(""),
            Line::from("Visual Indicators:"),
            Line::from("  ● Green   Active profile"),
            Line::from(""),
            Line::from("Global:"),
            Line::from("  ?         Toggle this help"),
            Line::from("  q         Quit"),
        ],
    };

    let help_widget = Paragraph::new(help_content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" Help - Press ? to close ")
                .style(Style::default().bg(Color::Rgb(0, 0, 0))),
        )
        .wrap(ratatui::widgets::Wrap { trim: true });

    // Clear the background
    f.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0))),
        popup_area,
    );

    f.render_widget(help_widget, popup_area);
}

// Helper function to create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
