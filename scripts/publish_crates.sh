#!/bin/bash
set -e

# Function to publish a crate
publish_crate() {
    local crate_dir=$1
    local retry_wait=10
    
    echo "---------------------------------------------------------"
    echo "Publishing crate in $crate_dir..."
    echo "---------------------------------------------------------"
    
    (cd "$crate_dir" && cargo publish)
    
    echo "Successfully published $crate_dir"
    echo "Waiting $retry_wait seconds for crate registry propagation..."
    sleep $retry_wait
}

# Ensure we are in the project root (simple check)
if [ ! -d "crates" ]; then
    echo "Error: Please run this script from the root of the repository."
    exit 1
fi

echo "Starting publication process for gtfs-guru ecosystem..."

# 1. gtfs-guru-model (Base dependency)
publish_crate "crates/gtfs_model"

# 2. gtfs-guru-core (Depends on model)
publish_crate "crates/gtfs_validator_core"

# 3. gtfs-guru-report (Depends on core)
publish_crate "crates/gtfs_validator_report"

# 4. gtfs-guru (CLI) (Depends on core and report)
publish_crate "crates/gtfs_validator_cli"

echo "---------------------------------------------------------"
echo "All crates have been successfully published!"
echo "---------------------------------------------------------"
