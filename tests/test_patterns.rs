use std::path::PathBuf;
use icaros::state::AppState;
use icaros::file_tree::TreeNode;

#[test]
fn test_root_lock_pattern() {
    let root_path = PathBuf::from("/test/root");
    let root_node = TreeNode {
        path: root_path.clone(),
        name: "root".to_string(),
        is_dir: true,
        is_locked: true,
        allow_create_in_locked: false,
        is_expanded: false,
        children: vec![],
        depth: 0,
    };
    
    let mut state = AppState::new(root_path.clone());
    state.update_from_tree(&root_node);
    
    assert_eq!(state.locked_patterns, vec!["**"]);
    assert_eq!(state.unlocked_patterns, Vec::<String>::new());  // When everything is locked, nothing is unlocked
}

#[test]
fn test_subdirectory_lock_pattern() {
    let root_path = PathBuf::from("/test/root");
    let src_path = root_path.join("src");
    
    let src_node = TreeNode {
        path: src_path.clone(),
        name: "src".to_string(),
        is_dir: true,
        is_locked: true,
        allow_create_in_locked: false,
        is_expanded: false,
        children: vec![],
        depth: 1,
    };
    
    let root_node = TreeNode {
        path: root_path.clone(),
        name: "root".to_string(),
        is_dir: true,
        is_locked: false,
        allow_create_in_locked: false,
        is_expanded: true,
        children: vec![src_node],
        depth: 0,
    };
    
    let mut state = AppState::new(root_path.clone());
    state.update_from_tree(&root_node);
    
    assert_eq!(state.locked_patterns, vec!["src/**"]);
}

#[test]
fn test_file_lock_pattern() {
    let root_path = PathBuf::from("/test/root");
    let readme_path = root_path.join("README.md");
    
    let readme_node = TreeNode {
        path: readme_path.clone(),
        name: "README.md".to_string(),
        is_dir: false,
        is_locked: true,
        allow_create_in_locked: false,
        is_expanded: false,
        children: vec![],
        depth: 1,
    };
    
    let root_node = TreeNode {
        path: root_path.clone(),
        name: "root".to_string(),
        is_dir: true,
        is_locked: false,
        allow_create_in_locked: false,
        is_expanded: true,
        children: vec![readme_node],
        depth: 0,
    };
    
    let mut state = AppState::new(root_path.clone());
    state.update_from_tree(&root_node);
    
    assert_eq!(state.locked_patterns, vec!["README.md"]);
}

#[test]
fn test_allow_create_pattern() {
    let root_path = PathBuf::from("/test/root");
    let src_path = root_path.join("src");
    
    let src_node = TreeNode {
        path: src_path.clone(),
        name: "src".to_string(),
        is_dir: true,
        is_locked: true,
        allow_create_in_locked: true,  // This directory allows creation
        is_expanded: false,
        children: vec![],
        depth: 1,
    };
    
    let root_node = TreeNode {
        path: root_path.clone(),
        name: "root".to_string(),
        is_dir: true,
        is_locked: false,
        allow_create_in_locked: false,
        is_expanded: true,
        children: vec![src_node],
        depth: 0,
    };
    
    let mut state = AppState::new(root_path.clone());
    state.update_from_tree(&root_node);
    
    assert_eq!(state.locked_patterns, vec!["src/**"]);
    assert_eq!(state.allow_create_patterns, vec!["src/**"]);
}

#[test]
fn test_unlocked_patterns_logic() {
    let root_path = PathBuf::from("/test/root");
    
    // Test 1: Nothing locked
    let mut state1 = AppState::new(root_path.clone());
    state1.locked_patterns = vec![];
    state1.unlocked_patterns = icaros::state::calculate_unlocked_patterns(&state1.locked_patterns);
    assert_eq!(state1.unlocked_patterns, vec!["**"]);
    
    // Test 2: Everything locked
    let mut state2 = AppState::new(root_path.clone());
    state2.locked_patterns = vec!["**".to_string()];
    state2.unlocked_patterns = icaros::state::calculate_unlocked_patterns(&state2.locked_patterns);
    assert_eq!(state2.unlocked_patterns, Vec::<String>::new());
    
    // Test 3: Specific paths locked
    let mut state3 = AppState::new(root_path.clone());
    state3.locked_patterns = vec!["src/**".to_string(), "tests/**".to_string()];
    state3.unlocked_patterns = icaros::state::calculate_unlocked_patterns(&state3.locked_patterns);
    assert_eq!(state3.unlocked_patterns, vec!["**"]);
}

#[test]
fn test_nested_allow_create_with_root_locked() {
    // Test case: root is locked, but nested directories have allow_create
    let root_path = PathBuf::from("/test/root");
    
    // Create nested structure
    let src_node = TreeNode {
        path: root_path.join("src"),
        name: "src".to_string(),
        is_dir: true,
        is_locked: true,
        allow_create_in_locked: true,  // This should be collected even though parent is locked
        is_expanded: false,
        children: vec![],
        depth: 1,
    };
    
    let tests_node = TreeNode {
        path: root_path.join("tests"),
        name: "tests".to_string(),
        is_dir: true,
        is_locked: true,
        allow_create_in_locked: true,  // This should also be collected
        is_expanded: false,
        children: vec![],
        depth: 1,
    };
    
    let root_node = TreeNode {
        path: root_path.clone(),
        name: "root".to_string(),
        is_dir: true,
        is_locked: true,  // Root is locked
        allow_create_in_locked: false,
        is_expanded: true,
        children: vec![src_node, tests_node],
        depth: 0,
    };
    
    let mut state = AppState::new(root_path.clone());
    state.update_from_tree(&root_node);
    
    // When root is locked, we should see "**" in locked_patterns
    assert_eq!(state.locked_patterns, vec!["**"]);
    // But we should still collect the allow_create patterns from nested directories
    assert_eq!(state.allow_create_patterns.len(), 2);
    assert!(state.allow_create_patterns.contains(&"src/**".to_string()));
    assert!(state.allow_create_patterns.contains(&"tests/**".to_string()));
}

#[test]
fn test_unlocked_subdirectory_in_locked_parent() {
    // Test case: root is locked, but some subdirectories are unlocked
    let root_path = PathBuf::from("/test/root");
    
    // Create an unlocked src directory
    let src_node = TreeNode {
        path: root_path.join("src"),
        name: "src".to_string(),
        is_dir: true,
        is_locked: false,  // This is unlocked despite parent being locked
        allow_create_in_locked: false,
        is_expanded: false,
        children: vec![],
        depth: 1,
    };
    
    // Create a locked tests directory
    let tests_node = TreeNode {
        path: root_path.join("tests"),
        name: "tests".to_string(),
        is_dir: true,
        is_locked: true,
        allow_create_in_locked: false,
        is_expanded: false,
        children: vec![],
        depth: 1,
    };
    
    let root_node = TreeNode {
        path: root_path.clone(),
        name: "root".to_string(),
        is_dir: true,
        is_locked: true,  // Root is locked
        allow_create_in_locked: false,
        is_expanded: true,
        children: vec![src_node, tests_node],
        depth: 0,
    };
    
    let mut state = AppState::new(root_path.clone());
    state.update_from_tree(&root_node);
    
    // When root is locked, we should see "**" in locked_patterns
    assert_eq!(state.locked_patterns, vec!["**"]);
    // But we should also see the unlocked subdirectory
    assert_eq!(state.unlocked_patterns, vec!["src/**"]);
}