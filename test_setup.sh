#!/bin/bash
# Test setup script for icaros file monitoring

echo "🔧 Setting up test environment for icaros file monitoring..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Create test directory structure
TEST_DIR="test_locked_files"
echo -e "${YELLOW}Creating test directory: $TEST_DIR${NC}"
mkdir -p $TEST_DIR/{src,docs,config}

# Create some test files
echo "Creating test files..."
echo "fn main() { println!(\"Hello, world!\"); }" > $TEST_DIR/src/main.rs
echo "# Important Documentation" > $TEST_DIR/docs/README.md
echo "config_value = 42" > $TEST_DIR/config/settings.toml
echo "This is a test file" > $TEST_DIR/test.txt

# Create .icaros state file with locked patterns
echo -e "${YELLOW}Creating .icaros state file with locked patterns...${NC}"
cat > $TEST_DIR/.icaros << 'EOF'
{
  "root_path": "test_locked_files",
  "active_profile": null,
  "profiles": {},
  "locked_patterns": [
    "src/**",
    "config/**"
  ],
  "unlocked_patterns": [
    "docs/**",
    "test.txt"
  ],
  "allow_create_patterns": [],
  "expanded_dirs": [
    "test_locked_files",
    "test_locked_files/src",
    "test_locked_files/docs",
    "test_locked_files/config"
  ]
}
EOF

echo -e "${GREEN}✅ Test environment created!${NC}"
echo ""
echo "Directory structure:"
tree $TEST_DIR 2>/dev/null || find $TEST_DIR -type f | sed 's|[^/]*/|  |g'
echo ""
echo -e "${YELLOW}Locked patterns:${NC}"
echo "  - src/** (all source files are locked)"
echo "  - config/** (all config files are locked)"
echo ""
echo -e "${GREEN}Unlocked patterns:${NC}"
echo "  - docs/** (documentation can be modified)"
echo "  - test.txt (test file can be modified)"
echo ""
echo -e "${YELLOW}To test the monitoring:${NC}"
echo "1. Run icaros with sudo:"
echo "   sudo cargo run test_locked_files"
echo ""
echo "2. Press 'm' to enable file monitoring"
echo ""
echo "3. In another terminal, run the demo script:"
echo "   ./test_demo.sh"
echo ""
echo "This will attempt to modify both locked and unlocked files to demonstrate the blocking."