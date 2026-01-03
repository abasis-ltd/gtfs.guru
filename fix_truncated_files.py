#!/usr/bin/env python3
"""
Recovery script: Remove truncated test modules from corrupted files.
This script finds files that don't end with '}' (truncated) and removes
the test module section (#[cfg(test)] mod tests { ... }) entirely,
allowing the main code to compile.
"""

import os
import re

RULES_DIR = "/Users/akimov/Documents/GitHub/gtfs-validator-rust/crates/gtfs_validator_core/src/rules"

def is_truncated(content):
    """Check if file is truncated (doesn't end with closing brace)."""
    stripped = content.rstrip()
    return not stripped.endswith('}')

def remove_test_module(content):
    """Remove the #[cfg(test)] mod tests section from the file."""
    # Find the start of the test module
    match = re.search(r'\n#\[cfg\(test\)\]\s*\nmod tests \{', content)
    if match:
        # Remove everything from #[cfg(test)] to end
        return content[:match.start()] + '\n'
    return content

def fix_file(filepath):
    """Fix a truncated file by removing its test module."""
    try:
        with open(filepath, 'r') as f:
            content = f.read()
    except Exception as e:
        print(f"Error reading {filepath}: {e}")
        return False
    
    if not is_truncated(content):
        return False
    
    # Check if it has a test module
    if '#[cfg(test)]' not in content:
        print(f"Truncated but no test module: {filepath}")
        return False
    
    new_content = remove_test_module(content)
    
    try:
        with open(filepath, 'w') as f:
            f.write(new_content)
        print(f"Fixed: {os.path.basename(filepath)}")
        return True
    except Exception as e:
        print(f"Error writing {filepath}: {e}")
        return False

def main():
    fixed = 0
    for filename in sorted(os.listdir(RULES_DIR)):
        if filename.endswith('.rs'):
            filepath = os.path.join(RULES_DIR, filename)
            if fix_file(filepath):
                fixed += 1
    
    print(f"\nFixed {fixed} files by removing corrupted test modules")
    print("The library code should now compile. Tests can be restored later.")

if __name__ == '__main__':
    main()
