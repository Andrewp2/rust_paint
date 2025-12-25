use eframe::egui;

struct PaintApp {
    tool: Tool,
    brush_color: egui::Color32,
    brush_size: f32,
    background: egui::Color32,
    status: Option<String>,
    canvas: Canvas,
    drag_start: Option<egui::Pos2>,
    drag_current: Option<egui::Pos2>,
    last_draw_pos: Option<egui::Pos2>,
    table_rows: u32,
    table_cols: u32,
    palette: Vec<egui::Color32>,
    recent: Vec<egui::Color32>,
    max_recent: usize,
    text_scale: u32,
    text_active: bool,
    text_pos: egui::Pos2,
    text_buffer: String,
    text_renderer: TextRenderer,
    text_preview: Option<TextPreview>,
    text_caret: usize,
    text_selection: Option<usize>,
    next_layer_id: u32,
}

struct TextPreview {
    texture: egui::TextureHandle,
    size: egui::Vec2,
    offset: egui::Vec2,
    key: PreviewKey,
}

#[derive(Clone, PartialEq, Eq)]
struct PreviewKey {
    text: String,
    size: u32,
    color: egui::Color32,
    scale: u32,
}

struct Canvas {
    composite: image::RgbaImage,
    layers: Vec<Layer>,
    active_layer: usize,
    origin: egui::Pos2,
    scale: f32,
    view_offset: egui::Vec2,
    viewport_size: egui::Vec2,
    composite_dirty: Option<DirtyRect>,
    tiles: Vec<Tile>,
    tiles_per_row: u32,
}

struct Layer {
    name: String,
    image: image::RgbaImage,
    visible: bool,
}

#[derive(Clone, Copy)]
struct DirtyRect {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

impl Layer {
    fn new(name: &str, width: u32, height: u32) -> Self {
        let mut image = image::RgbaImage::new(width, height);
        let transparent = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0);
        fill_image(&mut image, transparent);
        Self {
            name: name.to_string(),
            image,
            visible: true,
        }
    }
}

struct Tile {
    rect: TileRect,
    texture: Option<egui::TextureHandle>,
    dirty: bool,
    name: String,
}

#[derive(Clone, Copy)]
struct TileRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

const TILE_SIZE: u32 = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Brush,
    Eraser,
    Line,
    Rectangle,
    Table,
    Ellipse,
    Bucket,
    Text,
}

impl Default for Tool {
    fn default() -> Self {
        Self::Brush
    }
}

impl PaintApp {
    fn new() -> Self {
        let background = egui::Color32::from_rgb(200, 200, 200);
        let scale = 1.0;
        let size = egui::vec2(900.0, 600.0);
        Self {
            tool: Tool::Brush,
            brush_color: egui::Color32::BLACK,
            brush_size: 4.0,
            background,
            status: None,
            canvas: Canvas::new(background, size, scale),
            drag_start: None,
            drag_current: None,
            last_draw_pos: None,
            table_rows: 3,
            table_cols: 3,
            palette: vec![
                egui::Color32::BLACK,
                egui::Color32::WHITE,
                egui::Color32::from_rgb(244, 67, 54),
                egui::Color32::from_rgb(255, 152, 0),
                egui::Color32::from_rgb(255, 235, 59),
                egui::Color32::from_rgb(76, 175, 80),
                egui::Color32::from_rgb(0, 150, 136),
                egui::Color32::from_rgb(33, 150, 243),
                egui::Color32::from_rgb(63, 81, 181),
                egui::Color32::from_rgb(156, 39, 176),
                egui::Color32::from_rgb(121, 85, 72),
                egui::Color32::from_rgb(158, 158, 158),
            ],
            recent: Vec::new(),
            max_recent: 12,
            text_scale: 2,
            text_active: false,
            text_pos: egui::pos2(0.0, 0.0),
            text_buffer: String::new(),
            text_renderer: TextRenderer::new(),
            text_preview: None,
            text_caret: 0,
            text_selection: None,
            next_layer_id: 2,
        }
    }

    fn current_color(&self) -> egui::Color32 {
        match self.tool {
            Tool::Brush | Tool::Line | Tool::Rectangle | Tool::Table | Tool::Ellipse => {
                self.brush_color
            }
            Tool::Eraser => egui::Color32::TRANSPARENT,
            Tool::Bucket | Tool::Text => self.brush_color,
        }
    }

    fn push_recent(&mut self, color: egui::Color32) {
        if let Some(pos) = self.recent.iter().position(|c| *c == color) {
            self.recent.remove(pos);
        }
        self.recent.insert(0, color);
        if self.recent.len() > self.max_recent {
            self.recent.truncate(self.max_recent);
        }
    }
}

impl Canvas {
    fn new(background: egui::Color32, size: egui::Vec2, scale: f32) -> Self {
        let (width, height) = canvas_pixel_size(size, scale);
        let mut composite = image::RgbaImage::new(width, height);
        fill_image(&mut composite, background);
        let layers = vec![Layer::new("Layer 1", width, height)];
        let (tiles, tiles_per_row) = build_tiles(composite.width(), composite.height());
        Self {
            composite,
            layers,
            active_layer: 0,
            origin: egui::pos2(0.0, 0.0),
            scale,
            view_offset: egui::Vec2::ZERO,
            viewport_size: size,
            composite_dirty: Some(DirtyRect {
                min_x: 0,
                min_y: 0,
                max_x: width as i32 - 1,
                max_y: height as i32 - 1,
            }),
            tiles,
            tiles_per_row,
        }
    }

    fn update_layout(&mut self, viewport_size: egui::Vec2, viewport_min: egui::Pos2, scale: f32) {
        self.viewport_size = viewport_size;
        self.scale = scale;
        self.clamp_view_offset();
        self.origin = egui::pos2(
            viewport_min.x - self.view_offset.x / self.scale,
            viewport_min.y - self.view_offset.y / self.scale,
        );
    }

    fn set_layers_from_image(&mut self, image: image::RgbaImage, name: String) {
        let width = image.width();
        let height = image.height();
        self.layers = vec![Layer {
            name,
            image,
            visible: true,
        }];
        self.active_layer = 0;
        self.composite = image::RgbaImage::new(width, height);
        let (tiles, tiles_per_row) = build_tiles(width, height);
        self.tiles = tiles;
        self.tiles_per_row = tiles_per_row;
        self.view_offset = egui::Vec2::ZERO;
        self.clamp_view_offset();
        self.mark_composite_dirty_all();
    }

    fn ensure_composite(&mut self, background: egui::Color32) {
        let Some(rect) = self.composite_dirty.take() else {
            return;
        };
        let width = self.composite.width() as i32;
        let height = self.composite.height() as i32;
        let min_x = rect.min_x.clamp(0, width.saturating_sub(1));
        let min_y = rect.min_y.clamp(0, height.saturating_sub(1));
        let max_x = rect.max_x.clamp(0, width.saturating_sub(1));
        let max_y = rect.max_y.clamp(0, height.saturating_sub(1));

        if max_x < min_x || max_y < min_y {
            return;
        }

        let pixel = image::Rgba(color_to_array(background));
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                self.composite.put_pixel(x as u32, y as u32, pixel);
            }
        }
        for layer in &self.layers {
            if !layer.visible {
                continue;
            }
            let layer_width = layer.image.width() as i32;
            let layer_height = layer.image.height() as i32;
            let lx0 = min_x.clamp(0, layer_width.saturating_sub(1));
            let ly0 = min_y.clamp(0, layer_height.saturating_sub(1));
            let lx1 = max_x.clamp(0, layer_width.saturating_sub(1));
            let ly1 = max_y.clamp(0, layer_height.saturating_sub(1));
            if lx1 < lx0 || ly1 < ly0 {
                continue;
            }
            for y in ly0..=ly1 {
                for x in lx0..=lx1 {
                    let src = *layer.image.get_pixel(x as u32, y as u32);
                    let alpha = src.0[3];
                    if alpha == 0 {
                        continue;
                    }
                    blend_pixel_rgba(&mut self.composite, x as u32, y as u32, src, alpha);
                }
            }
        }
    }

    fn scroll_by(&mut self, delta: egui::Vec2) {
        self.view_offset.x = (self.view_offset.x - delta.x * self.scale).max(0.0);
        self.view_offset.y = (self.view_offset.y - delta.y * self.scale).max(0.0);
        self.clamp_view_offset();
    }

    fn clamp_view_offset(&mut self) {
        let max_x =
            (self.composite.width() as f32 - self.viewport_size.x * self.scale).max(0.0);
        let max_y =
            (self.composite.height() as f32 - self.viewport_size.y * self.scale).max(0.0);
        self.view_offset.x = self.view_offset.x.clamp(0.0, max_x);
        self.view_offset.y = self.view_offset.y.clamp(0.0, max_y);
    }

    fn clear(&mut self, _background: egui::Color32) {
        let transparent = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0);
        for layer in &mut self.layers {
            fill_image(&mut layer.image, transparent);
        }
        self.mark_composite_dirty_all();
    }

    fn ensure_tiles(&mut self, ctx: &egui::Context) {
        for tile in &mut self.tiles {
            if !tile.dirty && tile.texture.is_some() {
                continue;
            }
            let color_image = tile_color_image(&self.composite, tile.rect);
            match &mut tile.texture {
                Some(texture) => {
                    texture.set(color_image, egui::TextureOptions::NEAREST);
                    tile.dirty = false;
                }
                None => {
                    tile.texture = Some(ctx.load_texture(
                        &tile.name,
                        color_image,
                        egui::TextureOptions::NEAREST,
                    ));
                    tile.dirty = false;
                }
            }
        }
    }

    fn mark_dirty_rect(&mut self, min: egui::Pos2, max: egui::Pos2) {
        let min_x = min.x.floor().max(0.0) as i32;
        let min_y = min.y.floor().max(0.0) as i32;
        let max_x = max.x.ceil().min(self.composite.width() as f32 - 1.0) as i32;
        let max_y = max.y.ceil().min(self.composite.height() as f32 - 1.0) as i32;

        if max_x < min_x || max_y < min_y {
            return;
        }

        self.mark_composite_dirty(DirtyRect {
            min_x,
            min_y,
            max_x,
            max_y,
        });

        let min_tx = (min_x as u32) / TILE_SIZE;
        let max_tx = (max_x as u32) / TILE_SIZE;
        let min_ty = (min_y as u32) / TILE_SIZE;
        let max_ty = (max_y as u32) / TILE_SIZE;

        for ty in min_ty..=max_ty {
            for tx in min_tx..=max_tx {
                let index = (ty * self.tiles_per_row + tx) as usize;
                if let Some(tile) = self.tiles.get_mut(index) {
                    tile.dirty = true;
                }
            }
        }
    }

    fn mark_composite_dirty(&mut self, rect: DirtyRect) {
        match &mut self.composite_dirty {
            Some(existing) => {
                existing.min_x = existing.min_x.min(rect.min_x);
                existing.min_y = existing.min_y.min(rect.min_y);
                existing.max_x = existing.max_x.max(rect.max_x);
                existing.max_y = existing.max_y.max(rect.max_y);
            }
            None => {
                self.composite_dirty = Some(rect);
            }
        }
    }

    fn mark_composite_dirty_all(&mut self) {
        let width = self.composite.width() as i32;
        let height = self.composite.height() as i32;
        if width == 0 || height == 0 {
            self.composite_dirty = None;
            return;
        }
        self.composite_dirty = Some(DirtyRect {
            min_x: 0,
            min_y: 0,
            max_x: width - 1,
            max_y: height - 1,
        });
        for tile in &mut self.tiles {
            tile.dirty = true;
        }
    }

    fn active_layer_mut(&mut self) -> Option<&mut Layer> {
        self.layers.get_mut(self.active_layer)
    }
}

impl eframe::App for PaintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_pixels_per_point(2.0);
        let pixels_per_point = ctx.pixels_per_point();

        let mut commit_text = false;
        let mut cancel_text = false;
        if self.tool == Tool::Text && self.text_active {
            ctx.input(|i| {
                for event in &i.events {
                    match event {
                        egui::Event::Text(text) => {
                            delete_selection(
                                &mut self.text_buffer,
                                &mut self.text_caret,
                                &mut self.text_selection,
                            );
                            insert_text(&mut self.text_buffer, &mut self.text_caret, text);
                        }
                        egui::Event::Key {
                            key: egui::Key::Backspace,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            if delete_selection(
                                &mut self.text_buffer,
                                &mut self.text_caret,
                                &mut self.text_selection,
                            ) {
                                continue;
                            }
                            if self.text_caret > 0 && !modifiers.command {
                                self.text_caret = self.text_caret.saturating_sub(1);
                                remove_char_at(&mut self.text_buffer, self.text_caret);
                            }
                        }
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            pressed: true,
                            ..
                        } => {
                            commit_text = true;
                        }
                        egui::Event::Key {
                            key: egui::Key::Delete,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            if delete_selection(
                                &mut self.text_buffer,
                                &mut self.text_caret,
                                &mut self.text_selection,
                            ) {
                                continue;
                            }
                            if !modifiers.command {
                                remove_char_at(&mut self.text_buffer, self.text_caret);
                            }
                        }
                        egui::Event::Key {
                            key: egui::Key::ArrowLeft,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            move_caret(
                                &mut self.text_caret,
                                &mut self.text_selection,
                                &self.text_buffer,
                                -1,
                                modifiers.shift,
                            );
                        }
                        egui::Event::Key {
                            key: egui::Key::ArrowRight,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            move_caret(
                                &mut self.text_caret,
                                &mut self.text_selection,
                                &self.text_buffer,
                                1,
                                modifiers.shift,
                            );
                        }
                        egui::Event::Key {
                            key: egui::Key::Home,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            set_caret(
                                &mut self.text_caret,
                                &mut self.text_selection,
                                0,
                                modifiers.shift,
                            );
                        }
                        egui::Event::Key {
                            key: egui::Key::End,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            set_caret(
                                &mut self.text_caret,
                                &mut self.text_selection,
                                self.text_buffer.chars().count(),
                                modifiers.shift,
                            );
                        }
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            pressed: true,
                            ..
                        } => {
                            cancel_text = true;
                        }
                        _ => {}
                    }
                }
            });
        }

        egui::SidePanel::left("tools_panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Tools");
                ui.separator();

                ui.label("Tool");
                ui.radio_value(&mut self.tool, Tool::Brush, "Brush");
                ui.radio_value(&mut self.tool, Tool::Eraser, "Eraser");
                ui.radio_value(&mut self.tool, Tool::Line, "Line");
                ui.radio_value(&mut self.tool, Tool::Rectangle, "Rectangle");
                ui.radio_value(&mut self.tool, Tool::Table, "Table");
                ui.radio_value(&mut self.tool, Tool::Ellipse, "Ellipse");
                ui.radio_value(&mut self.tool, Tool::Bucket, "Bucket");
                ui.radio_value(&mut self.tool, Tool::Text, "Text");

                ui.add_space(8.0);
                ui.label("Color");
                let color_response = egui::color_picker::color_edit_button_srgba(
                    ui,
                    &mut self.brush_color,
                    egui::color_picker::Alpha::Opaque,
                );
                let _ = color_response;

                ui.add_space(8.0);
                ui.label("Palette");
                let mut palette_pick = None;
                ui.horizontal_wrapped(|ui| {
                    for color in &self.palette {
                        let response = ui.add(
                            egui::Button::new("")
                                .fill(*color)
                                .min_size(egui::vec2(24.0, 24.0)),
                        );
                        if response.clicked() {
                            palette_pick = Some(*color);
                        }
                    }
                });
                if let Some(color) = palette_pick {
                    self.brush_color = color;
                    self.push_recent(color);
                }

                ui.add_space(8.0);
                ui.label("Recent");
                let mut recent_pick = None;
                ui.horizontal_wrapped(|ui| {
                    for color in &self.recent {
                        let response = ui.add(
                            egui::Button::new("")
                                .fill(*color)
                                .min_size(egui::vec2(24.0, 24.0)),
                        );
                        if response.clicked() {
                            recent_pick = Some(*color);
                        }
                    }
                });
                if let Some(color) = recent_pick {
                    self.brush_color = color;
                }

                ui.add_space(8.0);
                if self.tool == Tool::Text {
                    ui.label("Text");
                    ui.label("Click on the canvas, then type. Enter commits.");
                    ui.add(egui::Slider::new(&mut self.text_scale, 1..=8).text("Size"));
                }

                ui.add_space(8.0);
                ui.label("Background");
                let background_response = egui::color_picker::color_edit_button_srgba(
                    ui,
                    &mut self.background,
                    egui::color_picker::Alpha::Opaque,
                );
                if background_response.changed() {
                    self.canvas.mark_composite_dirty_all();
                }

                ui.add_space(8.0);
                ui.label("Brush size");
                ui.add(egui::Slider::new(&mut self.brush_size, 1.0..=30.0).suffix(" px"));

                ui.add_space(8.0);
                ui.label("Table");
                ui.add(egui::Slider::new(&mut self.table_rows, 1..=12).text("Rows"));
                ui.add(egui::Slider::new(&mut self.table_cols, 1..=12).text("Cols"));

                ui.add_space(8.0);
                ui.label("Layers");
                let mut new_active = self.canvas.active_layer;
                let mut layers_changed = false;
                let active_layer = self.canvas.active_layer;
                for (index, layer) in self.canvas.layers.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let mut visible = layer.visible;
                        if ui.checkbox(&mut visible, "").clicked() {
                            layer.visible = visible;
                            layers_changed = true;
                        }
                        if ui
                            .selectable_label(index == active_layer, &layer.name)
                            .clicked()
                        {
                            new_active = index;
                        }
                    });
                }
                self.canvas.active_layer = new_active.min(self.canvas.layers.len().saturating_sub(1));
                if layers_changed {
                    self.canvas.mark_composite_dirty_all();
                }

                ui.horizontal(|ui| {
                    if ui.button("Add").clicked() {
                        let name = format!("Layer {}", self.next_layer_id);
                        self.next_layer_id += 1;
                        let width = self.canvas.composite.width();
                        let height = self.canvas.composite.height();
                        self.canvas.layers.push(Layer::new(&name, width, height));
                        self.canvas.active_layer = self.canvas.layers.len() - 1;
                        self.canvas.mark_composite_dirty_all();
                    }

                    let can_delete = self.canvas.layers.len() > 1;
                    if ui
                        .add_enabled(can_delete, egui::Button::new("Delete"))
                        .clicked()
                    {
                        self.canvas.layers.remove(self.canvas.active_layer);
                        if self.canvas.active_layer >= self.canvas.layers.len() {
                            self.canvas.active_layer = self.canvas.layers.len().saturating_sub(1);
                        }
                        self.canvas.mark_composite_dirty_all();
                    }
                });

                ui.horizontal(|ui| {
                    let can_up = self.canvas.active_layer > 0;
                    if ui
                        .add_enabled(can_up, egui::Button::new("Up"))
                        .clicked()
                    {
                        let idx = self.canvas.active_layer;
                        self.canvas.layers.swap(idx, idx - 1);
                        self.canvas.active_layer -= 1;
                        self.canvas.mark_composite_dirty_all();
                    }
                    let can_down = self.canvas.active_layer + 1 < self.canvas.layers.len();
                    if ui
                        .add_enabled(can_down, egui::Button::new("Down"))
                        .clicked()
                    {
                        let idx = self.canvas.active_layer;
                        self.canvas.layers.swap(idx, idx + 1);
                        self.canvas.active_layer += 1;
                        self.canvas.mark_composite_dirty_all();
                    }
                });

                ui.add_space(8.0);
                if ui.button("Clear").clicked() {
                    self.canvas.clear(self.background);
                }

                ui.add_space(8.0);
                if ui.button("Save PNG").clicked() {
                    let dialog = rfd::FileDialog::new()
                        .add_filter("Image", &["png"])
                        .set_file_name("painting.png");
                    if let Some(path) = dialog.save_file() {
                        self.canvas.ensure_composite(self.background);
                        match self.canvas.composite.save(&path) {
                            Ok(()) => self.status = Some(format!("Saved to {}", path.display())),
                            Err(err) => self.status = Some(format!("Save failed: {}", err)),
                        }
                    }
                }

                if ui.button("Open PNG").clicked() {
                    let dialog = rfd::FileDialog::new().add_filter("Image", &["png"]);
                    if let Some(path) = dialog.pick_file() {
                        match image::open(&path) {
                            Ok(img) => {
                                self.canvas.set_layers_from_image(
                                    img.to_rgba8(),
                                    "Image".to_string(),
                                );
                                self.next_layer_id = 2;
                                self.canvas.mark_composite_dirty_all();
                                self.status = Some(format!("Opened {}", path.display()));
                            }
                            Err(err) => {
                                self.status = Some(format!("Open failed: {}", err));
                            }
                        }
                    }
                }

                if let Some(status) = &self.status {
                    ui.add_space(8.0);
                    ui.label(status);
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let (response, painter) = ui.allocate_painter(available, egui::Sense::drag());
            let rect = response.rect;

            self.canvas
                .update_layout(rect.size(), rect.min, pixels_per_point);

            if response.hovered() {
                let scroll_delta = ctx.input(|i| i.raw_scroll_delta);
                if scroll_delta != egui::Vec2::ZERO {
                    self.canvas.scroll_by(scroll_delta);
                    self.canvas
                        .update_layout(rect.size(), rect.min, pixels_per_point);
                }
            }

            self.canvas.ensure_composite(self.background);
            self.canvas.ensure_tiles(ctx);
            for tile in &self.canvas.tiles {
                let Some(texture) = &tile.texture else {
                    continue;
                };
                let tile_min = egui::pos2(
                    self.canvas.origin.x + tile.rect.x as f32 / self.canvas.scale,
                    self.canvas.origin.y + tile.rect.y as f32 / self.canvas.scale,
                );
                let tile_max = egui::pos2(
                    tile_min.x + tile.rect.width as f32 / self.canvas.scale,
                    tile_min.y + tile.rect.height as f32 / self.canvas.scale,
                );
                painter.image(
                    texture.id(),
                    egui::Rect::from_min_max(tile_min, tile_max),
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }

            if cancel_text {
                self.text_active = false;
                self.text_buffer.clear();
                self.text_preview = None;
                self.text_caret = 0;
                self.text_selection = None;
            }

            if commit_text && !self.text_buffer.is_empty() {
                let scale = self.canvas.scale;
                let origin = self.canvas.origin;
                let text_px = 16.0 * self.text_scale as f32 * scale.max(1.0);
                let color = image::Rgba(color_to_array(self.brush_color));
                if let Some(layer) = self.canvas.active_layer_mut() {
                    if let Some((min, max)) = self.text_renderer.draw_text(
                        &mut layer.image,
                        &self.text_buffer,
                        canvas_to_image(self.text_pos, origin, scale),
                        text_px,
                        color,
                    ) {
                        self.canvas.mark_dirty_rect(min, max);
                    }
                }
                self.text_buffer.clear();
                self.text_active = false;
                self.text_preview = None;
                self.text_caret = 0;
                self.text_selection = None;
            }

            if response.drag_started() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if rect.contains(pos) {
                        self.drag_start = Some(pos);
                        self.drag_current = Some(pos);
                        if matches!(
                            self.tool,
                            Tool::Brush
                                | Tool::Line
                                | Tool::Rectangle
                                | Tool::Table
                                | Tool::Ellipse
                                | Tool::Bucket
                                | Tool::Text
                        ) {
                            self.push_recent(self.brush_color);
                        }
                        if matches!(self.tool, Tool::Text) {
                            if self.text_active && !self.text_buffer.is_empty() {
                                let scale = self.canvas.scale;
                                let origin = self.canvas.origin;
                                let text_px = 16.0 * self.text_scale as f32 * scale.max(1.0);
                                let color = image::Rgba(color_to_array(self.brush_color));
                                if let Some(layer) = self.canvas.active_layer_mut() {
                                    if let Some((min, max)) = self.text_renderer.draw_text(
                                        &mut layer.image,
                                        &self.text_buffer,
                                        canvas_to_image(
                                            self.text_pos,
                                            origin,
                                            scale,
                                        ),
                                        text_px,
                                        color,
                                    ) {
                                        self.canvas.mark_dirty_rect(min, max);
                                    }
                                }
                                self.text_buffer.clear();
                            }
                            self.text_active = true;
                            self.text_pos = pos;
                            self.text_caret = 0;
                            self.text_selection = None;
                            self.drag_start = None;
                            self.drag_current = None;
                            self.last_draw_pos = None;
                        } else {
                            if matches!(self.tool, Tool::Brush | Tool::Eraser) {
                                let scale = self.canvas.scale;
                                let origin = self.canvas.origin;
                                let color = image::Rgba(color_to_array(self.current_color()));
                                let canvas_pos = canvas_to_image(pos, origin, scale);
                                let radius = (self.brush_size / 2.0).max(1.0) * scale;
                                if let Some(layer) = self.canvas.active_layer_mut() {
                                    draw_filled_circle(
                                        &mut layer.image,
                                        canvas_pos,
                                        radius,
                                        color,
                                    );
                                    self.canvas.mark_dirty_rect(
                                        egui::pos2(canvas_pos.x - radius, canvas_pos.y - radius),
                                        egui::pos2(canvas_pos.x + radius, canvas_pos.y + radius),
                                    );
                                    self.last_draw_pos = Some(canvas_pos);
                                }
                            }
                            if matches!(self.tool, Tool::Bucket) {
                                let scale = self.canvas.scale;
                                let origin = self.canvas.origin;
                                let canvas_pos = canvas_to_image(pos, origin, scale);
                                let x = canvas_pos.x.round() as i32;
                                let y = canvas_pos.y.round() as i32;
                                let replacement =
                                    image::Rgba(color_to_array(self.current_color()));
                                if let Some(layer) = self.canvas.active_layer_mut() {
                                    if x >= 0
                                        && y >= 0
                                        && x < layer.image.width() as i32
                                        && y < layer.image.height() as i32
                                    {
                                        let target =
                                            *layer.image.get_pixel(x as u32, y as u32);
                                        if target != replacement {
                                            flood_fill(
                                                &mut layer.image,
                                                egui::pos2(x as f32, y as f32),
                                                target,
                                                replacement,
                                            );
                                            self.canvas.mark_composite_dirty_all();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if rect.contains(pos) {
                        self.drag_current = Some(pos);
                        if matches!(self.tool, Tool::Brush | Tool::Eraser) {
                            let scale = self.canvas.scale;
                            let origin = self.canvas.origin;
                            let color = image::Rgba(color_to_array(self.current_color()));
                            let canvas_pos = canvas_to_image(pos, origin, scale);
                            if let Some(prev) = self.last_draw_pos {
                                if let Some(layer) = self.canvas.active_layer_mut() {
                                    draw_brush_segment(
                                        &mut layer.image,
                                        prev,
                                        canvas_pos,
                                        (self.brush_size / 2.0).max(1.0) * scale,
                                        color,
                                    );
                                    let radius = (self.brush_size / 2.0).max(1.0) * scale;
                                    let min_x = prev.x.min(canvas_pos.x) - radius;
                                    let min_y = prev.y.min(canvas_pos.y) - radius;
                                    let max_x = prev.x.max(canvas_pos.x) + radius;
                                    let max_y = prev.y.max(canvas_pos.y) + radius;
                                    self.canvas.mark_dirty_rect(
                                        egui::pos2(min_x, min_y),
                                        egui::pos2(max_x, max_y),
                                    );
                                }
                            }
                            self.last_draw_pos = Some(canvas_pos);
                        }
                    }
                }
            }

            if response.drag_stopped() {
                if let (Some(start), Some(end)) = (self.drag_start, self.drag_current) {
                    let origin = self.canvas.origin;
                    let scale = self.canvas.scale;
                    let color = image::Rgba(color_to_array(self.current_color()));
                    let radius = (self.brush_size / 2.0).max(1.0) * scale;
                    match self.tool {
                        Tool::Line => {
                            if let Some(layer) = self.canvas.active_layer_mut() {
                                draw_line(
                                    &mut layer.image,
                                    canvas_to_image(start, origin, scale),
                                    canvas_to_image(end, origin, scale),
                                    radius,
                                    color,
                                );
                                let start = canvas_to_image(start, origin, scale);
                                let end = canvas_to_image(end, origin, scale);
                                let min_x = start.x.min(end.x) - radius;
                                let min_y = start.y.min(end.y) - radius;
                                let max_x = start.x.max(end.x) + radius;
                                let max_y = start.y.max(end.y) + radius;
                                self.canvas.mark_dirty_rect(
                                    egui::pos2(min_x, min_y),
                                    egui::pos2(max_x, max_y),
                                );
                            }
                        }
                        Tool::Rectangle => {
                            if let Some(layer) = self.canvas.active_layer_mut() {
                                let start = canvas_to_image(start, origin, scale);
                                let end = canvas_to_image(end, origin, scale);
                                let thickness = (self.brush_size * scale).round() as i32;
                                draw_rect_outline(
                                    &mut layer.image,
                                    start,
                                    end,
                                    thickness,
                                    color,
                                );
                                let expand = thickness as f32;
                                let min_x = start.x.min(end.x) - expand;
                                let min_y = start.y.min(end.y) - expand;
                                let max_x = start.x.max(end.x) + expand;
                                let max_y = start.y.max(end.y) + expand;
                                self.canvas.mark_dirty_rect(
                                    egui::pos2(min_x, min_y),
                                    egui::pos2(max_x, max_y),
                                );
                            }
                        }
                        Tool::Table => {
                            if let Some(layer) = self.canvas.active_layer_mut() {
                                let start = canvas_to_image(start, origin, scale);
                                let end = canvas_to_image(end, origin, scale);
                                let thickness = (self.brush_size * scale).round() as i32;
                                draw_table(
                                    &mut layer.image,
                                    start,
                                    end,
                                    self.table_rows.max(1),
                                    self.table_cols.max(1),
                                    thickness,
                                    color,
                                );
                                let expand = thickness as f32;
                                let min_x = start.x.min(end.x) - expand;
                                let min_y = start.y.min(end.y) - expand;
                                let max_x = start.x.max(end.x) + expand;
                                let max_y = start.y.max(end.y) + expand;
                                self.canvas.mark_dirty_rect(
                                    egui::pos2(min_x, min_y),
                                    egui::pos2(max_x, max_y),
                                );
                            }
                        }
                        Tool::Ellipse => {
                            if let Some(layer) = self.canvas.active_layer_mut() {
                                let start = canvas_to_image(start, origin, scale);
                                let end = canvas_to_image(end, origin, scale);
                                let thickness = (self.brush_size * scale).round() as i32;
                                draw_ellipse_outline(
                                    &mut layer.image,
                                    start,
                                    end,
                                    thickness,
                                    color,
                                );
                                let expand = thickness as f32;
                                let min_x = start.x.min(end.x) - expand;
                                let min_y = start.y.min(end.y) - expand;
                                let max_x = start.x.max(end.x) + expand;
                                let max_y = start.y.max(end.y) + expand;
                                self.canvas.mark_dirty_rect(
                                    egui::pos2(min_x, min_y),
                                    egui::pos2(max_x, max_y),
                                );
                            }
                        }
                        _ => {}
                    }
                }
                self.drag_start = None;
                self.drag_current = None;
                self.last_draw_pos = None;
            }

            if let (Some(start), Some(end)) = (self.drag_start, self.drag_current) {
                match self.tool {
                    Tool::Line => {
                        render_line(&painter, start, end, self.brush_size, self.current_color())
                    }
                    Tool::Rectangle => {
                        render_rect_aligned(
                            &painter,
                            start,
                            end,
                            self.brush_size,
                            self.current_color(),
                            self.canvas.origin,
                            self.canvas.scale,
                        )
                    }
                    Tool::Table => render_table(
                        &painter,
                        start,
                        end,
                        self.table_rows.max(1),
                        self.table_cols.max(1),
                        self.brush_size,
                        self.current_color(),
                    ),
                    Tool::Ellipse => {
                        render_ellipse(&painter, start, end, self.brush_size, self.current_color())
                    }
                    _ => {}
                }
            }

            if self.tool == Tool::Text && self.text_active && !self.text_buffer.is_empty() {
                let key = PreviewKey {
                    text: self.text_buffer.clone(),
                    size: self.text_scale,
                    color: self.brush_color,
                    scale: (self.canvas.scale * 100.0).round() as u32,
                };
                let mut needs_update = true;
                if let Some(preview) = &self.text_preview {
                    needs_update = preview.key != key;
                }
                if needs_update {
                    let px_size = 16.0 * self.text_scale as f32 * self.canvas.scale.max(1.0);
                    if let Some(bitmap) = self.text_renderer.rasterize_text(
                        &self.text_buffer,
                        px_size,
                        image::Rgba(color_to_array(self.brush_color)),
                    ) {
                        let size = [
                            bitmap.image.width() as usize,
                            bitmap.image.height() as usize,
                        ];
                        let color_image =
                            egui::ColorImage::from_rgba_unmultiplied(size, bitmap.image.as_raw());
                        let scale = self.canvas.scale.max(1.0);
                        let preview_size = egui::vec2(
                            bitmap.image.width() as f32 / scale,
                            bitmap.image.height() as f32 / scale,
                        );
                        let preview_offset = egui::vec2(
                            bitmap.min.x / scale,
                            bitmap.min.y / scale,
                        );
                        match &mut self.text_preview {
                            Some(preview) => {
                                preview
                                    .texture
                                    .set(color_image, egui::TextureOptions::NEAREST);
                                preview.size = preview_size;
                                preview.offset = preview_offset;
                                preview.key = key;
                            }
                            None => {
                                let texture = ctx.load_texture(
                                    "text_preview",
                                    color_image,
                                    egui::TextureOptions::NEAREST,
                                );
                                self.text_preview = Some(TextPreview {
                                    texture,
                                    size: preview_size,
                                    offset: preview_offset,
                                    key,
                                });
                            }
                        }
                    }
                }

                if let Some(preview) = &self.text_preview {
                    let pos = self.text_pos + preview.offset;
                    let rect = egui::Rect::from_min_size(pos, preview.size);
                    painter.image(
                        preview.texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                    painter.rect_stroke(
                        rect.expand(4.0),
                        0.0,
                        egui::Stroke::new(1.0, egui::Color32::LIGHT_GRAY),
                    );

                    let scale = self.canvas.scale.max(1.0);
                    if let Some(metrics) = self.text_renderer.line_metrics(
                        16.0 * self.text_scale as f32 * scale,
                    ) {
                        let positions = self.text_renderer.text_positions(
                            &self.text_buffer,
                            16.0 * self.text_scale as f32 * scale,
                        );
                        let caret = self.text_caret.min(positions.len().saturating_sub(1));
                        let caret_x = self.text_pos.x + positions[caret] / scale;
                        let ascent = metrics.ascent / scale;
                        let line_height = metrics.new_line_size / scale;
                        let top = self.text_pos.y - ascent;
                        let bottom = top + line_height;

                        if let Some(anchor) = self.text_selection {
                            if anchor != self.text_caret && !positions.is_empty() {
                                let a = anchor.min(positions.len() - 1);
                                let b = self.text_caret.min(positions.len() - 1);
                                let (start, end) = if a <= b { (a, b) } else { (b, a) };
                                let x0 = self.text_pos.x + positions[start] / scale;
                                let x1 = self.text_pos.x + positions[end] / scale;
                                painter.rect_filled(
                                    egui::Rect::from_min_max(
                                        egui::pos2(x0, top),
                                        egui::pos2(x1.max(x0 + 1.0), bottom),
                                    ),
                                    0.0,
                                    egui::Color32::from_rgba_unmultiplied(0, 120, 215, 80),
                                );
                            }
                        }

                        if should_draw_caret(ctx) {
                            painter.line_segment(
                                [egui::pos2(caret_x, top), egui::pos2(caret_x, bottom)],
                                egui::Stroke::new(1.0, egui::Color32::BLACK),
                            );
                        }
                    }
                }
            } else if self.text_preview.is_some() && self.text_buffer.is_empty() {
                self.text_preview = None;
            }

            if self.tool == Tool::Text && self.text_active && self.text_buffer.is_empty() {
                let scale = self.canvas.scale.max(1.0);
                if let Some(metrics) =
                    self.text_renderer.line_metrics(16.0 * self.text_scale as f32 * scale)
                {
                    let ascent = metrics.ascent / scale;
                    let line_height = metrics.new_line_size / scale;
                    let top = self.text_pos.y - ascent;
                    let bottom = top + line_height;
                    let caret_x = self.text_pos.x;
                    if should_draw_caret(ctx) {
                        painter.line_segment(
                            [egui::pos2(caret_x, top), egui::pos2(caret_x, bottom)],
                            egui::Stroke::new(1.0, egui::Color32::BLACK),
                        );
                    }
                    painter.rect_stroke(
                        egui::Rect::from_min_max(
                            egui::pos2(caret_x - 2.0, top),
                            egui::pos2(caret_x + 2.0, bottom),
                        ),
                        0.0,
                        egui::Stroke::new(1.0, egui::Color32::LIGHT_GRAY),
                    );
                }
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(egui::vec2(900.0, 600.0)),
        ..Default::default()
    };

    eframe::run_native(
        "Rust Paint",
        options,
        Box::new(|_cc| Box::new(PaintApp::new())),
    )
}

fn canvas_pixel_size(size: egui::Vec2, scale: f32) -> (u32, u32) {
    let width = (size.x * scale).max(1.0).round() as u32;
    let height = (size.y * scale).max(1.0).round() as u32;
    (width, height)
}

fn canvas_to_image(pos: egui::Pos2, origin: egui::Pos2, scale: f32) -> egui::Pos2 {
    egui::pos2((pos.x - origin.x) * scale, (pos.y - origin.y) * scale)
}

fn image_to_canvas(pos: egui::Pos2, origin: egui::Pos2, scale: f32) -> egui::Pos2 {
    egui::pos2(pos.x / scale + origin.x, pos.y / scale + origin.y)
}

fn color_to_array(color: egui::Color32) -> [u8; 4] {
    [color.r(), color.g(), color.b(), color.a()]
}

fn build_tiles(width: u32, height: u32) -> (Vec<Tile>, u32) {
    let tiles_per_row = (width + TILE_SIZE - 1) / TILE_SIZE;
    let tiles_per_col = (height + TILE_SIZE - 1) / TILE_SIZE;
    let mut tiles = Vec::with_capacity((tiles_per_row * tiles_per_col) as usize);

    for ty in 0..tiles_per_col {
        for tx in 0..tiles_per_row {
            let x = tx * TILE_SIZE;
            let y = ty * TILE_SIZE;
            let width = (TILE_SIZE).min(width - x);
            let height = (TILE_SIZE).min(height - y);
            tiles.push(Tile {
                rect: TileRect {
                    x,
                    y,
                    width,
                    height,
                },
                texture: None,
                dirty: true,
                name: format!("paint_tile_{}_{}", tx, ty),
            });
        }
    }

    (tiles, tiles_per_row)
}

fn tile_color_image(image: &image::RgbaImage, rect: TileRect) -> egui::ColorImage {
    let mut pixels = Vec::with_capacity((rect.width * rect.height * 4) as usize);
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            let pixel = image.get_pixel(x, y).0;
            pixels.extend_from_slice(&pixel);
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([rect.width as usize, rect.height as usize], &pixels)
}

fn fill_image(image: &mut image::RgbaImage, color: egui::Color32) {
    let pixel = image::Rgba(color_to_array(color));
    for y in 0..image.height() {
        for x in 0..image.width() {
            image.put_pixel(x, y, pixel);
        }
    }
}

fn draw_line(
    image: &mut image::RgbaImage,
    start: egui::Pos2,
    end: egui::Pos2,
    radius: f32,
    color: image::Rgba<u8>,
) {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let steps = dx.abs().max(dy.abs()).max(1.0);
    let step_x = dx / steps;
    let step_y = dy / steps;
    let mut x = start.x;
    let mut y = start.y;

    for _ in 0..=steps as i32 {
        draw_filled_circle(image, egui::pos2(x, y), radius, color);
        x += step_x;
        y += step_y;
    }
}

fn draw_brush_segment(
    image: &mut image::RgbaImage,
    start: egui::Pos2,
    end: egui::Pos2,
    radius: f32,
    color: image::Rgba<u8>,
) {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let distance = (dx * dx + dy * dy).sqrt();
    if distance <= 0.5 {
        draw_filled_circle(image, end, radius, color);
        return;
    }

    let step = (radius * 0.5).max(1.0);
    let steps = (distance / step).ceil().max(1.0) as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = start.x + dx * t;
        let y = start.y + dy * t;
        draw_filled_circle(image, egui::pos2(x, y), radius, color);
    }
}

fn draw_filled_circle(
    image: &mut image::RgbaImage,
    center: egui::Pos2,
    radius: f32,
    color: image::Rgba<u8>,
) {
    if radius <= 0.5 {
        let x = center.x.round() as i32;
        let y = center.y.round() as i32;
        if x >= 0 && y >= 0 && x < image.width() as i32 && y < image.height() as i32 {
            image.put_pixel(x as u32, y as u32, color);
        }
        return;
    }

    let radius_sq = radius * radius;
    let min_x = (center.x - radius).floor().max(0.0) as i32;
    let min_y = (center.y - radius).floor().max(0.0) as i32;
    let max_x = (center.x + radius).ceil().min(image.width() as f32 - 1.0) as i32;
    let max_y = (center.y + radius).ceil().min(image.height() as f32 - 1.0) as i32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 - center.x;
            let dy = y as f32 - center.y;
            if dx * dx + dy * dy <= radius_sq {
                image.put_pixel(x as u32, y as u32, color);
            }
        }
    }
}

fn draw_rect_outline(
    image: &mut image::RgbaImage,
    start: egui::Pos2,
    end: egui::Pos2,
    thickness: i32,
    color: image::Rgba<u8>,
) {
    let min_x = start.x.min(end.x).floor() as i32;
    let max_x = start.x.max(end.x).ceil() as i32;
    let min_y = start.y.min(end.y).floor() as i32;
    let max_y = start.y.max(end.y).ceil() as i32;
    let thickness = thickness.max(1);

    let width = image.width() as i32;
    let height = image.height() as i32;
    let x_start = min_x.max(0);
    let x_end = max_x.min(width - 1);
    let y_start = min_y.max(0);
    let y_end = max_y.min(height - 1);

    let top_limit = (min_y + thickness - 1).min(max_y);
    let bottom_limit = (max_y - thickness + 1).max(min_y);
    let left_limit = (min_x + thickness - 1).min(max_x);
    let right_limit = (max_x - thickness + 1).max(min_x);

    for y in y_start..=y_end {
        let on_top = y <= top_limit;
        let on_bottom = y >= bottom_limit;
        for x in x_start..=x_end {
            let on_left = x <= left_limit;
            let on_right = x >= right_limit;
            if on_top || on_bottom || on_left || on_right {
                image.put_pixel(x as u32, y as u32, color);
            }
        }
    }
}

fn draw_ellipse_outline(
    image: &mut image::RgbaImage,
    start: egui::Pos2,
    end: egui::Pos2,
    thickness: i32,
    color: image::Rgba<u8>,
) {
    let min_x = start.x.min(end.x);
    let max_x = start.x.max(end.x);
    let min_y = start.y.min(end.y);
    let max_y = start.y.max(end.y);
    let width = (max_x - min_x).max(1.0);
    let height = (max_y - min_y).max(1.0);
    let center = egui::pos2(min_x + width / 2.0, min_y + height / 2.0);
    let rx = width / 2.0;
    let ry = height / 2.0;
    let thickness = thickness.max(1) as f32;

    let step = 1.0 / (rx.max(ry) * 6.0).max(64.0);
    let mut t = 0.0;
    while t <= std::f32::consts::TAU + step {
        let x = center.x + rx * t.cos();
        let y = center.y + ry * t.sin();
        draw_filled_circle(image, egui::pos2(x, y), thickness / 2.0, color);
        t += step;
    }
}

fn draw_table(
    image: &mut image::RgbaImage,
    start: egui::Pos2,
    end: egui::Pos2,
    rows: u32,
    cols: u32,
    thickness: i32,
    color: image::Rgba<u8>,
) {
    draw_rect_outline(image, start, end, thickness, color);

    let min_x = start.x.min(end.x);
    let max_x = start.x.max(end.x);
    let min_y = start.y.min(end.y);
    let max_y = start.y.max(end.y);
    let rows = rows.max(1);
    let cols = cols.max(1);
    let row_step = (max_y - min_y) / rows as f32;
    let col_step = (max_x - min_x) / cols as f32;

    for row in 1..rows {
        let y = (min_y + row_step * row as f32).round() as i32;
        draw_hline(
            image,
            y,
            min_x.floor() as i32,
            max_x.ceil() as i32,
            thickness,
            color,
        );
    }

    for col in 1..cols {
        let x = (min_x + col_step * col as f32).round() as i32;
        draw_vline(
            image,
            x,
            min_y.floor() as i32,
            max_y.ceil() as i32,
            thickness,
            color,
        );
    }
}

fn draw_hline(
    image: &mut image::RgbaImage,
    y: i32,
    x0: i32,
    x1: i32,
    thickness: i32,
    color: image::Rgba<u8>,
) {
    let (width, height) = (image.width() as i32, image.height() as i32);
    let half_low = (thickness - 1) / 2;
    let half_high = thickness / 2;
    let x_start = x0.min(x1).max(0);
    let x_end = x0.max(x1).min(width - 1);

    for dy in -half_low..=half_high {
        let yy = y + dy;
        if yy < 0 || yy >= height {
            continue;
        }
        for x in x_start..=x_end {
            image.put_pixel(x as u32, yy as u32, color);
        }
    }
}

fn draw_vline(
    image: &mut image::RgbaImage,
    x: i32,
    y0: i32,
    y1: i32,
    thickness: i32,
    color: image::Rgba<u8>,
) {
    let (width, height) = (image.width() as i32, image.height() as i32);
    let half_low = (thickness - 1) / 2;
    let half_high = thickness / 2;
    let y_start = y0.min(y1).max(0);
    let y_end = y0.max(y1).min(height - 1);

    for dx in -half_low..=half_high {
        let xx = x + dx;
        if xx < 0 || xx >= width {
            continue;
        }
        for y in y_start..=y_end {
            image.put_pixel(xx as u32, y as u32, color);
        }
    }
}

fn render_line(
    painter: &egui::Painter,
    start: egui::Pos2,
    end: egui::Pos2,
    width: f32,
    color: egui::Color32,
) {
    painter.line_segment([start, end], egui::Stroke::new(width, color));
    let radius = width / 2.0;
    painter.circle_filled(start, radius, color);
    painter.circle_filled(end, radius, color);
}

fn render_rect_aligned(
    painter: &egui::Painter,
    start: egui::Pos2,
    end: egui::Pos2,
    width: f32,
    color: egui::Color32,
    origin: egui::Pos2,
    scale: f32,
) {
    let start_img = canvas_to_image(start, origin, scale);
    let end_img = canvas_to_image(end, origin, scale);
    let min_x = start_img.x.min(end_img.x).floor();
    let max_x = start_img.x.max(end_img.x).ceil();
    let min_y = start_img.y.min(end_img.y).floor();
    let max_y = start_img.y.max(end_img.y).ceil();
    let min_canvas = image_to_canvas(egui::pos2(min_x, min_y), origin, scale);
    let max_canvas = image_to_canvas(egui::pos2(max_x, max_y), origin, scale);
    let thickness_px = (width * scale).round().max(1.0);
    let stroke_width = thickness_px / scale;
    let inset = stroke_width / 2.0;
    let rect = egui::Rect::from_two_pos(min_canvas, max_canvas)
        .shrink2(egui::vec2(inset, inset));
    painter.rect_stroke(rect, 0.0, egui::Stroke::new(stroke_width, color));
}

fn render_table(
    painter: &egui::Painter,
    start: egui::Pos2,
    end: egui::Pos2,
    rows: u32,
    cols: u32,
    width: f32,
    color: egui::Color32,
) {
    let rect = egui::Rect::from_two_pos(start, end);
    let stroke = egui::Stroke::new(width, color);
    painter.rect_stroke(rect, 0.0, stroke);

    let rows = rows.max(1);
    let cols = cols.max(1);
    let row_step = rect.height() / rows as f32;
    let col_step = rect.width() / cols as f32;

    for row in 1..rows {
        let y = rect.min.y + row_step * row as f32;
        painter.line_segment(
            [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
            stroke,
        );
    }

    for col in 1..cols {
        let x = rect.min.x + col_step * col as f32;
        painter.line_segment(
            [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
            stroke,
        );
    }
}

fn render_ellipse(
    painter: &egui::Painter,
    start: egui::Pos2,
    end: egui::Pos2,
    width: f32,
    color: egui::Color32,
) {
    let rect = egui::Rect::from_two_pos(start, end);
    painter.add(egui::Shape::ellipse_stroke(
        rect.center(),
        rect.size() / 2.0,
        egui::Stroke::new(width, color),
    ));
}

struct TextRenderer {
    font: Option<fontdue::Font>,
}

struct TextBitmap {
    image: image::RgbaImage,
    min: egui::Pos2,
    max: egui::Pos2,
}

struct GlyphRaster {
    metrics: fontdue::Metrics,
    bitmap: Vec<u8>,
    x: i32,
    y_top: i32,
}

impl TextRenderer {
    fn new() -> Self {
        Self { font: None }
    }

    fn ensure_font(&mut self) {
        if self.font.is_some() {
            return;
        }
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let query = fontdb::Query {
            families: &[fontdb::Family::SansSerif],
            ..Default::default()
        };
        let id = db.query(&query).or_else(|| db.faces().next().map(|face| face.id));
        if let Some(id) = id {
            let _ = db.with_face_data(id, |data, index| {
                let settings = fontdue::FontSettings {
                    collection_index: index,
                    ..Default::default()
                };
                if let Ok(font) = fontdue::Font::from_bytes(data, settings) {
                    self.font = Some(font);
                }
            });
        }
    }

    fn draw_text(
        &mut self,
        image: &mut image::RgbaImage,
        text: &str,
        origin: egui::Pos2,
        px_size: f32,
        color: image::Rgba<u8>,
    ) -> Option<(egui::Pos2, egui::Pos2)> {
        let bitmap = self.rasterize_text(text, px_size, color)?;
        let offset_x = origin.x.round() as i32 + bitmap.min.x as i32;
        let offset_y = origin.y.round() as i32 + bitmap.min.y as i32;
        let width = image.width() as i32;
        let height = image.height() as i32;

        for y in 0..bitmap.image.height() as i32 {
            let yy = offset_y + y;
            if yy < 0 || yy >= height {
                continue;
            }
            for x in 0..bitmap.image.width() as i32 {
                let xx = offset_x + x;
                if xx < 0 || xx >= width {
                    continue;
                }
                let pixel = bitmap.image.get_pixel(x as u32, y as u32).0;
                let alpha = pixel[3];
                if alpha == 0 {
                    continue;
                }
                let src = image::Rgba([pixel[0], pixel[1], pixel[2], 255]);
                blend_pixel(image, xx as u32, yy as u32, src, alpha);
            }
        }

        Some((
            egui::pos2(
                origin.x + bitmap.min.x,
                origin.y + bitmap.min.y,
            ),
            egui::pos2(
                origin.x + bitmap.max.x,
                origin.y + bitmap.max.y,
            ),
        ))
    }

    fn rasterize_text(
        &mut self,
        text: &str,
        px_size: f32,
        color: image::Rgba<u8>,
    ) -> Option<TextBitmap> {
        self.ensure_font();
        let font = self.font.as_ref()?;
        let px_size = px_size.max(1.0);
        let line_metrics = font.horizontal_line_metrics(px_size);
        let line_height = line_metrics
            .map(|m| m.new_line_size)
            .unwrap_or(px_size * 1.2)
            .ceil() as i32;

        let mut cursor_x = 0;
        let mut cursor_y = 0;
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;
        let mut glyphs = Vec::new();

        for ch in text.chars() {
            if ch == '\n' {
                cursor_x = 0;
                cursor_y += line_height;
                continue;
            }

            let (metrics, bitmap) = font.rasterize(ch, px_size);
            if metrics.width == 0 || metrics.height == 0 {
                cursor_x += metrics.advance_width.round() as i32;
                continue;
            }
            let glyph_x = cursor_x + metrics.xmin;
            let glyph_y_bottom = cursor_y - metrics.ymin;
            let glyph_y_top = glyph_y_bottom - (metrics.height as i32 - 1);

            min_x = min_x.min(glyph_x);
            min_y = min_y.min(glyph_y_top);
            max_x = max_x.max(glyph_x + metrics.width as i32 - 1);
            max_y = max_y.max(glyph_y_bottom);
            glyphs.push(GlyphRaster {
                metrics,
                bitmap,
                x: glyph_x,
                y_top: glyph_y_top,
            });
            cursor_x += glyphs
                .last()
                .map(|g| g.metrics.advance_width.round() as i32)
                .unwrap_or(0);
        }

        if min_x == i32::MAX {
            return None;
        }

        let width = (max_x - min_x + 1) as u32;
        let height = (max_y - min_y + 1) as u32;
        let mut image = image::RgbaImage::new(width, height);

        for glyph in glyphs {
            for gy in 0..glyph.metrics.height as i32 {
                let row = (gy as usize) * glyph.metrics.width;
                let y = glyph.y_top - min_y + gy;
                for gx in 0..glyph.metrics.width as i32 {
                    let x = glyph.x - min_x + gx;
                    let alpha = glyph.bitmap[row + gx as usize];
                    if alpha == 0 {
                        continue;
                    }
                    blend_pixel_rgba(&mut image, x as u32, y as u32, color, alpha);
                }
            }
        }

        Some(TextBitmap {
            image,
            min: egui::pos2(min_x as f32, min_y as f32),
            max: egui::pos2(max_x as f32, max_y as f32),
        })
    }

    fn line_metrics(&mut self, px_size: f32) -> Option<fontdue::LineMetrics> {
        self.ensure_font();
        self.font
            .as_ref()
            .and_then(|font| font.horizontal_line_metrics(px_size.max(1.0)))
    }

    fn text_positions(&mut self, text: &str, px_size: f32) -> Vec<f32> {
        self.ensure_font();
        let Some(font) = self.font.as_ref() else {
            return vec![0.0];
        };
        let px_size = px_size.max(1.0);
        let mut positions = Vec::with_capacity(text.chars().count() + 1);
        let mut x = 0.0;
        positions.push(x);
        for ch in text.chars() {
            let (metrics, _) = font.rasterize(ch, px_size);
            x += metrics.advance_width;
            positions.push(x);
        }
        positions
    }
}

fn blend_pixel(
    image: &mut image::RgbaImage,
    x: u32,
    y: u32,
    color: image::Rgba<u8>,
    alpha: u8,
) {
    let dst = image.get_pixel_mut(x, y);
    let a = alpha as u32;
    let inv = 255 - a;
    for i in 0..3 {
        let src = color.0[i] as u32;
        let dstc = dst.0[i] as u32;
        dst.0[i] = ((src * a + dstc * inv) / 255) as u8;
    }
    dst.0[3] = 255;
}

fn blend_pixel_rgba(
    image: &mut image::RgbaImage,
    x: u32,
    y: u32,
    color: image::Rgba<u8>,
    alpha: u8,
) {
    let dst = image.get_pixel_mut(x, y);
    let src_a = alpha as f32 / 255.0;
    let dst_a = dst.0[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0 {
        *dst = image::Rgba([0, 0, 0, 0]);
        return;
    }
    for i in 0..3 {
        let src = color.0[i] as f32 / 255.0;
        let dstc = dst.0[i] as f32 / 255.0;
        let out = (src * src_a + dstc * dst_a * (1.0 - src_a)) / out_a;
        dst.0[i] = (out * 255.0).round() as u8;
    }
    dst.0[3] = (out_a * 255.0).round() as u8;
}

fn insert_text(text: &mut String, caret: &mut usize, input: &str) {
    let idx = byte_index(text, *caret);
    text.insert_str(idx, input);
    *caret += input.chars().count();
}

fn remove_char_at(text: &mut String, caret: usize) {
    let len = text.chars().count();
    if caret >= len {
        return;
    }
    let start = byte_index(text, caret);
    let end = byte_index(text, caret + 1);
    text.replace_range(start..end, "");
}

fn delete_selection(text: &mut String, caret: &mut usize, selection: &mut Option<usize>) -> bool {
    let Some(anchor) = *selection else {
        return false;
    };
    if anchor == *caret {
        *selection = None;
        return false;
    }
    let (start, end) = if anchor < *caret {
        (anchor, *caret)
    } else {
        (*caret, anchor)
    };
    let start_b = byte_index(text, start);
    let end_b = byte_index(text, end);
    text.replace_range(start_b..end_b, "");
    *caret = start;
    *selection = None;
    true
}

fn move_caret(
    caret: &mut usize,
    selection: &mut Option<usize>,
    text: &str,
    delta: i32,
    extend: bool,
) {
    let len = text.chars().count();
    let new_pos = if delta < 0 {
        caret.saturating_sub(1)
    } else {
        (*caret + 1).min(len)
    };
    set_caret(caret, selection, new_pos, extend);
}

fn set_caret(caret: &mut usize, selection: &mut Option<usize>, pos: usize, extend: bool) {
    if extend {
        if selection.is_none() {
            *selection = Some(*caret);
        }
    } else {
        *selection = None;
    }
    *caret = pos;
}

fn byte_index(text: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    text.char_indices()
        .nth(char_index)
        .map(|(i, _)| i)
        .unwrap_or_else(|| text.len())
}

fn should_draw_caret(ctx: &egui::Context) -> bool {
    let time = ctx.input(|i| i.time);
    ((time * 2.0) as i64) % 2 == 0
}

fn flood_fill(
    image: &mut image::RgbaImage,
    start: egui::Pos2,
    target: image::Rgba<u8>,
    replacement: image::Rgba<u8>,
) {
    let width = image.width() as i32;
    let height = image.height() as i32;
    let sx = start.x.round() as i32;
    let sy = start.y.round() as i32;
    if sx < 0 || sy < 0 || sx >= width || sy >= height {
        return;
    }

    let mut stack = Vec::new();
    stack.push((sx, sy));

    while let Some((x, y)) = stack.pop() {
        if x < 0 || y < 0 || x >= width || y >= height {
            continue;
        }
        if *image.get_pixel(x as u32, y as u32) != target {
            continue;
        }
        image.put_pixel(x as u32, y as u32, replacement);
        stack.push((x + 1, y));
        stack.push((x - 1, y));
        stack.push((x, y + 1));
        stack.push((x, y - 1));
    }
}
