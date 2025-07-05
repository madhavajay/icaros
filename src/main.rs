mod file_tree;
mod state;
mod ui;

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
            
            let tree = file_tree::build_tree(&root_path, &args.ignore)?;
            
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
    
    // Apply locked patterns
    for pattern in &state.locked_patterns {
        if pattern == "**" {
            eprintln!("  Locking entire tree with pattern: {}", pattern);
            // Lock entire tree recursively
            lock_all_recursive(&mut app.tree);
        } else if let Some(path) = pattern_to_path(&state.root_path, pattern) {
            eprintln!("  Locking path: {:?} from pattern: {}", path, pattern);
            restore_locked(&mut app.tree, &path);
        }
    }
    
    // Then apply unlocked patterns (exceptions to locked patterns)
    // This must come after locked patterns to override them
    for pattern in &state.unlocked_patterns {
        if let Some(path) = pattern_to_path(&state.root_path, pattern) {
            eprintln!("  Unlocking path: {:?} from pattern: {}", path, pattern);
            restore_unlocked(&mut app.tree, &path);
        }
    }
    
    // Finally restore allow_create patterns
    for pattern in &state.allow_create_patterns {
        if let Some(path) = pattern_to_path(&state.root_path, pattern) {
            restore_allow_create(&mut app.tree, &path);
        }
    }
    
    app.update_items();
    eprintln!("State restoration complete.");
}

fn pattern_to_path(root: &std::path::Path, pattern: &str) -> Option<std::path::PathBuf> {
    if pattern == "**" {
        // Special case: root directory is locked
        Some(root.to_path_buf())
    } else {
        let clean_pattern = pattern.trim_end_matches("/**");
        if clean_pattern.is_empty() {
            Some(root.to_path_buf())
        } else {
            Some(root.join(clean_pattern))
        }
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
    
    // Create ICAROS.md content with lock system instructions
    let icaros_content = r#"# ICAROS.md - File Lock System Guide

> Icaros are sacred songs and chants used in Central and South American traditions to guide and protect people during spiritual journeys and vision quests.

You are the Shaman-in-the-loop. Use Icaros to guide your Agentic People Spirits on their journey and keep them from 1-shotting themselves.

## CRITICAL: File Lock System

**BEFORE making ANY file modifications in this project:**

1. **ALWAYS** read `.icaros` first
2. Check the `locked_patterns` array
3. Lock rules:
   - If a file/directory matches a pattern in `locked_patterns` → REFUSE all operations (edit, delete, create)
   - Exception: If directory is in `allow_create_patterns` → ALLOW creating new files only
4. Default: Everything is unlocked unless explicitly in `locked_patterns`
5. If locked, inform user that the file/directory is locked

## Lock File Location
- Primary: `.icaros` (in project root)
- Alternative: Check `--state-file` argument if specified

## Example Workflow
```yaml
Before any file operation:
1. Read .icaros
2. Parse locked_patterns array
3. Check if target path matches any pattern:
   - "src/**" matches src/main.rs, src/lib.rs, src/utils/helper.rs
   - "README.md" matches only README.md
4. If matched in locked_patterns:
   - For create operation: Check allow_create_patterns
   - For edit/delete: Always refuse
5. If not matched → proceed with operation
```

## Pattern Matching
- `**` wildcard matches any number of directories
- `dir/**` locks entire directory tree
- Specific files use exact paths relative to root
- Compact representation: if entire dir is locked, just show `dir/**`

## Remember
- The lock file uses absolute paths
- Lock state is saved immediately after changes
- Locked directories lock all their children
- This system helps users control which files AI can modify
"#;

    // Write ICAROS.md
    fs::write(&icaros_md_path, icaros_content)?;
    println!("Created ICAROS.md with file lock system instructions");
    
    // Check if CLAUDE.md exists
    if claude_md_path.exists() {
        // Read existing content
        let claude_content = fs::read_to_string(&claude_md_path)?;
        
        // Check if it already references ICAROS.md
        if !claude_content.contains("ICAROS.md") {
            // Add reference to ICAROS.md at the beginning
            let updated_content = format!(
                "# CLAUDE.md - Project-Specific Instructions for Claude\n\n## File Lock System\nSee [ICAROS.md](./ICAROS.md) for critical file lock system instructions.\n\n{}",
                claude_content.trim_start_matches("# CLAUDE.md - Project-Specific Instructions for Claude").trim_start()
            );
            fs::write(&claude_md_path, updated_content)?;
            println!("Updated CLAUDE.md to reference ICAROS.md");
        } else {
            println!("CLAUDE.md already references ICAROS.md");
        }
    } else {
        // Create new CLAUDE.md
        let claude_content = r#"# CLAUDE.md - Project-Specific Instructions for Claude

## File Lock System
See [ICAROS.md](./ICAROS.md) for critical file lock system instructions.

## Project Description
[Add your project-specific instructions here]

## Important Reminders
- Always check the file lock system before modifying files
- Respect locked patterns to prevent unwanted modifications
"#;
        fs::write(&claude_md_path, claude_content)?;
        println!("Created CLAUDE.md with reference to ICAROS.md");
    }
    
    Ok(())
}
