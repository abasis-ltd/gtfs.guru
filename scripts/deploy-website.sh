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

# 1. Sync public assets only; deployment/configuration files must not enter the web root.
log_step "Copying website files to server via rsync..."
rsync -az \
    --exclude Dockerfile \
    --exclude nginx.conf \
    "$WEBSITE_DIR/" "$SERVER:$REMOTE_DIR/"
ssh "$SERVER" "rm -f -- ~/gtfs-guru-web/Dockerfile ~/gtfs-guru-web/nginx.conf"

log_info "Website files copied successfully!"

# 2. Confirm the live site is really serving what we just pushed. rsync runs
# without --delete and the deploy is manual, so "it copied without an error" is
# not evidence that the browser validator works: a stale script.js next to a
# fresh pkg/ is exactly how every feed came to be reported as "too large".
log_step "Verifying the deployed site..."
if ! python3 "$SCRIPT_DIR/check_deployed_site.py" --base-url "${VERIFY_URL:-https://gtfs.guru}"; then
    log_error "The live site is not serving these files. The deploy is not finished."
    exit 1
fi

echo ""
echo "✅ Deployment complete and verified! Refresh your browser to see changes."
