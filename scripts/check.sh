#!/bin/bash
set -e

echo "Running formatting check..."
cargo fmt --all -- --check

echo "Running Clippy..."
cargo clippy --all-targets --all-features

echo "Running tests..."
cargo test --all-features

echo "All checks passed!"
