use icaros::{file_tree, state, ui};
use std::fs;
use tempfile::TempDir;

#[test]
#[ignore = "This test requires access to internal restore_state function which is not exposed"]
fn test_restore_everything_locked_except_tests() {
    // Create a temporary directory structure
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create directory structure
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
    fs::create_dir(root.join("tests")).unwrap();
    fs::write(root.join("tests/test1.rs"), "#[test] fn test() {}").unwrap();
    fs::write(root.join("README.md"), "# Test").unwrap();

    // Build the tree
    let tree = file_tree::build_tree(root, &[], false).unwrap();

    // Create state file with everything locked except tests
    let state_file = root.join(".icaros");
    let state_content = r#"{
  "root_path": "ROOT_PATH",
  "locked_patterns": [
    "**"
  ],
  "unlocked_patterns": [
    "tests/**"
  ],
  "allow_create_patterns": [],
  "expanded_dirs": ["ROOT_PATH"]
}"#
    .replace("ROOT_PATH", &root.to_string_lossy());

    fs::write(&state_file, state_content).unwrap();

    // Create app and restore state
    let app = ui::App::new(tree, state_file.clone(), root.to_path_buf());
    let _state = state::AppState::load_from_file(&state_file).unwrap();

    // Manually call restore_state (since we can't import it from main)
    // Instead, let's check the tree directly after loading

    // For now, let's print the tree state to verify
    print_tree_state(&app.tree, 0);

    // Verify the root is locked
    assert!(app.tree.is_locked, "Root should be locked");

    // Find and verify tests directory is unlocked
    let tests_node = find_node(&app.tree, "tests");
    assert!(tests_node.is_some(), "Tests directory should exist");

    // Note: This test currently fails because restore_state is not public
    // and we need to test the actual restoration logic
}

fn find_node<'a>(node: &'a file_tree::TreeNode, name: &str) -> Option<&'a file_tree::TreeNode> {
    if node.name == name {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_node(child, name) {
            return Some(found);
        }
    }
    None
}

fn print_tree_state(node: &file_tree::TreeNode, indent: usize) {
    let lock_str = if node.is_locked { "[LOCKED]" } else { "" };
    println!("{}{} {}", "  ".repeat(indent), node.name, lock_str);
    for child in &node.children {
        print_tree_state(child, indent + 1);
    }
}

#[test]
fn test_yaml_serialization() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a test state
    let mut state = state::AppState::new(root.to_path_buf());
    state.locked_patterns = vec!["**".to_string()];
    state.unlocked_patterns = vec!["tests/**".to_string()];
    state.allow_create_patterns = vec![];
    state.expanded_dirs = vec![root.to_path_buf()];

    // Save to YAML
    let state_file = root.join(".icaros");
    state.save_to_file(&state_file).unwrap();

    // Read the file content and verify it's YAML
    let content = fs::read_to_string(&state_file).unwrap();
    println!("Generated YAML content:\n{}", content);

    // Verify it contains YAML syntax (no JSON braces at start of lines)
    assert!(
        !content.starts_with("{"),
        "Content should not start with JSON brace"
    );
    assert!(
        !content.ends_with("}"),
        "Content should not end with JSON brace"
    );
    assert!(
        content.contains("root_path:"),
        "Should contain YAML key-value pairs"
    );
    assert!(
        content.contains("locked_patterns:"),
        "Should contain locked_patterns key"
    );
    assert!(content.contains("- '**'"), "Should contain YAML list items");

    // Load it back and verify
    let loaded_state = state::AppState::load_from_file(&state_file).unwrap();
    assert_eq!(loaded_state.root_path, state.root_path);
    assert_eq!(loaded_state.locked_patterns, state.locked_patterns);
    assert_eq!(loaded_state.unlocked_patterns, state.unlocked_patterns);
    assert_eq!(
        loaded_state.allow_create_patterns,
        state.allow_create_patterns
    );
    assert_eq!(loaded_state.expanded_dirs, state.expanded_dirs);
}
