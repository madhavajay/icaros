#!/bin/bash
# Demo script to test icaros file monitoring

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

TEST_DIR="test_locked_files"

echo -e "${BLUE}🧪 Icaros File Monitoring Demo${NC}"
echo "================================"
echo ""

# Function to attempt file modification
attempt_modify() {
    local file=$1
    local content=$2
    local desc=$3
    
    echo -e "${YELLOW}Attempting to modify: $file${NC}"
    echo -e "Description: $desc"
    echo -e "New content: \"$content\""
    
    # Simulate different editors/processes
    case $4 in
        "vim")
            # Simulate vim edit
            echo "$content" > "$file"
            ;;
        "code")
            # Simulate VS Code (using a node process name)
            node -e "require('fs').writeFileSync('$file', '$content')" 2>/dev/null || echo "$content" > "$file"
            ;;
        *)
            # Default write
            echo "$content" > "$file"
            ;;
    esac
    
    echo -e "${GREEN}✓ Write attempted${NC}"
    echo "---"
    echo ""
    sleep 2
}

# Test 1: Modify unlocked file (should succeed)
echo -e "${BLUE}Test 1: Modifying UNLOCKED file${NC}"
attempt_modify "$TEST_DIR/test.txt" "This file is unlocked and can be modified!" "This should succeed" "vim"

# Test 2: Modify unlocked documentation (should succeed)
echo -e "${BLUE}Test 2: Modifying UNLOCKED documentation${NC}"
attempt_modify "$TEST_DIR/docs/README.md" "# Updated Documentation\nThis is allowed!" "Docs are unlocked" "vim"

# Test 3: Modify locked source file (should be blocked/reverted)
echo -e "${RED}Test 3: Modifying LOCKED source file${NC}"
attempt_modify "$TEST_DIR/src/main.rs" "fn main() { println!(\"Hacked!\"); }" "This should be BLOCKED" "code"

# Test 4: Modify locked config file (should be blocked/reverted)
echo -e "${RED}Test 4: Modifying LOCKED config file${NC}"
attempt_modify "$TEST_DIR/config/settings.toml" "config_value = 999" "This should be BLOCKED" "code"

# Test 5: Try to delete a locked file (should be blocked)
echo -e "${RED}Test 5: Attempting to DELETE locked file${NC}"
echo "Trying to delete src/main.rs..."
rm -f "$TEST_DIR/src/main.rs" 2>/dev/null && echo "File deleted" || echo "Delete failed"
echo ""
sleep 2

# Test 6: Create new file in locked directory (should be blocked)
echo -e "${RED}Test 6: Creating NEW file in locked directory${NC}"
echo "Trying to create src/new_file.rs..."
echo "fn new() {}" > "$TEST_DIR/src/new_file.rs" 2>/dev/null && echo "File created" || echo "Create failed"
echo ""

# Show final state
echo -e "${BLUE}Final file contents:${NC}"
echo "---"
for file in "$TEST_DIR/test.txt" "$TEST_DIR/docs/README.md" "$TEST_DIR/src/main.rs" "$TEST_DIR/config/settings.toml"; do
    if [ -f "$file" ]; then
        echo -e "${YELLOW}$file:${NC}"
        head -n 1 "$file"
        echo ""
    fi
done

echo -e "${GREEN}Demo complete!${NC}"
echo ""
echo "If monitoring was active, you should see:"
echo "- ✅ Unlocked files (test.txt, docs/*) were modified successfully"
echo "- 🛡️ Locked files (src/*, config/*) had their changes blocked/reverted"
echo ""
echo "Check the icaros terminal for blocked operation notifications!"