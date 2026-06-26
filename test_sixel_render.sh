#!/usr/bin/env bash

# Script to test sixel rendering in teletipo
# This creates a simple sixel image and sends it to teletipo

# Create a temporary named pipe to communicate with teletipo
FIFO="/tmp/teletipo_sixel_test.fifo"
rm -f "$FIFO"
mkfifo "$FIFO"

# Generate a simple sixel image: 32x32 pixel gradient (8x2 chars)
# This is a minimal valid sixel with a simple pattern
SIXEL=$'\x1bPq#0;2;0;0;0#1;2;255;0;0#2;2;0;255;0#3;2;0;0;255"32;32;0#0~~~~~~-#1~~~~~~-#2~~~~~~-#3~~~~~~-\x1b\\'

# Write the sixel to the FIFO in the background
(sleep 0.5; echo -ne "$SIXEL"; sleep 1) > "$FIFO" &

# Run teletipo, redirecting stdin from the FIFO
echo "Starting teletipo with sixel input..."
timeout 10 /home/fernando/teletipo/target/release/teletipo < "$FIFO" 2>&1 | head -100

# Clean up
rm -f "$FIFO"
wait
