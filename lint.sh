#!/bin/bash
set -e

echo "Running cargo fmt check..."
cargo fmt -- --check

echo "Running cargo clippy with warnings as errors..."
cargo clippy -- -D warnings

echo "Running cargo clippy for all targets..."
cargo clippy --all-targets -- -D warnings

echo "Running tests..."
cargo test

echo "All checks passed!"%