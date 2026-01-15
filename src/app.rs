use egui::{Color32, Id, LayerId, Order, PopupAnchor, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use crate::data::ProfileData;

pub struct VisualizerApp {
    profile_data: Option<ProfileData>,
    error_msg: Option<String>,

    // state
    cursor_time: f64,
    hover_time: Option<f64>,
    window_size_seconds: f64,

    // playback
    playing: bool,
    playback_speed: f64,

    // cache
    // this isn't working as intended
    function_colors: HashMap<String, Color32>,

    // filters
    show_rx: bool,
    show_tx: bool,

    // timeline state
    timeline_start_time: f64,
    timeline_end_time: f64,
    timeline_pe_scroll: f32,
    timeline_track_height: f32,
}

impl VisualizerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let root_dir = PathBuf::from(".");
        let mut app = Self {
            profile_data: None,
            error_msg: None,
            cursor_time: 0.0,
            hover_time: None,
            window_size_seconds: 0.01,
            playing: false,
            playback_speed: 1.0,
            function_colors: HashMap::new(),
            show_rx: true,
            show_tx: true,
            timeline_start_time: 0.0,
            timeline_end_time: 1.0,
            timeline_pe_scroll: 0.0,
            timeline_track_height: 16.0,
        };

        match ProfileData::load_from_dir(&root_dir) {
            Ok(data) => {
                if !data.events.is_empty() {
                    app.cursor_time = data.min_time;
                }
                let mut colors = HashMap::new();
                for e in &data.events {
                    if !colors.contains_key(&e.raw.function) {
                        colors.insert(e.raw.function.clone(), generate_color(&e.raw.function));
                    }
                }
                app.function_colors = colors;
                app.profile_data = Some(data);
                app.timeline_start_time = app.profile_data.as_ref().unwrap().min_time;
                app.timeline_end_time = app.profile_data.as_ref().unwrap().max_time;
            }
            Err(e) => {
                app.error_msg = Some(format!("failed to load data: {}", e));
            }
        }

        app
    }

    fn ui_bandwidth(&mut self, ui: &mut egui::Ui) {
        let Some(data) = self.profile_data.as_ref() else {
            return;
        };
        let rect = ui.available_rect_before_wrap();
        let center = rect.center();
        let radius = rect.width().min(rect.height()) / 3.0;
        let node_radius = 15.0;

        // viewing around what time
        let is_hovering = self.hover_time.is_some();
        let view_time = self.hover_time.unwrap_or(self.cursor_time);

        ui.vertical_centered(|ui| {
            if is_hovering {
                ui.label(
                    egui::RichText::new(format!("Showing bandwidth at Hover: {:.6}s", view_time))
                        .color(Color32::YELLOW),
                );
            } else {
                ui.label(format!("Showing bandwidth at Cursor: {:.6}s", view_time));
            }
        });

        // range
        let start_time = view_time - self.window_size_seconds / 2.0;
        let end_time = view_time + self.window_size_seconds / 2.0;

        let start_idx = data.events.partition_point(|e| e.raw.time < start_time);

        // aggregation
        // comms[(src, dst)] = bytes
        let mut comms: HashMap<(u32, u32), (u64, u64)> = HashMap::new();

        for i in start_idx..data.events.len() {
            let event = &data.events[i];
            if event.raw.time > end_time {
                break;
            }
            if event.raw.target_pe >= 0 {
                let src = event.source_pe;
                let dst = event.raw.target_pe as u32;
                if src != dst {
                    if self.show_tx && event.raw.bytes_tx > 0 {
                        comms.entry((src, dst)).or_insert((0, 0)).0 += event.raw.bytes_tx;
                    }
                    if self.show_rx && event.raw.bytes_rx > 0 {
                        comms.entry((dst, src)).or_insert((0, 0)).1 += event.raw.bytes_rx;
                    }
                }
            }
        }

        let painter = ui.painter();

        // nodes
        let count = data.pe_count;
        let angle_step = std::f32::consts::TAU / count as f32;

        let get_pos = |pe: u32| -> Pos2 {
            let angle = pe as f32 * angle_step - std::f32::consts::PI / 2.0;
            center + Vec2::new(angle.cos(), angle.sin()) * radius
        };

        // hovered node?
        let mut hovered_pe = None;
        if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
            for i in 0..count {
                let pos = get_pos(i);
                if pos.distance(pointer_pos) <= node_radius * 1.5 {
                    hovered_pe = Some(i);
                    break;
                }
            }
        }

        // interaction stats if hovered: (tx, rx)
        let mut interaction_bytes: HashMap<u32, (u64, u64)> = HashMap::new();
        let mut max_interaction = 0;

        if let Some(h) = hovered_pe {
            for ((src, dst), (tx, rx)) in &comms {
                if *src == h {
                    let e = interaction_bytes.entry(*dst).or_insert((0, 0));
                    e.0 += tx;
                    e.1 += rx;
                    max_interaction = max_interaction.max(e.0 + e.1);
                } else if *dst == h {
                    let e = interaction_bytes.entry(*src).or_insert((0, 0));
                    e.0 += tx;
                    e.1 += rx;
                    max_interaction = max_interaction.max(e.0 + e.1);
                }
            }
        }

        // bandwidth arrows
        for ((src, dst), (tx, rx)) in &comms {
            let p1 = get_pos(*src);
            let p2 = get_pos(*dst);

            let total = *tx + *rx;
            if total == 0 {
                continue;
            }
            let mut is_muted = false;

            if let Some(h) = hovered_pe {
                if *src != h && *dst != h {
                    is_muted = true;
                }
            }

            let width = ((total as f32).max(1.0).ln() / 2.0).clamp(0.5, 8.0);
            let alpha = ((total as f32) / 1000.0).clamp(50.0, 200.0) as u8;

            let r = (255.0 * (*tx as f32 / total as f32)) as u8;
            let b = (255.0 * (*rx as f32 / total as f32)) as u8;
            let g = 0;

            let color = Color32::from_rgba_premultiplied(r, g, b, alpha);

            let color = if is_muted {
                // convert to grayscale and lower alpha
                let gray = (color.r() as f32 * 0.2126
                    + color.g() as f32 * 0.7152
                    + color.b() as f32 * 0.0722) as u8;
                Color32::from_rgba_premultiplied(gray, gray, gray, (alpha as f32 * 0.1) as u8)
            } else {
                color
            };

            let stroke = Stroke::new(width, color);

            // avoid overlaps
            let dir = (p2 - p1).normalized();
            let normal = Vec2::new(-dir.y, dir.x);
            let offset = normal * 6.0; // offset

            // direction for shortening and head
            let start_point = p1 + dir * node_radius + offset;
            let end_point = p2 - dir * node_radius + offset;

            painter.line_segment([start_point, end_point], stroke);

            // head
            let arrow_len = 8.0 + width;
            let arrow_angle = std::f32::consts::PI / 6.0;

            // rotation to target
            let angle_vec = (end_point - start_point).normalized();
            let angle = angle_vec.y.atan2(angle_vec.x);
            let angle1 = angle + std::f32::consts::PI - arrow_angle;
            let angle2 = angle + std::f32::consts::PI + arrow_angle;

            let p_arrow1 = end_point + Vec2::new(angle1.cos(), angle1.sin()) * arrow_len;
            let p_arrow2 = end_point + Vec2::new(angle2.cos(), angle2.sin()) * arrow_len;

            painter.add(egui::Shape::convex_polygon(
                vec![end_point, p_arrow1, p_arrow2],
                color,
                Stroke::NONE,
            ));
        }

        // draw nodes
        for i in 0..count {
            let pos = get_pos(i);

            let mut fill_color = Color32::DARK_GRAY;
            let mut stroke_color = Color32::WHITE;
            let mut stroke_width = 1.0;

            if let Some(h) = hovered_pe {
                if i == h {
                    // hovered node
                    fill_color = Color32::from_rgb(100, 100, 200); // highlight
                    stroke_width = 2.0;
                } else if let Some((tx, rx)) = interaction_bytes.get(&i) {
                    // node interacting with hovered node
                    let total = tx + rx;
                    if total > 0 && max_interaction > 0 {
                        let ratio_tx = *tx as f32 / total as f32;
                        let ratio_rx = *rx as f32 / total as f32;

                        let r_target = (255.0 * ratio_tx) as u8;
                        let b_target = (255.0 * ratio_rx) as u8;

                        let intensity = (total as f32 / max_interaction as f32)
                            .sqrt()
                            .clamp(0.0, 1.0);

                        let base = 50.0;
                        let r = (base + (r_target as f32 - base) * intensity) as u8;
                        let g = (base * (1.0 - intensity)) as u8;
                        let b = (base + (b_target as f32 - base) * intensity) as u8;

                        fill_color = Color32::from_rgb(r, g, b);
                    }
                } else {
                    // irrelevant node
                    fill_color = Color32::from_rgba_premultiplied(50, 50, 50, 50);
                    stroke_color = Color32::from_rgba_premultiplied(200, 200, 200, 50);
                }
            }

            painter.circle_filled(pos, node_radius, fill_color);
            painter.circle_stroke(pos, node_radius, Stroke::new(stroke_width, stroke_color));
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                format!("{}", i),
                egui::FontId::proportional(14.0),
                stroke_color,
            );
        }
    }

    fn ui_timeline(&mut self, ui: &mut egui::Ui) {
        let Some(data) = self.profile_data.as_ref() else {
            return;
        };
        let available_size = ui.available_size();
        let track_height = self.timeline_track_height;
        let ruler_height = 30.0;
        let label_width = 120.0;

        let (response, painter) = ui.allocate_painter(available_size, Sense::click_and_drag());
        let rect = response.rect;

        let timeline_rect =
            Rect::from_min_max(rect.min + Vec2::new(label_width, ruler_height), rect.max);

        if response.hovered() {
            let zoom_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if zoom_delta != 0.0 {
                if ui.input(|i| i.modifiers.shift) {
                    let zoom_factor = ((-zoom_delta / 200.0) as f32).exp();
                    let old_track_height = self.timeline_track_height;
                    self.timeline_track_height =
                        (self.timeline_track_height * zoom_factor).clamp(8.0, 100.0);

                    if let Some(hover_pos) = response.hover_pos() {
                        let y_in_content =
                            (hover_pos.y - timeline_rect.min.y) + self.timeline_pe_scroll;
                        let new_y_in_content =
                            y_in_content * (self.timeline_track_height / old_track_height);
                        self.timeline_pe_scroll =
                            new_y_in_content - (hover_pos.y - timeline_rect.min.y);
                    }
                } else {
                    let zoom_factor = ((-zoom_delta / 200.0) as f64).exp();
                    if let Some(hover_pos) = response.hover_pos() {
                        let ratio =
                            ((hover_pos.x - timeline_rect.min.x) / timeline_rect.width()) as f64;
                        let hover_time = self.timeline_start_time
                            + ratio * (self.timeline_end_time - self.timeline_start_time);

                        self.timeline_start_time =
                            hover_time - (hover_time - self.timeline_start_time) * zoom_factor;
                        self.timeline_end_time =
                            hover_time + (self.timeline_end_time - hover_time) * zoom_factor;

                        if self.timeline_end_time - self.timeline_start_time < 0.000000001 {
                            let center = (self.timeline_start_time + self.timeline_end_time) / 2.0;
                            self.timeline_start_time = center - 0.0000000005;
                            self.timeline_end_time = center + 0.0000000005;
                        }
                    }
                }
            }
        }

        if response.dragged() {
            let delta = response.drag_delta();

            let time_per_pixel =
                (self.timeline_end_time - self.timeline_start_time) / timeline_rect.width() as f64;
            let time_delta = delta.x as f64 * time_per_pixel;
            self.timeline_start_time -= time_delta;
            self.timeline_end_time -= time_delta;

            self.timeline_pe_scroll -= delta.y;
        }

        let duration = self.timeline_end_time - self.timeline_start_time;
        if self.timeline_start_time > data.max_time {
            self.timeline_start_time = data.max_time;
            self.timeline_end_time = self.timeline_start_time + duration;
        }
        if self.timeline_end_time < data.min_time {
            self.timeline_end_time = data.min_time;
            self.timeline_start_time = self.timeline_end_time - duration;
        }

        let total_content_height = data.pe_count as f32 * self.timeline_track_height;
        let max_scroll = (total_content_height - (timeline_rect.height() - track_height)).max(0.0);
        self.timeline_pe_scroll = self.timeline_pe_scroll.clamp(0.0, max_scroll);

        let timeline_start = self.timeline_start_time;
        let timeline_end = self.timeline_end_time;
        let timeline_rect_min_x = timeline_rect.min.x;
        let timeline_rect_width = timeline_rect.width();

        let time_to_x = |t: f64| {
            let ratio = (t - timeline_start) / (timeline_end - timeline_start);
            timeline_rect_min_x + ratio as f32 * timeline_rect_width
        };
        let x_to_time = |x: f32| {
            let ratio = (x - timeline_rect_min_x) / timeline_rect_width;
            timeline_start + ratio as f64 * (timeline_end - timeline_start)
        };

        painter.rect_filled(rect, 0.0, Color32::from_gray(18));

        let data_painter = painter.with_clip_rect(timeline_rect);

        if let Some(h_time) = self.hover_time {
            let h_start = h_time - self.window_size_seconds / 2.0;
            let h_end = h_time + self.window_size_seconds / 2.0;
            let x_start = time_to_x(h_start);
            let x_end = time_to_x(h_end);
            let highlight_rect = Rect::from_min_max(
                Pos2::new(x_start.max(timeline_rect.min.x), timeline_rect.min.y),
                Pos2::new(x_end.min(timeline_rect.max.x), timeline_rect.max.y),
            );
            data_painter.rect_filled(
                highlight_rect,
                0.0,
                Color32::from_rgba_premultiplied(255, 255, 0, 15),
            );
        }

        for i in 0..=data.pe_count {
            let y_in_content = i as f32 * self.timeline_track_height;
            let y = timeline_rect.min.y + y_in_content - self.timeline_pe_scroll;
            if y >= timeline_rect.min.y && y <= timeline_rect.max.y {
                data_painter.line_segment(
                    [
                        Pos2::new(timeline_rect.min.x, y),
                        Pos2::new(timeline_rect.max.x, y),
                    ],
                    Stroke::new(1.0, Color32::from_gray(30)),
                );
            }
        }

        let start_idx = data
            .events
            .partition_point(|e| e.raw.time < self.timeline_start_time - 0.5);
        let mut hovered_event = None;

        for i in start_idx..data.events.len() {
            let e = &data.events[i];
            if e.raw.time > self.timeline_end_time {
                break;
            }

            let x_start = time_to_x(e.raw.time);
            let x_end = time_to_x(e.raw.time + e.raw.duration_sec.max(0.000000001));

            if x_end < timeline_rect.min.x || x_start > timeline_rect.max.x {
                continue;
            }

            let y_start_in_content = e.source_pe as f32 * self.timeline_track_height;
            let y_start = timeline_rect.min.y + y_start_in_content - self.timeline_pe_scroll;
            let y_end = y_start + self.timeline_track_height;

            if y_end < timeline_rect.min.y || y_start > timeline_rect.max.y {
                continue;
            }

            let color = self
                .function_colors
                .get(&e.raw.function)
                .copied()
                .unwrap_or(Color32::GRAY);
            let event_rect = Rect::from_min_max(
                Pos2::new(x_start.max(timeline_rect.min.x), y_start + 1.0),
                Pos2::new(x_end.min(timeline_rect.max.x), y_end - 1.0),
            );

            if event_rect.width() > 2.0 {
                data_painter.rect_filled(event_rect, 1.0, color);
                data_painter.rect_stroke(
                    event_rect,
                    1.0,
                    Stroke::new(0.5, Color32::BLACK.gamma_multiply(0.5)),
                    StrokeKind::Inside,
                );
            } else {
                data_painter.rect_filled(event_rect, 0.0, color);
            }

            if let Some(mouse_pos) = response.hover_pos() {
                if event_rect.contains(mouse_pos) {
                    hovered_event = Some(e);
                }
            }
        }

        let label_area_rect =
            Rect::from_min_max(rect.min, Pos2::new(timeline_rect.min.x, rect.max.y));
        painter.rect_filled(label_area_rect, 0.0, Color32::from_gray(22));

        //painter.line_segment(
        //[
        //Pos2::new(rect.min.x, timeline_rect.min.y),
        //Pos2::new(timeline_rect.min.x, rect.max.y),
        //],
        //Stroke::new(1.0, Color32::from_gray(40)),
        //);

        let labels_painter = painter.with_clip_rect(label_area_rect);
        for i in 0..data.pe_count {
            let y_in_content = i as f32 * self.timeline_track_height;
            let y = timeline_rect.min.y + y_in_content - self.timeline_pe_scroll;
            if y + self.timeline_track_height < timeline_rect.min.y {
                continue;
            }
            if y > timeline_rect.max.y {
                break;
            }

            let hostname = data.pe_hostnames.get(&i).cloned().unwrap_or_default();

            labels_painter.text(
                Pos2::new(rect.min.x + 5.0, y + 2.0),
                egui::Align2::LEFT_TOP,
                format!("PE {}", i),
                egui::FontId::proportional(11.0),
                Color32::from_gray(200),
            );

            labels_painter.text(
                Pos2::new(rect.min.x + 5.0, y + 12.0),
                egui::Align2::LEFT_TOP,
                hostname,
                egui::FontId::proportional(8.0),
                Color32::from_gray(120),
            );
        }

        let ruler_area_rect =
            Rect::from_min_max(rect.min, Pos2::new(rect.max.x, timeline_rect.min.y));
        painter.rect_filled(ruler_area_rect, 0.0, Color32::from_gray(35));

        painter.line_segment(
            [
                Pos2::new(rect.min.x, timeline_rect.min.y),
                Pos2::new(rect.max.x, timeline_rect.min.y),
            ],
            Stroke::new(1.0, Color32::from_gray(60)),
        );

        let ruler_painter = painter.with_clip_rect(ruler_area_rect);
        let time_range = self.timeline_end_time - self.timeline_start_time;
        let ideal_tick_spacing = 100.0f64;
        let time_per_tick = time_range * (ideal_tick_spacing / timeline_rect.width() as f64);
        let log = time_per_tick.log10().floor();
        let base = 10.0f64.powf(log);
        let tick_step = if time_per_tick / base < 2.0 {
            base
        } else if time_per_tick / base < 5.0 {
            base * 2.0
        } else {
            base * 5.0
        };

        let first_tick = (self.timeline_start_time / tick_step).ceil() * tick_step;
        let mut curr_tick = first_tick;
        while curr_tick <= self.timeline_end_time {
            let x = time_to_x(curr_tick);
            ruler_painter.line_segment(
                [
                    Pos2::new(x, ruler_area_rect.min.y),
                    Pos2::new(x, ruler_area_rect.max.y),
                ],
                Stroke::new(1.0, Color32::from_gray(80)),
            );
            ruler_painter.text(
                Pos2::new(x + 2.0, ruler_area_rect.min.y + 2.0),
                egui::Align2::LEFT_TOP,
                format!("{:.4}s", curr_tick),
                egui::FontId::proportional(10.0),
                Color32::LIGHT_GRAY,
            );
            curr_tick += tick_step;
        }

        let px = time_to_x(self.cursor_time);
        if px >= timeline_rect.min.x && px <= timeline_rect.max.x {
            painter.line_segment(
                [Pos2::new(px, rect.min.y), Pos2::new(px, rect.max.y)],
                Stroke::new(1.0, Color32::WHITE),
            );
            let head_size = 6.0;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(px, timeline_rect.min.y),
                    Pos2::new(px - head_size, timeline_rect.min.y - head_size),
                    Pos2::new(px + head_size, timeline_rect.min.y - head_size),
                ],
                Color32::WHITE,
                Stroke::NONE,
            ));
        }

        if let Some(pos) = response.hover_pos() {
            if timeline_rect.contains(pos) {
                self.hover_time = Some(x_to_time(pos.x));
            } else {
                self.hover_time = None;
            }

            if response.clicked() || response.dragged() {
                if ruler_area_rect.contains(pos)
                    || (timeline_rect.contains(pos) && ui.input(|i| i.modifiers.shift))
                {
                    self.cursor_time = x_to_time(pos.x).clamp(data.min_time, data.max_time);
                }
            }
        } else {
            self.hover_time = None;
        }

        if let Some(e) = hovered_event {
            let ctx = ui.ctx().clone();
            egui::Tooltip::always_open(
                ctx,
                LayerId::new(Order::Tooltip, Id::new("hover_tooltip")),
                Id::new("hover_tooltip"),
                PopupAnchor::Pointer,
            )
            .show(|ui: &mut egui::Ui| {
                ui.strong(&e.raw.function);
                if let Some(hostname) = data.pe_hostnames.get(&e.source_pe) {
                    ui.small(format!("PE {} on {hostname}", e.source_pe));
                }
                ui.label(format!("Time: {:.9}s", e.raw.duration_sec));
                let total_bytes = e.raw.bytes_rx + e.raw.bytes_tx;
                if total_bytes > 0 {
                    if e.raw.bytes_rx > 0 && e.raw.bytes_tx > 0 {
                        ui.label(format!(
                            "Data: {} bytes (RX: {}, TX: {})",
                            total_bytes, e.raw.bytes_rx, e.raw.bytes_tx
                        ));
                    } else if e.raw.bytes_rx > 0 {
                        ui.label(format!("Data: {} bytes (RX)", e.raw.bytes_rx));
                    } else {
                        ui.label(format!("Data: {} bytes (TX)", e.raw.bytes_tx));
                    }

                    if e.raw.duration_sec > 0.0 {
                        let bw_gbps = (total_bytes as f64 / e.raw.duration_sec) / 1e9;
                        ui.label(format!("BW: {:.2} GB/s", bw_gbps));
                    }
                }

                if let Some(trace) = &e.raw.symboltrace {
                    if !trace.is_empty() {
                        ui.separator();
                        ui.label(egui::RichText::new("Call Stack:").strong());
                        for line in trace.split('|') {
                            if !line.trim().is_empty() {
                                ui.label(egui::RichText::new(line).small());
                            }
                        }
                    }
                }
            });
        }
    }
}

fn generate_color(s: &str) -> Color32 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    let hash = hasher.finish();

    // kinda a pastel theme
    let r = ((hash >> 16) & 0xFF) as u8;
    let g = ((hash >> 8) & 0xFF) as u8;
    let b = (hash & 0xFF) as u8;

    // help visibility on dark bg
    Color32::from_rgb(
        (r / 2).saturating_add(128),
        (g / 2).saturating_add(128),
        (b / 2).saturating_add(128),
    )
}

impl eframe::App for VisualizerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(err) = &self.error_msg {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Error");
                ui.label(err);
            });
            return;
        }

        let max_time = self
            .profile_data
            .as_ref()
            .map(|d| d.max_time)
            .unwrap_or(0.0);
        let min_time = self
            .profile_data
            .as_ref()
            .map(|d| d.min_time)
            .unwrap_or(0.0);

        if self.playing {
            let dt = ctx.input(|i| i.stable_dt) as f64;
            self.cursor_time += dt * self.playback_speed;
            if self.cursor_time > max_time {
                self.cursor_time = max_time;
                self.playing = false;
            }
            ctx.request_repaint();
        }

        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button(if self.playing { "|| Pause" } else { "|> Play" })
                    .clicked()
                {
                    if !self.playing && self.cursor_time >= max_time - 0.00001 {
                        self.cursor_time = min_time;
                    }
                    self.playing = !self.playing;
                }

                ui.label("Speed:");
                ui.add(
                    egui::Slider::new(&mut self.playback_speed, 0.1..=max_time.max(1.0))
                        .logarithmic(true),
                );

                ui.separator();
                ui.label(format!("Time: {:.6}s", self.cursor_time));
                ui.separator();
                ui.label("Window:");
                let window_max = (max_time - min_time).max(0.0001);
                ui.add(
                    egui::Slider::new(&mut self.window_size_seconds, 0.0001..=window_max)
                        .text("s")
                        .logarithmic(true),
                );

                ui.separator();
                ui.checkbox(&mut self.show_rx, "RX");
                ui.checkbox(&mut self.show_tx, "TX");
            });
        });

        // bottom panel
        egui::TopBottomPanel::bottom("timeline")
            .resizable(true)
            .min_height(200.0)
            .show(ctx, |ui| {
                self.ui_timeline(ui);
            });

        // bandwidth graph
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.profile_data.is_some() {
                self.ui_bandwidth(ui);
            } else {
                ui.label("No data loaded.");
            }
        });
    }
}
