#!/usr/bin/env python3
"""
Fixed script to properly add ..Default::default() to struct initializations.
This script finds struct initializations that are missing fields and adds ..Default::default()
at the END of the struct, just before the closing brace.
"""

import os
import re

RULES_DIR = "/Users/akimov/Documents/GitHub/gtfs-validator-rust/crates/gtfs_validator_core/src/rules"

def find_struct_end(content, start_pos):
    """Find the closing brace of a struct starting at start_pos (which points to opening {)."""
    depth = 0
    i = start_pos
    while i < len(content):
        if content[i] == '{':
            depth += 1
        elif content[i] == '}':
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1

def fix_struct_default(content, open_brace_pos):
    """
    Fix a struct by removing ..Default::default() if it's at the wrong position
    and adding it properly before the closing brace.
    """
    close_brace = find_struct_end(content, open_brace_pos)
    if close_brace == -1:
        return content, False
    
    struct_content = content[open_brace_pos+1:close_brace]
    
    # Check if ..Default::default() already exists at correct position (end)
    # Correct position means it's the last thing before the closing brace
    stripped = struct_content.rstrip()
    if stripped.endswith('..Default::default()'):
        return content, False
    
    # Check if ..Default::default() exists somewhere in the struct (wrong position)
    if '..Default::default()' in struct_content:
        # It's in wrong position (at start), remove it
        struct_content = struct_content.replace('..Default::default(),', '')
        struct_content = struct_content.replace('..Default::default()', '')
        
        # Clean up any resulting double newlines
        while '\n\n\n' in struct_content:
            struct_content = struct_content.replace('\n\n\n', '\n\n')
    
    # Find the indentation of the opening brace line
    line_start = content.rfind('\n', 0, open_brace_pos)
    if line_start == -1:
        line_start = 0
    else:
        line_start += 1
    
    # Get base indent (indent of struct initializer + 4 spaces for field indent)
    base_indent = ""
    j = line_start
    while j < open_brace_pos and content[j] in ' \t':
        base_indent += content[j]
        j += 1
    field_indent = base_indent + "    "
    
    # Now add ..Default::default() at the end
    # First, find last non-whitespace character
    stripped_content = struct_content.rstrip()
    
    # Check if it ends with a comma
    if stripped_content and stripped_content[-1] == ',':
        # Already has trailing comma, just add the Default
        new_struct_content = stripped_content + f"\n{field_indent}..Default::default()\n{base_indent}"
    elif stripped_content:
        # Need to add comma first
        new_struct_content = stripped_content + f",\n{field_indent}..Default::default()\n{base_indent}"
    else:
        # Empty struct
        new_struct_content = f"\n{field_indent}..Default::default()\n{base_indent}"
    
    # Reconstruct the content
    new_content = content[:open_brace_pos+1] + new_struct_content + content[close_brace:]
    
    return new_content, True

def process_file(filepath):
    """Process a single file."""
    try:
        with open(filepath, 'r') as f:
            content = f.read()
    except Exception as e:
        print(f"Error reading {filepath}: {e}")
        return 0
    
    # Only process test modules
    if '#[cfg(test)]' not in content:
        return 0
    
    original = content
    changes = 0
    
    # Patterns to match struct initializations that need fixing
    patterns = [
        r'gtfs_model::Stop\s*\{',
        r'gtfs_model::Route\s*\{', 
        r'gtfs_model::StopTime\s*\{',
        r'gtfs_model::FareAttribute\s*\{',
        r'gtfs_model::FareRule\s*\{',
        r'gtfs_model::Transfer\s*\{',
        r'gtfs_model::Trip\s*\{',
    ]
    
    for pattern in patterns:
        offset = 0
        while True:
            match = re.search(pattern, content[offset:])
            if not match:
                break
            
            # Find the opening brace
            start = offset + match.start()
            brace_pos = offset + match.end() - 1
            while brace_pos < len(content) and content[brace_pos] != '{':
                brace_pos += 1
            
            if brace_pos >= len(content):
                break
            
            content, changed = fix_struct_default(content, brace_pos)
            if changed:
                changes += 1
            
            offset = start + 1
    
    if content != original:
        try:
            with open(filepath, 'w') as f:
                f.write(content)
            print(f"Fixed {filepath}: {changes} changes")
        except Exception as e:
            print(f"Error writing {filepath}: {e}")
            return 0
    
    return changes

def main():
    total = 0
    for filename in sorted(os.listdir(RULES_DIR)):
        if filename.endswith('.rs'):
            filepath = os.path.join(RULES_DIR, filename)
            total += process_file(filepath)
    print(f"\nTotal fixes: {total}")

if __name__ == '__main__':
    main()
