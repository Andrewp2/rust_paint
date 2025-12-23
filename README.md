# Rust Paint

A simple local paint app built with `eframe/egui`.

## Features
- Brush and eraser with adjustable size
- Lines, rectangles, ellipses, and table grids
- Paint bucket fill
- Text tool with system font rendering, caret, and selection
- Palette + recent swatches
- PNG export

## Run
```bash
cargo run
```

For better performance:
```bash
cargo run --release
```

## Usage
- Select a tool from the left panel.
- Brush/Eraser: click-drag to draw.
- Shapes: click-drag to place, release to commit.
- Bucket: click to fill a region.
- Text: click to place a caret, type, Enter to commit, Esc to cancel.

## Notes
- The canvas is bitmap-backed; undo is not currently available.
- PNG export saves the current canvas.
