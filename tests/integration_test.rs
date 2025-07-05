use std::fs;
use std::path::PathBuf;
use icaros::{file_tree, state};

#[test]
fn test_build_tree() {
    let temp_dir = std::env::temp_dir().join("claude_tree_test");
    fs::create_dir_all(&temp_dir).unwrap();
    
    fs::create_dir_all(temp_dir.join("src")).unwrap();
    fs::write(temp_dir.join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(temp_dir.join("README.md"), "# Test").unwrap();
    
    let tree = file_tree::build_tree(&temp_dir, &[]).unwrap();
    
    assert_eq!(tree.name, "claude_tree_test");
    assert!(tree.is_dir);
    assert_eq!(tree.children.len(), 2);
    
    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_state_persistence() {
    let temp_dir = std::env::temp_dir().join("claude_state_test");
    fs::create_dir_all(&temp_dir).unwrap();
    
    let state_file = temp_dir.join("test_state.json");
    
    let mut state = state::AppState::new(PathBuf::from("/test"));
    state.locked_patterns = vec!["src/**".to_string(), "README.md".to_string()];
    state.allow_create_patterns = vec!["src/**".to_string()];
    state.save_to_file(&state_file).unwrap();
    
    let loaded_state = state::AppState::load_from_file(&state_file).unwrap();
    assert_eq!(loaded_state.locked_patterns, vec!["src/**", "README.md"]);
    assert_eq!(loaded_state.allow_create_patterns, vec!["src/**"]);
    assert_eq!(loaded_state.root_path, PathBuf::from("/test"));
    
    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_tree_node_locking() {
    let mut node = file_tree::TreeNode::new(
        PathBuf::from("/test/file.rs"),
        "file.rs".to_string(),
        false,
        0
    );
    
    assert!(!node.is_locked);
    node.toggle_lock();
    assert!(node.is_locked);
    node.toggle_lock();
    assert!(!node.is_locked);
}

#[test]
fn test_tree_node_with_children() {
    let mut parent = file_tree::TreeNode::new(
        PathBuf::from("/test"),
        "test".to_string(),
        true,
        0
    );
    
    let child1 = file_tree::TreeNode::new(
        PathBuf::from("/test/file1.rs"),
        "file1.rs".to_string(),
        false,
        1
    );
    
    let child2 = file_tree::TreeNode::new(
        PathBuf::from("/test/file2.rs"),
        "file2.rs".to_string(),
        false,
        1
    );
    
    parent.children.push(child1);
    parent.children.push(child2);
    
    parent.toggle_lock();
    
    assert!(parent.is_locked);
    assert!(parent.children[0].is_locked);
    assert!(parent.children[1].is_locked);
    
    let locked_files = parent.get_locked_files();
    assert_eq!(locked_files.len(), 2);
}