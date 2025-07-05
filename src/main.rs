mod file_tree;
mod state;
mod ui;
mod git;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::fs;
use anyhow::Result;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
    
    #[arg(default_value = ".")]
    path: PathBuf,
    
    #[arg(short, long, help = "Path to state file")]
    state_file: Option<PathBuf>,
    
    #[arg(short, long, help = "Additional ignore patterns")]
    ignore: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Initialize CLAUDE.md and ICAROS.md files")]
    Init,
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    let root_path = args.path.canonicalize()?;
    
    match args.command {
        Some(Commands::Init) => {
            init_command(&root_path)?;
            Ok(())
        }
        None => {
            let state_file = args.state_file.unwrap_or_else(|| {
                root_path.join(".icaros")
            });
            
            let tree = file_tree::build_tree(&root_path, &args.ignore, false)?;
            
            let mut app = ui::App::new(tree, state_file.clone(), root_path.clone());
            
            if state_file.exists() {
                if let Ok(state) = state::AppState::load_from_file(&state_file) {
                    restore_state(&mut app, &state);
                }
            }
            
            let final_app = ui::run_ui(app)?;
            
            println!("\nState file: {}", state_file.display());
            println!("Locked files: {}", final_app.get_locked_files().len());
            
            Ok(())
        }
    }
}

fn restore_state(app: &mut ui::App, state: &state::AppState) {
    eprintln!("Restoring state...");
    eprintln!("  locked_patterns: {:?}", state.locked_patterns);
    eprintln!("  unlocked_patterns: {:?}", state.unlocked_patterns);
    
    // First restore expanded dirs
    for expanded_dir in &state.expanded_dirs {
        restore_expanded(&mut app.tree, expanded_dir);
    }
    
    // Apply locked patterns and track explicitly locked paths
    app.explicitly_locked_paths.clear();
    for pattern in &state.locked_patterns {
        if pattern == "**" {
            eprintln!("  Locking entire tree with pattern: {}", pattern);
            app.explicitly_locked_paths.push(state.root_path.clone());
        } else if let Some(path) = pattern_to_path(&state.root_path, pattern) {
            eprintln!("  Locking path: {:?} from pattern: {}", path, pattern);
            app.explicitly_locked_paths.push(path);
        }
    }
    
    // Then apply unlocked patterns (exceptions to locked patterns)
    app.explicitly_unlocked_paths.clear();
    for pattern in &state.unlocked_patterns {
        if let Some(path) = pattern_to_path(&state.root_path, pattern) {
            eprintln!("  Explicit unlock path: {:?} from pattern: {}", path, pattern);
            app.explicitly_unlocked_paths.push(path);
        }
    }
    
    // Finally restore allow_create patterns
    for pattern in &state.allow_create_patterns {
        if let Some(path) = pattern_to_path(&state.root_path, pattern) {
            restore_allow_create(&mut app.tree, &path);
        }
    }
    
    // Clean up any conflicts or duplicates
    app.cleanup_lock_lists();
    
    // Apply the explicit locks to the tree
    app.reapply_explicit_locks();
    
    app.update_items();
    eprintln!("State restoration complete.");
}

fn pattern_to_path(root: &std::path::Path, pattern: &str) -> Option<std::path::PathBuf> {
    if pattern == "**" {
        // Special case: root directory is locked
        Some(root.to_path_buf())
    } else if pattern.ends_with("/**") {
        // Directory pattern
        let clean_pattern = pattern.trim_end_matches("/**");
        if clean_pattern.is_empty() {
            Some(root.to_path_buf())
        } else {
            Some(root.join(clean_pattern))
        }
    } else {
        // File or specific path pattern
        Some(root.join(pattern))
    }
}

fn restore_expanded(node: &mut file_tree::TreeNode, path: &Path) -> bool {
    if node.path == path && node.is_dir {
        node.is_expanded = true;
        return true;
    }
    
    for child in &mut node.children {
        if restore_expanded(child, path) {
            return true;
        }
    }
    false
}

fn lock_all_recursive(node: &mut file_tree::TreeNode) {
    node.is_locked = true;
    node.allow_create_in_locked = false;
    for child in &mut node.children {
        lock_all_recursive(child);
    }
}

fn restore_locked(node: &mut file_tree::TreeNode, path: &Path) -> bool {
    if node.path == path || node.path.starts_with(path) {
        node.is_locked = true;
        // Don't reset allow_create_in_locked here - it will be set by restore_allow_create
        // Also don't call lock_all_children as it resets allow_create_in_locked flags
        return true;
    }
    
    for child in &mut node.children {
        restore_locked(child, path);
    }
    false
}

fn restore_unlocked(node: &mut file_tree::TreeNode, path: &Path) -> bool {
    if node.path == path || node.path.starts_with(path) {
        node.is_locked = false;
        node.allow_create_in_locked = false;
        // Also unlock all children recursively
        for child in &mut node.children {
            unlock_all_recursive(child);
        }
        return true;
    }
    
    for child in &mut node.children {
        restore_unlocked(child, path);
    }
    false
}

fn unlock_all_recursive(node: &mut file_tree::TreeNode) {
    node.is_locked = false;
    node.allow_create_in_locked = false;
    for child in &mut node.children {
        unlock_all_recursive(child);
    }
}

fn restore_allow_create(node: &mut file_tree::TreeNode, path: &Path) -> bool {
    if node.path == path && node.is_dir && node.is_locked {
        node.allow_create_in_locked = true;
        return true;
    }
    
    for child in &mut node.children {
        if restore_allow_create(child, path) {
            return true;
        }
    }
    false
}

fn init_command(root_path: &Path) -> Result<()> {
    let claude_md_path = root_path.join("CLAUDE.md");
    let icaros_md_path = root_path.join("ICAROS.md");
    
    // Load templates from embedded files or from prompts directory
    let icaros_content = load_template("ICAROS.md")?;
    
    // Write ICAROS.md
    fs::write(&icaros_md_path, icaros_content)?;
    println!("Created ICAROS.md with file lock system instructions");
    
    // Check if CLAUDE.md exists
    if claude_md_path.exists() {
        // Read existing content
        let claude_content = fs::read_to_string(&claude_md_path)?;
        
        // Check if it already references ICAROS.md
        if !claude_content.contains("ICAROS.md") {
            // Load update template
            let update_template = load_template("CLAUDE_UPDATE.md")?;
            
            // Remove any existing CLAUDE.md header variations
            let existing_content = claude_content
                .trim_start_matches("# CLAUDE.md - Project-Specific Instructions for Claude")
                .trim_start_matches("# CLAUDE.md - My Existing Instructions")
                .trim_start_matches("# CLAUDE.md")
                .trim_start();
            
            let updated_content = update_template.replace("{existing_content}", existing_content);
            fs::write(&claude_md_path, updated_content)?;
            println!("Updated CLAUDE.md to reference ICAROS.md");
        } else {
            println!("CLAUDE.md already references ICAROS.md");
        }
    } else {
        // Create new CLAUDE.md
        let claude_content = load_template("CLAUDE.md")?;
        fs::write(&claude_md_path, claude_content)?;
        println!("Created CLAUDE.md with reference to ICAROS.md");
    }
    
    Ok(())
}

fn load_template(filename: &str) -> Result<String> {
    // First try to load from user's config directory
    if let Some(config_dir) = dirs::config_dir() {
        let user_template = config_dir.join("icaros").join("prompts").join(filename);
        if user_template.exists() {
            return fs::read_to_string(user_template)
                .map_err(|e| anyhow::anyhow!("Failed to read user template: {}", e));
        }
    }
    
    // Then try from application's prompts directory
    let app_template = PathBuf::from("prompts").join(filename);
    if app_template.exists() {
        return fs::read_to_string(app_template)
            .map_err(|e| anyhow::anyhow!("Failed to read app template: {}", e));
    }
    
    // Finally, use the embedded defaults
    let content = match filename {
        "ICAROS.md" => include_str!("../prompts/ICAROS.md"),
        "CLAUDE.md" => include_str!("../prompts/CLAUDE.md"),
        "CLAUDE_UPDATE.md" => include_str!("../prompts/CLAUDE_UPDATE.md"),
        _ => return Err(anyhow::anyhow!("Unknown template: {}", filename)),
    };
    
    Ok(content.to_string())
}
