#!/bin/bash

# Script to create ANSI art files from images using chafa

# Create the art directory if it doesn't exist
mkdir -p animations/art

# Example: Convert an image to ANSI art
# Usage: ./create_ansi_art.sh <input_image> <output_file>

if [ $# -lt 2 ]; then
    echo "Usage: $0 <input_image> <output_file>"
    echo "Example: $0 jungle.jpg animations/art/jungle.ans"
    exit 1
fi

INPUT_IMAGE="$1"
OUTPUT_FILE="$2"

# Check if chafa is installed
if ! command -v chafa &> /dev/null; then
    echo "Error: chafa is not installed"
    echo "Install with: brew install chafa"
    exit 1
fi

# Generate ANSI art with chafa
# Adjust size based on your terminal/display preferences
chafa --colors=256 --format=ansi --size=40x15 "$INPUT_IMAGE" > "$OUTPUT_FILE"

echo "Created ANSI art: $OUTPUT_FILE"