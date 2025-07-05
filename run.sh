#!/bin/bash

# Icaros Runner Script

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Building Icaros...${NC}"

# Build the project
if cargo build --release; then
    echo -e "${GREEN}Build successful!${NC}"
    echo ""
    echo -e "${BLUE}Starting Icaros...${NC}"
    echo ""
    echo "Controls:"
    echo "  ↑/↓     - Navigate up/down"
    echo "  Enter   - Open/close folders (expand/collapse)"
    echo "  Space   - Lock/unlock files or folders"
    echo "  q       - Quit and save"
    echo ""
    
    # Run with any arguments passed to the script
    cargo run --release -- "$@"
else
    echo "Build failed!"
    exit 1
fi