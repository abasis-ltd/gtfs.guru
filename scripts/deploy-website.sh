#!/bin/bash
# ============================================================================
# Deploy ONLY the website files to Hetzner using SCP
# Usage: ./scripts/deploy-website.sh [server-ip-or-hostname]
# ============================================================================

set -euo pipefail

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_step() { echo -e "${BLUE}[STEP]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Get server from argument
SERVER="${1:-}"

if [[ -z "$SERVER" ]]; then
    log_error "Usage: $0 <server-ip-or-hostname>"
    exit 1
fi

# Add botuser@ if not specified
if [[ "$SERVER" != *"@"* ]]; then
    SERVER="botuser@$SERVER"
fi

REMOTE_DIR="~/gtfs-guru-web"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
WEBSITE_DIR="$PROJECT_DIR/website"

echo ""
echo "🚀 GTFS Guru - Deploy Website (SCP)"
echo "======================================"
echo "Server: $SERVER"
echo "Remote directory: $REMOTE_DIR"
echo ""

# 1. Sync website files using SCP (matching user's past workflow)
log_step "Copying website files to server via SCP..."
scp -r "$WEBSITE_DIR"/* "$SERVER:$REMOTE_DIR"

log_info "Website files copied successfully!"
echo ""
echo "✅ Deployment complete! Refresh your browser to see changes."
