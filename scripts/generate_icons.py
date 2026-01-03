import os
import struct
import zlib

def write_png(width, height, filename):
    # Minimal PNG implementation
    # Signature
    png = b'\x89PNG\r\n\x1a\n'
    
    # IHDR chunk
    # Color type 6 (Truecolor with alpha), bit depth 8
    ihdr_data = struct.pack('!IIBBBBB', width, height, 8, 6, 0, 0, 0)
    ihdr_crc = zlib.crc32(b'IHDR' + ihdr_data)
    png += struct.pack('!I', len(ihdr_data)) + b'IHDR' + ihdr_data + struct.pack('!I', ihdr_crc)
    
    # IDAT chunk
    # RGBA data: Red pixel (FF 00 00 FF)
    # Scanline filter 0 (None) at start of each row + width * 4 bytes
    raw_data = b''
    for _ in range(height):
        raw_data += b'\x00' + b'\xff\x00\x00\xff' * width
        
    compressed = zlib.compress(raw_data)
    idat_crc = zlib.crc32(b'IDAT' + compressed)
    png += struct.pack('!I', len(compressed)) + b'IDAT' + compressed + struct.pack('!I', idat_crc)
    
    # IEND chunk
    iend_crc = zlib.crc32(b'IEND')
    png += struct.pack('!I', 0) + b'IEND' + struct.pack('!I', iend_crc)
    
    with open(filename, 'wb') as f:
        f.write(png)

# Create icons directory
icons_dir = "crates/gtfs_validator_gui/icons"
if not os.path.exists(icons_dir):
    os.makedirs(icons_dir)

# Create PNGs
write_png(32, 32, os.path.join(icons_dir, "32x32.png"))
write_png(128, 128, os.path.join(icons_dir, "128x128.png"))
write_png(128, 128, os.path.join(icons_dir, "128x128@2x.png"))

# Create empty ICO/ICNS (Tauri might handle empty or just needs file existence, 
# but for safety let's copy the png content or just use minimal valid headers if needed. 
# Cargo build complained about PNG read error specifically. 
# For ICO/ICNS, let's just copy the PNG to them, sometimes that works or at least passes the minimal check if it acts as a container? 
# Actually, let's just leave them as touch files if errors were specifically about PNG. 
# The error was: "failed to read icon ... 32x32.png: unexpected end of file". 
# So PNGs definitely need to be valid.
# Let's hope ICO/ICNS are not strictly parsed during build if targets are not being bundled yet.)

# Better: just write something non-empty to ICO/ICNS if "touch" isn't enough, but usually only bundling checks them strictly.
# The error panicked in `tauri::generate_context!`, which likely scans for all icons defined in config.
# Let's make copies of PNG for ICO/ICNS to be safe against size checks, though format will be wrong. 
# If it fails on format, I'll need a proper generator. But let's start with PNGs.

with open(os.path.join(icons_dir, "32x32.png"), 'rb') as src:
    data = src.read()
    with open(os.path.join(icons_dir, "icon.ico"), 'wb') as dst:
        dst.write(data)
    with open(os.path.join(icons_dir, "icon.icns"), 'wb') as dst:
        dst.write(data)

print("Icons generated successfully")
