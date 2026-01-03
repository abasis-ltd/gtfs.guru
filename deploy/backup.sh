#!/bin/bash
# ============================================================================
# GTFS Validator - Backup Script
# Run this ON THE SERVER to create a backup of job data
# Usage: ./deploy/backup.sh [backup-directory]
# ============================================================================

set -euo pipefail

BACKUP_DIR="${1:-/root/backups}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="gtfs-validator-backup-$TIMESTAMP.tar.gz"

echo ""
echo "💾 GTFS Validator - Backup"
echo "=========================="
echo ""

# Create backup directory
mkdir -p "$BACKUP_DIR"

echo "Creating backup: $BACKUP_DIR/$BACKUP_FILE"

# Backup Docker volume data
docker run --rm \
    -v gtfs-validator_gtfs-data:/data:ro \
    -v "$BACKUP_DIR":/backup \
    alpine \
    tar czf "/backup/$BACKUP_FILE" -C /data .

# Show backup size
BACKUP_SIZE=$(du -h "$BACKUP_DIR/$BACKUP_FILE" | cut -f1)
echo ""
echo "✅ Backup complete!"
echo "   File: $BACKUP_DIR/$BACKUP_FILE"
echo "   Size: $BACKUP_SIZE"
echo ""

# Cleanup old backups (keep last 7)
echo "Cleaning up old backups (keeping last 7)..."
ls -t "$BACKUP_DIR"/gtfs-validator-backup-*.tar.gz 2>/dev/null | tail -n +8 | xargs -r rm -f

echo "Done!"
echo ""
