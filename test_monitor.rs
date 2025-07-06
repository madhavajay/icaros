// Simple test program to demonstrate fs_usage_sys monitoring integration
// This is a standalone test that can be run to verify the monitoring works

use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

fn main() {
    println!("🧪 Testing fs_usage_sys monitoring functionality...\n");
    
    // First, set up the test environment
    println!("Setting up test files...");
    let test_dir = PathBuf::from("test_monitor_demo");
    fs::create_dir_all(&test_dir).expect("Failed to create test directory");
    
    // Create some test files
    let locked_file = test_dir.join("locked.txt");
    let unlocked_file = test_dir.join("unlocked.txt");
    
    fs::write(&locked_file, "This file is locked").expect("Failed to create locked file");
    fs::write(&unlocked_file, "This file is unlocked").expect("Failed to create unlocked file");
    
    println!("Created test files:");
    println!("  - {:?} (will be locked)", locked_file);
    println!("  - {:?} (will remain unlocked)", unlocked_file);
    
    // Note: In the real implementation, this would be integrated with icaros
    // For now, this is just a demonstration of the concept
    
    println!("\n📝 Testing file operations...\n");
    
    // Test 1: Write to unlocked file
    println!("Test 1: Writing to UNLOCKED file");
    match fs::write(&unlocked_file, "Modified content - this should work!") {
        Ok(_) => println!("✅ Success: Unlocked file was modified"),
        Err(e) => println!("❌ Error: {}", e),
    }
    
    // Small delay
    thread::sleep(Duration::from_secs(1));
    
    // Test 2: Attempt to write to locked file
    println!("\nTest 2: Writing to LOCKED file");
    println!("⚠️  Note: In the full implementation, this would be blocked by the monitor");
    match fs::write(&locked_file, "Trying to modify locked file!") {
        Ok(_) => println!("⚡ File was written (monitor would revert this)"),
        Err(e) => println!("❌ Error: {}", e),
    }
    
    // Test 3: Delete attempt
    println!("\nTest 3: Attempting to DELETE locked file");
    println!("⚠️  Note: In the full implementation, this would be blocked");
    match fs::remove_file(&locked_file) {
        Ok(_) => println!("⚡ File was deleted (monitor would log this)"),
        Err(e) => println!("❌ Error: {}", e),
    }
    
    println!("\n🔍 Final file contents:");
    
    if unlocked_file.exists() {
        let content = fs::read_to_string(&unlocked_file).unwrap_or_default();
        println!("Unlocked file: {}", content);
    } else {
        println!("Unlocked file: [deleted]");
    }
    
    if locked_file.exists() {
        let content = fs::read_to_string(&locked_file).unwrap_or_default();
        println!("Locked file: {}", content);
    } else {
        println!("Locked file: [deleted]");
    }
    
    // Clean up
    println!("\n🧹 Cleaning up test files...");
    let _ = fs::remove_dir_all(&test_dir);
    
    println!("\n✅ Test complete!");
    println!("\nTo see the full monitoring in action:");
    println!("1. Run: ./test_setup.sh");
    println!("2. Run: sudo cargo run -- --monitor test_locked_files");
    println!("3. In another terminal: ./test_demo.sh");
}