use egui::{Color32, PopupAnchor, Pos2, Stroke, Vec2, Vec2b};
use egui_plot::{Plot, Polygon, VLine};
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
    first_frame: bool,

    // playback
    playing: bool,
    playback_speed: f64,

    // cache
    // this isn't working as intended
    function_colors: HashMap<String, Color32>,
}

impl VisualizerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let root_dir = PathBuf::from("..");
        let mut app = Self {
            profile_data: None,
            error_msg: None,
            cursor_time: 0.0,
            hover_time: None,
            window_size_seconds: 0.01,
            first_frame: true,
            playing: false,
            playback_speed: 1.0,
            function_colors: HashMap::new(),
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
            }
            Err(e) => {
                app.error_msg = Some(format!("failed to load data: {}", e));
            }
        }

        app
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

        let Some(data) = &self.profile_data else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("No data loaded.");
            });
            return;
        };

        // Handle Playback
        if self.playing {
            let dt = ctx.input(|i| i.stable_dt) as f64;
            self.cursor_time += dt * self.playback_speed;
            if self.cursor_time > data.max_time {
                self.cursor_time = data.max_time;
                self.playing = false;
            }
            ctx.request_repaint();
        }

        // Top Panel for Controls
        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button(if self.playing {
                        "⏸ Pause"
                    } else {
                        "▶ Play"
                    })
                    .clicked()
                {
                    if !self.playing && self.cursor_time >= data.max_time - 0.00001 {
                        self.cursor_time = data.min_time;
                    }
                    self.playing = !self.playing;
                }

                ui.label("Speed:");
                ui.add(egui::Slider::new(&mut self.playback_speed, 0.1..=10.0).logarithmic(true));

                ui.separator();
                ui.label(format!("Time: {:.6}s", self.cursor_time));
                ui.separator();
                ui.label("Window:");
                ui.add(
                    egui::Slider::new(&mut self.window_size_seconds, 0.0001..=0.1)
                        .text("s")
                        .logarithmic(true),
                );
            });
        });

        // Bottom Panel for Timeline
        egui::TopBottomPanel::bottom("timeline")
            .resizable(true)
            .min_height(200.0)
            .show(ctx, |ui| {
                let plot = Plot::new("timeline_plot")
                    .allow_zoom([true, false]) // Horizontal zoom only
                    .allow_drag([true, true])
                    .link_cursor("shared_cursor", Vec2b::new(true, false));

                plot.show(ui, |plot_ui| {
                    // adjust to show entire trace
                    if self.first_frame {
                        let min_x = data.min_time;
                        let max_x = data.max_time;
                        let min_y = -1.0;
                        let max_y = (data.pe_count as f64) + 1.0;
                        plot_ui.set_plot_bounds(egui_plot::PlotBounds::from_min_max(
                            [min_x, min_y],
                            [max_x, max_y],
                        ));
                        self.first_frame = false;
                    }

                    // clamp to data bounds
                    let bounds = plot_ui.plot_bounds();
                    let min_x_current = *bounds.range_x().start();
                    let max_x_current = *bounds.range_x().end();
                    let min_y_current = *bounds.range_y().start();

                    // prevent scaling vertically
                    const PIXELS_PER_TRACK: f64 = 30.0;
                    let panel_height = plot_ui.response().rect.height() as f64;
                    let desired_y_range = panel_height / PIXELS_PER_TRACK;

                    // clamp y to data bounds
                    let mut min_y = min_y_current;
                    let mut max_y = min_y + desired_y_range;

                    // and x
                    let mut min_x = min_x_current;
                    let mut max_x = max_x_current;
                    let x_width = max_x - min_x;

                    let data_min_x = data.min_time;
                    let data_max_x = data.max_time;
                    let data_min_y = -1.0;
                    let data_max_y = data.pe_count as f64 + 1.0;

                    let bounds_adjustment_needed = true; // todo: conditionally

                    if min_x < data_min_x {
                        min_x = data_min_x;
                        max_x = min_x + x_width;
                    }
                    if max_x > data_max_x {
                        max_x = data_max_x;
                        min_x = max_x - x_width;
                        if min_x < data_min_x {
                            min_x = data_min_x;
                        }
                    }

                    // clamp y
                    if min_y < data_min_y {
                        min_y = data_min_y;
                        max_y = min_y + desired_y_range;
                    }
                    if max_y > data_max_y {
                        max_y = data_max_y;
                        min_y = max_y - desired_y_range;
                        if min_y < data_min_y {
                            min_y = data_min_y;
                            max_y = min_y + desired_y_range;
                        }
                    }

                    if bounds_adjustment_needed {
                        plot_ui.set_plot_bounds(egui_plot::PlotBounds::from_min_max(
                            [min_x, min_y],
                            [max_x, max_y],
                        ));
                    }

                    // playhead
                    plot_ui.vline(
                        VLine::new(format!("{}", self.cursor_time), self.cursor_time)
                            .stroke(Stroke::new(1.0, Color32::WHITE)),
                    );

                    // highlight integration area
                    if let Some(h_time) = self.hover_time {
                        let h_start = h_time - self.window_size_seconds / 2.0;
                        let h_end = h_time + self.window_size_seconds / 2.0;
                        let points = vec![
                            [h_start, min_y],
                            [h_end, min_y],
                            [h_end, max_y],
                            [h_start, max_y],
                        ];
                        plot_ui.polygon(
                            Polygon::new("", egui_plot::PlotPoints::new(points))
                                .fill_color(Color32::from_rgba_premultiplied(255, 255, 0, 15)),
                        );
                    }

                    let mouse_pos = if plot_ui.response().hovered() {
                        plot_ui.pointer_coordinate()
                    } else {
                        None
                    };

                    // update hover time
                    if let Some(pos) = mouse_pos {
                        if pos.x >= data.min_time
                            && pos.x <= data.max_time
                            && pos.y >= -1.0
                            && pos.y <= data.pe_count as f64
                        {
                            self.hover_time = Some(pos.x);
                        } else {
                            self.hover_time = None;
                        }
                    } else {
                        self.hover_time = None;
                    }

                    // binary search so we can handle larger sets
                    // might be unnecessary but it doesnt seem to hurt
                    // and is a nice one-liner
                    let start_idx = data.events.partition_point(|e| e.raw.time < (min_x - 1.0));

                    let track_height = 0.8;

                    // draw function events
                    for i in start_idx..data.events.len() {
                        let e = &data.events[i];
                        if e.raw.time > max_x {
                            break;
                        }

                        // cull non-y visible events
                        if e.source_pe as f64 + track_height < min_y
                            || e.source_pe as f64 - track_height > max_y
                        {
                            continue;
                        }

                        let y_center = e.source_pe as f64;
                        let y_min = y_center - track_height / 2.0;
                        let y_max = y_center + track_height / 2.0;

                        let t_start = e.raw.time;
                        let t_end = t_start + e.raw.duration_sec.max(0.0000001); // todo: maybe unnecessary?

                        let color = self
                            .function_colors
                            .get(&e.raw.function)
                            .copied()
                            .unwrap_or(Color32::GRAY);

                        let points = vec![
                            [t_start, y_min],
                            [t_end, y_min],
                            [t_end, y_max],
                            [t_start, y_max],
                        ];

                        plot_ui.polygon(
                            Polygon::new(&e.raw.function, egui_plot::PlotPoints::new(points))
                                .fill_color(color),
                        );

                        // tooltip
                        if let Some(pos) = mouse_pos {
                            if pos.x >= t_start
                                && pos.x <= t_end
                                && pos.y >= y_min
                                && pos.y <= y_max
                            {
                                egui::Tooltip::always_open(
                                    ctx.clone(), // what
                                    egui::LayerId::new(
                                        egui::Order::Tooltip,
                                        egui::Id::new("hover_tooltip"),
                                    ),
                                    egui::Id::new("hover_tooltip"),
                                    PopupAnchor::Pointer,
                                )
                                .show(|ui: &mut egui::Ui| {
                                    ui.strong(&e.raw.function);
                                    ui.label(format!("Time: {:.9}s", e.raw.duration_sec));
                                    if e.raw.size_bytes > 0 {
                                        ui.label(format!("Data: {} bytes", e.raw.size_bytes));
                                        if e.raw.duration_sec > 0.0 {
                                            let bw_gbps = (e.raw.size_bytes as f64
                                                / e.raw.duration_sec)
                                                / 1e9;
                                            ui.label(format!("BW: {:.2} GB/s", bw_gbps));
                                        }
                                    }
                                });
                            }
                        }
                    }

                    // scrubbing
                    if plot_ui.response().clicked() || plot_ui.response().dragged() {
                        if let Some(pos) = plot_ui.pointer_coordinate() {
                            self.cursor_time = pos.x.clamp(data.min_time, data.max_time);
                        }
                    }
                });
            });

        // bandwidth graph
        egui::CentralPanel::default().show(ctx, |ui| {
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
                        egui::RichText::new(format!(
                            "Showing bandwidth at Hover: {:.6}s",
                            view_time
                        ))
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
            let mut comms: HashMap<(u32, u32), u64> = HashMap::new();

            for i in start_idx..data.events.len() {
                let event = &data.events[i];
                if event.raw.time > end_time {
                    break;
                }
                if event.raw.target_pe >= 0 {
                    let src = event.source_pe;
                    let dst = event.raw.target_pe as u32;
                    if src != dst {
                        *comms.entry((src, dst)).or_insert(0) += event.raw.size_bytes;
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

            // interaction stats if hovered
            let mut interaction_bytes: HashMap<u32, u64> = HashMap::new();
            let mut max_interaction = 0;

            if let Some(h) = hovered_pe {
                for ((src, dst), bytes) in &comms {
                    if *src == h {
                        let b = *interaction_bytes.get(dst).unwrap_or(&0) + bytes;
                        interaction_bytes.insert(*dst, b);
                        if b > max_interaction {
                            max_interaction = b;
                        }
                    } else if *dst == h {
                        let b = *interaction_bytes.get(src).unwrap_or(&0) + bytes;
                        interaction_bytes.insert(*src, b);
                        if b > max_interaction {
                            max_interaction = b;
                        }
                    }
                }
            }

            // bandwidth arrows
            for ((src, dst), bytes) in &comms {
                let p1 = get_pos(*src);
                let p2 = get_pos(*dst);

                let scaled_bytes = *bytes;
                let mut is_muted = false;

                if let Some(h) = hovered_pe {
                    if *src != h && *dst != h {
                        is_muted = true;
                    }
                }

                let width = ((scaled_bytes as f32).max(1.0).ln() / 2.0).clamp(0.5, 8.0);
                let mut alpha = ((scaled_bytes as f32) / 1000.0).clamp(50.0, 200.0) as u8;

                // this does not work as intended
                if is_muted {
                    alpha = (alpha as f32 * 0.1) as u8;
                }

                let color = Color32::from_rgba_premultiplied(200, 100, 100, alpha);

                // but this does
                let color = if is_muted {
                    // convert to grayscale
                    let gray = (color.r() as f32 * 0.2126
                        + color.g() as f32 * 0.7152
                        + color.b() as f32 * 0.0722) as u8;
                    Color32::from_rgba_premultiplied(gray, gray, gray, alpha)
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
                    } else if let Some(bytes) = interaction_bytes.get(&i) {
                        // node interacting with hovered node
                        if max_interaction > 0 {
                            // todo: this looks ugly
                            let ratio = *bytes as f32 / max_interaction as f32;
                            let r = (50.0 + (255.0 - 50.0) * ratio) as u8;
                            let g = (50.0 + (100.0 - 50.0) * ratio) as u8;
                            let b = (50.0 + (0.0 - 50.0) * ratio) as u8;
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
        });
    }
}
