#!/usr/bin/env python3
"""
ABOUTME: Script to generate RGBA PNG icons for xNAUT application
ABOUTME: Creates proper alpha-channel icons in required sizes
"""

from PIL import Image, ImageDraw
import os

def create_gradient_circle_icon(size, output_path):
    """
    Create a circular icon with a blue-to-purple gradient background.

    Args:
        size: Tuple of (width, height) for the icon
        output_path: Path where to save the PNG file
    """
    # Create RGBA image (with alpha channel)
    img = Image.new('RGBA', size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Calculate circle dimensions with some padding
    padding = size[0] // 8
    circle_bbox = [padding, padding, size[0] - padding, size[1] - padding]

    # Draw the circle with a blue-purple gradient effect
    # We'll simulate gradient by drawing multiple concentric circles
    center_x = size[0] // 2
    center_y = size[1] // 2
    max_radius = (size[0] - 2 * padding) // 2

    for i in range(max_radius, 0, -1):
        # Gradient from blue (70, 130, 255) to purple (139, 92, 246)
        ratio = i / max_radius
        r = int(70 + (139 - 70) * (1 - ratio))
        g = int(130 + (92 - 130) * (1 - ratio))
        b = int(255 + (246 - 255) * (1 - ratio))

        color = (r, g, b, 255)  # Fully opaque

        bbox = [
            center_x - i,
            center_y - i,
            center_x + i,
            center_y + i
        ]
        draw.ellipse(bbox, fill=color)

    # Save as PNG with RGBA
    img.save(output_path, 'PNG')
    print(f"Created {output_path} ({size[0]}x{size[1]}) - RGBA format")

def main():
    # Define the icons directory
    icons_dir = '/home/jarvis/projects/NautCode/xnowd/src-tauri/icons'

    # Create icons directory if it doesn't exist
    os.makedirs(icons_dir, exist_ok=True)

    # Generate the required icons
    create_gradient_circle_icon((32, 32), os.path.join(icons_dir, '32x32.png'))
    create_gradient_circle_icon((128, 128), os.path.join(icons_dir, '128x128.png'))
    create_gradient_circle_icon((256, 256), os.path.join(icons_dir, '128x128@2x.png'))

    print("\nAll icons generated successfully!")
    print("Icons are in RGBA format with alpha channel transparency.")

if __name__ == '__main__':
    main()
