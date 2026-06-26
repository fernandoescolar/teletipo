#!/usr/bin/env bash

# Sixel Rendering Diagnostic Tool
# This script analyzes sixel files and provides information about their rendering

if [ $# -eq 0 ]; then
    echo "Sixel Diagnostic Tool - Analyze sixel image files"
    echo "Usage: $0 <sixel-file> [<sixel-file> ...]"
    echo ""
    echo "This tool examines sixel DCS sequences and provides rendering information."
    exit 1
fi

analyze_sixel() {
    local file="$1"
    
    if [ ! -f "$file" ]; then
        echo "Error: File not found: $file"
        return 1
    fi
    
    echo "================================"
    echo "File: $file"
    echo "Size: $(ls -lh "$file" | awk '{print $5}')"
    echo "================================"
    
    # Extract raster attributes
    local raster=$(od -An -tx1 "$file" | head -1)
    echo "Header bytes: $raster"
    
    # Check for DCS start (1b 50 71 = ESC P q)
    if [[ $raster =~ ^.*\ 1b\ 50\ 71 ]]; then
        echo "✓ Valid DCS sixel header (ESC P q) found"
    else
        echo "✗ Warning: DCS header might be missing or malformed"
    fi
    
    # Extract raster attributes if they start with a quote
    local hex_dump=$(od -An -tx1 -N 100 "$file" | tr -d '\n' | sed 's/ //g')
    
    # Look for raster format pattern: 22 followed by digits and semicolons
    # The raster attributes look like: "1;1;600;450
    if echo "$hex_dump" | grep -q '^.*1b5071.*22'; then
        # Found quote after Pq, extract until next # (23)
        local raster_attr=$(echo "$hex_dump" | sed 's/^.*1b507122//' | sed 's/23.*//')
        
        # Convert hex pairs to ASCII
        local raster_str=""
        while [ ${#raster_attr} -gt 0 ]; do
            local hex_pair="${raster_attr:0:2}"
            local char=$(printf "\\x$hex_pair" 2>/dev/null)
            raster_str="$raster_str$char"
            raster_attr="${raster_attr:2}"
        done
        
        echo "Raster attributes found: \"$raster_str"
        
        # Parse dimensions from raster attributes
        IFS=';' read -ra parts <<< "$raster_str"
        if [ ${#parts[@]} -ge 4 ]; then
            echo "  Format: Extended (4-param)"
            echo "  Aspect ratio: ${parts[0]}:${parts[1]}"
            echo "  Width:  ${parts[2]} pixels"
            echo "  Height: ${parts[3]} pixels"
            
            local w=${parts[2]}
            local h=${parts[3]}
            
            # Calculate grid dimensions (assuming 8x16 pixel cells)
            local cols=$((($w + 7) / 8))
            local rows=$((($h + 15) / 16))
            echo "  Grid placement: ~$cols columns x $rows rows"
            
            # Check if image might extend beyond typical terminal
            if [ $cols -gt 80 ]; then
                echo "  ⚠ Warning: Image is $cols columns wide (typical terminal: 80)"
                echo "     Image may extend beyond visible area or require scrolling"
            fi
            if [ $rows -gt 24 ]; then
                echo "  ⚠ Warning: Image is $rows rows tall (typical terminal: 24)"
                echo "     Bottom part of image will be off-screen"
            fi
        elif [ ${#parts[@]} -ge 2 ]; then
            echo "  Format: Legacy (3-param)"
            echo "  Width:  ${parts[0]} pixels"
            echo "  Height: ${parts[1]} pixels"
        fi
    fi
    
    # Check for proper termination
    local tail=$(od -An -tx1 -N 10 "$file" | tail -c 20)
    if [[ $tail =~ 1b\ 5c ]]; then
        echo "✓ Proper termination (ESC \\) found"
    else
        echo "✗ Warning: Proper termination (ESC \\) not found"
    fi
    
    echo ""
}

for file in "$@"; do
    analyze_sixel "$file"
done
