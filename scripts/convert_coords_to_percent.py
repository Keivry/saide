#!/usr/bin/env python3
"""
Convert keyboard mapping coordinates from physical resolution to percentages.

Usage:
    python scripts/convert_coords_to_percent.py

This script:
1. Queries device physical resolution via adb
2. Reads config.toml
3. Converts absolute coordinates to percentages (0.0-1.0)
4. Considers rotation angle when converting
5. Backs up original config.toml
6. Writes converted config
"""

import re
import subprocess
import sys
from pathlib import Path
from datetime import datetime


def get_device_physical_size():
    """Get device physical resolution via adb."""
    try:
        result = subprocess.run(
            ["adb", "shell", "wm", "size"],
            capture_output=True,
            text=True,
            check=True
        )
        
        # Parse: Physical size: 1260x2800
        match = re.search(r'Physical size:\s*(\d+)x(\d+)', result.stdout)
        if match:
            width, height = int(match.group(1)), int(match.group(2))
            print(f"✓ Detected device physical size: {width}x{height}")
            return width, height
        else:
            print("⚠ Could not parse physical size, using default: 1080x2340")
            return 1080, 2340
    except Exception as e:
        print(f"⚠ Failed to query device size: {e}")
        print("Using default: 1080x2340")
        return 1080, 2340


def convert_profile(profile_text, physical_width, physical_height):
    """
    Convert coordinates in a profile to percentages.
    
    Rotation angles:
    - 0: Portrait (0°)    - width=physical_width,  height=physical_height
    - 1: Landscape (90°)  - width=physical_height, height=physical_width
    - 2: Portrait (180°)  - width=physical_width,  height=physical_height
    - 3: Landscape (270°) - width=physical_height, height=physical_width
    """
    lines = profile_text.split('\n')
    
    # Find rotation value
    rotation = 0
    for line in lines:
        if match := re.match(r'rotation\s*=\s*(\d+)', line):
            rotation = int(match.group(1))
            break
    
    # Determine coordinate system based on rotation
    if rotation in [1, 3]:  # Landscape (90° or 270°)
        coord_width = physical_height
        coord_height = physical_width
    else:  # Portrait (0° or 180°)
        coord_width = physical_width
        coord_height = physical_height
    
    print(f"  Rotation {rotation} (effective resolution: {coord_width}x{coord_height})")
    
    # Convert coordinates
    converted_lines = []
    for line in lines:
        # Convert x coordinate
        if match := re.match(r'(x\s*=\s*)(\d+)(.*)', line):
            prefix, x_str, suffix = match.groups()
            x = int(x_str)
            x_percent = round(x / coord_width, 4)
            converted_lines.append(f'{prefix}{x_percent}  # was {x} / {coord_width}{suffix}')
            print(f"    x: {x} → {x_percent:.4f} ({x_percent*100:.2f}%)")
        # Convert y coordinate
        elif match := re.match(r'(y\s*=\s*)(\d+)(.*)', line):
            prefix, y_str, suffix = match.groups()
            y = int(y_str)
            y_percent = round(y / coord_height, 4)
            converted_lines.append(f'{prefix}{y_percent}  # was {y} / {coord_height}{suffix}')
            print(f"    y: {y} → {y_percent:.4f} ({y_percent*100:.2f}%)")
        else:
            converted_lines.append(line)
    
    return '\n'.join(converted_lines)


def main():
    config_path = Path(__file__).parent.parent / "config.toml"
    
    if not config_path.exists():
        print(f"❌ Config file not found: {config_path}")
        sys.exit(1)
    
    print(f"📄 Reading config: {config_path}")
    config_text = config_path.read_text()
    
    # Check if already converted
    if "# was " in config_text:
        print("⚠ Config appears to already be converted (contains '# was' comments)")
        response = input("Continue anyway? (y/N): ").strip().lower()
        if response != 'y':
            print("Aborted.")
            sys.exit(0)
    
    # Get device physical size
    physical_width, physical_height = get_device_physical_size()
    
    # Backup original
    backup_path = config_path.with_suffix(f'.toml.backup.{datetime.now().strftime("%Y%m%d_%H%M%S")}')
    backup_path.write_text(config_text)
    print(f"💾 Backed up to: {backup_path.name}")
    
    # Split config into profiles
    # Pattern: [[mappings.profiles]] ... next [[mappings.profiles]] or end
    profile_pattern = re.compile(r'(\[\[mappings\.profiles\]\].*?)(?=\[\[mappings\.profiles\]\]|\Z)', re.DOTALL)
    
    converted_parts = []
    last_end = 0
    
    for match in profile_pattern.finditer(config_text):
        # Add text before profile
        converted_parts.append(config_text[last_end:match.start()])
        
        # Convert profile
        profile_text = match.group(1)
        
        # Extract profile name for logging
        name_match = re.search(r'name\s*=\s*"([^"]+)"', profile_text)
        profile_name = name_match.group(1) if name_match else "Unknown"
        
        print(f"\n🔧 Converting profile: {profile_name}")
        converted_profile = convert_profile(profile_text, physical_width, physical_height)
        converted_parts.append(converted_profile)
        
        last_end = match.end()
    
    # Add remaining text
    converted_parts.append(config_text[last_end:])
    
    # Write converted config
    converted_config = ''.join(converted_parts)
    config_path.write_text(converted_config)
    
    print(f"\n✅ Conversion complete!")
    print(f"   Original backed up to: {backup_path.name}")
    print(f"   Converted config written to: {config_path.name}")
    print("\n💡 Next steps:")
    print("   1. Review the converted config.toml")
    print("   2. Update KeyboardMapper to use percentages")
    print("   3. Test the mappings in-game")


if __name__ == "__main__":
    main()
