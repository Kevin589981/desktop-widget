#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::{
    fs,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use eframe::{
    egui::{self, style::Visuals, FontData, FontDefinitions, FontFamily, Frame, Sense},
    epaint::{Color32, ColorImage,  TextureHandle, Vec2},
};
use image::{io::Reader as ImageReader, DynamicImage};
use rand::{seq::SliceRandom, thread_rng};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIconBuilder,
    TrayIconEvent, // <-- 新增: 引入图标事件
    ClickType,     // <-- 新增: 引入点击类型
};
use walkdir::WalkDir;

const CONFIG_FILE: &str = "photo_widget_config.json";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
enum ResizeAnchor {
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
enum TimeUnit {
    Seconds,
    Minutes,
    Hours,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
enum ImageOrientationFilter {
    Both,
    Landscape,
    Portrait,
}

#[derive(Serialize, Deserialize, Clone)]
struct AppConfig {
    folders: Vec<PathBuf>,
    always_on_top: bool,
    refresh_interval: u64,
    refresh_value: u64,
    refresh_unit: TimeUnit,
    landscape_width: f32,
    landscape_height: f32,
    portrait_width: f32,
    portrait_height: f32,
    fit_mode: FitMode,
    resize_anchor: ResizeAnchor,
    orientation_filter: ImageOrientationFilter,
    // --- 新增字段：用于存储窗口位置 ---
    // serde(default) 确保在旧的配置文件中没有此字段时程序不会崩溃
    #[serde(default)]
    window_pos: Option<(f32, f32)>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            folders: vec![],
            always_on_top: false,
            refresh_interval: 300,
            refresh_value: 5,
            refresh_unit: TimeUnit::Minutes,
            landscape_width: 400.0,
            landscape_height: 300.0,
            portrait_width: 300.0,
            portrait_height: 400.0,
            fit_mode: FitMode::Cover,
            resize_anchor: ResizeAnchor::Center,
            orientation_filter: ImageOrientationFilter::Both,
            // --- 初始化新增的字段 ---
            window_pos: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
enum FitMode {
    Cover,
    Contain,
}

struct PhotoWidget {
    config: AppConfig,
    image_files: Vec<PathBuf>,
    current_image_index: usize,
    current_image: Option<TextureHandle>,
    last_update: Instant,
    show_settings: bool,
    image_rx: Receiver<Option<DynamicImage>>,
    image_tx: Sender<Option<DynamicImage>>,
    current_image_path: Option<PathBuf>,
    tray_rx: Receiver<TrayMessage>,
    folder_rx: Receiver<PathBuf>,
    folder_tx: Sender<PathBuf>,
    last_window_size: Option<Vec2>,
    show_drag_bar: bool,
    hover_leave_time: Option<Instant>,
    last_screen_check: Instant,
}

impl PhotoWidget {
    // --- 修改：让 new 函数接收一个已加载的 config ---
    fn new(_cc: &eframe::CreationContext<'_>, tray_rx: Receiver<TrayMessage>, config: AppConfig) -> Self {
        // --- 移除：不再在此处加载配置 ---
        // let mut config: AppConfig = load_config().unwrap_or_default();
        let mut config = config;
        let interval = config.refresh_interval;
        if interval > 0 {
            if interval % 3600 == 0 { config.refresh_unit = TimeUnit::Hours; config.refresh_value = interval / 3600; }
            else if interval % 60 == 0 { config.refresh_unit = TimeUnit::Minutes; config.refresh_value = interval / 60; }
            else { config.refresh_unit = TimeUnit::Seconds; config.refresh_value = interval; }
        }
        let (image_tx, image_rx) = mpsc::channel();
        let (folder_tx, folder_rx) = mpsc::channel();

        let mut app = Self {
            config,
            image_files: Vec::new(),
            current_image_index: 0,
            current_image: None,
            last_update: Instant::now(),
            show_settings: false,
            image_tx,
            image_rx,
            current_image_path: None,
            tray_rx,
            folder_rx,
            folder_tx,
            last_window_size: None,
            show_drag_bar: false,
            hover_leave_time: None,
            last_screen_check: Instant::now(),
        };

        app.scan_image_files();
        app.load_random_image();
        app
    }

    fn scan_image_files(&mut self) {
        self.image_files.clear();
        for folder in &self.config.folders {
            for entry in WalkDir::new(folder).into_iter().filter_map(Result::ok).filter(|e|{let path=e.path();path.is_file()&&path.extension().map_or(false,|s|{let s=s.to_string_lossy().to_lowercase();s=="jpg"||s=="jpeg"||s=="png"||s=="gif"||s=="bmp"})}) {
                let path = entry.path();
                if let Ok((width, height)) = image::image_dimensions(path) {
                    let is_landscape = width >= height;
                    let is_portrait = height > width;
                    let should_add = match self.config.orientation_filter {
                        ImageOrientationFilter::Both => true,
                        ImageOrientationFilter::Landscape => is_landscape,
                        ImageOrientationFilter::Portrait => is_portrait,
                    };
                    if should_add { self.image_files.push(path.to_path_buf()); }
                }
            }
        }
        self.image_files.shuffle(&mut thread_rng());
        self.current_image_index = 0;
    }

    fn load_random_image(&mut self) {
        if self.image_files.is_empty() {
            return;
        }

        if self.current_image_index >= self.image_files.len() {
            self.image_files.shuffle(&mut thread_rng());
            self.current_image_index = 0;
        }

        if let Some(path) = self.image_files.get(self.current_image_index).cloned() {
            self.current_image_path = Some(path.clone());
            let image_tx = self.image_tx.clone();
            
            thread::spawn(move || {
                if let Ok(reader) = ImageReader::open(&path) {
                    if let Ok(image) = reader.with_guessed_format() {
                        if let Ok(decoded_image) = image.decode() {
                            let _ = image_tx.send(Some(decoded_image));
                        }
                    }
                }
            });
            
            self.current_image_index += 1;
        }
    }
}

impl eframe::App for PhotoWidget {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.last_screen_check.elapsed() > Duration::from_secs(1) {
            if let (Some(window_pos), Some(screen_size)) = (
                frame.info().window_info.position,
                frame.info().window_info.monitor_size,
            ) {
                let window_size = frame.info().window_info.size;
                let mut new_pos = window_pos;
                let mut changed = false;

                if window_pos.x < 0.0 { new_pos.x = 0.0; changed = true; }
                if window_pos.y < 0.0 { new_pos.y = 0.0; changed = true; }
                if window_pos.x + window_size.x > screen_size.x { new_pos.x = screen_size.x - window_size.x; changed = true; }
                if window_pos.y + window_size.y > screen_size.y { new_pos.y = screen_size.y - window_size.y; changed = true; }
                
                if changed { frame.set_window_pos(new_pos); }
            }
            self.last_screen_check = Instant::now();
        }

        if let Ok(msg) = self.tray_rx.try_recv() {
            match msg {
                TrayMessage::ShowSettings => { self.show_settings = true; frame.set_decorations(true);frame.focus();  }
                TrayMessage::FocusWindow => {
                    frame.focus();
                }   
                TrayMessage::Quit => { frame.close(); }
            }
        }
        if let Ok(folder) = self.folder_rx.try_recv() { if !self.config.folders.contains(&folder) { self.config.folders.push(folder); self.scan_image_files(); self.load_random_image(); } }
        if let Ok(Some(image)) = self.image_rx.try_recv() {
            let size = [image.width() as _, image.height() as _];
            let image_buffer = image.to_rgba8();
            let pixels = image_buffer.as_flat_samples();
            let color_image = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
            self.current_image = Some(ctx.load_texture(self.current_image_path.as_ref().unwrap().to_string_lossy(), color_image, Default::default()));
        }
        if self.config.refresh_interval > 0 && self.last_update.elapsed().as_secs() >= self.config.refresh_interval && !self.show_settings {
            self.load_random_image();
            self.last_update = Instant::now();
        }
        let new_size = if self.show_settings { Vec2::new(500.0, 600.0) } else {
            if let Some(texture) = &self.current_image {
                let texture_size = texture.size_vec2();
                let image_aspect = texture_size.x / texture_size.y;

                let (target_width_preset, target_height_preset) = 
                    if texture_size.x >= texture_size.y {
                        (self.config.landscape_width, self.config.landscape_height)
                    } else {
                        (self.config.portrait_width, self.config.portrait_height)
                    };
                
                let target_aspect = if target_height_preset > 0.0 { target_width_preset / target_height_preset } else { image_aspect };

                match self.config.fit_mode {
                    FitMode::Cover => { 
                        if texture_size.x >= texture_size.y {
                            Vec2::new(self.config.landscape_width, self.config.landscape_height)
                        } else {
                            Vec2::new(self.config.portrait_width, self.config.portrait_height)
                        }
                    }
                    FitMode::Contain => {
                        let (new_width, new_height) = if image_aspect > target_aspect {
                            (target_height_preset * image_aspect, target_height_preset)
                        } else {
                            (target_width_preset, target_width_preset / image_aspect)
                        };
                        Vec2::new(new_width, new_height)
                    }
                }
            } else { Vec2::new(self.config.landscape_width, self.config.landscape_height) }
        };

        if let Some(old_size) = self.last_window_size {
            if old_size != new_size {
                if let Some(current_pos) = frame.info().window_info.position {
                    let delta = new_size - old_size;
                    let new_window_pos = match self.config.resize_anchor {
                        ResizeAnchor::Center => current_pos - delta / 2.0,
                        ResizeAnchor::TopLeft => current_pos,
                        ResizeAnchor::TopRight => current_pos - egui::vec2(delta.x, 0.0),
                        ResizeAnchor::BottomLeft => current_pos - egui::vec2(0.0, delta.y),
                        ResizeAnchor::BottomRight => current_pos - delta,
                    };
                    frame.set_window_pos(new_window_pos);
                }
            }
        }
        frame.set_window_size(new_size);
        self.last_window_size = Some(new_size);
        frame.set_always_on_top(self.config.always_on_top);

        if self.show_settings {
            self.show_drag_bar = false;
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Settings"); ui.separator();
                if ui.button("Add Folder").clicked() { let folder_tx = self.folder_tx.clone(); thread::spawn(move || { if let Some(folder) = FileDialog::new().pick_folder() { let _ = folder_tx.send(folder); } }); }
                ui.label("Image Folders:");
                let mut folder_to_remove = None;
                for (i, folder) in self.config.folders.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.button("Remove").clicked() { folder_to_remove = Some(i); }
                        ui.label(folder.to_string_lossy());
                    });
                }
                if let Some(i) = folder_to_remove { self.config.folders.remove(i); self.scan_image_files(); }
                ui.separator();
                ui.checkbox(&mut self.config.always_on_top, "Always on Top"); ui.separator();
                let old_filter = self.config.orientation_filter;
                ui.label("Image Orientation:");
                ui.horizontal(|ui| { ui.radio_value(&mut self.config.orientation_filter, ImageOrientationFilter::Both, "Both"); ui.radio_value(&mut self.config.orientation_filter, ImageOrientationFilter::Landscape, "Landscape"); ui.radio_value(&mut self.config.orientation_filter, ImageOrientationFilter::Portrait, "Portrait"); });
                if self.config.orientation_filter != old_filter { self.scan_image_files(); self.load_random_image(); }
                ui.separator();
                ui.label("Refresh Interval (0 to disable):");
                ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut self.config.refresh_value).speed(1.0).clamp_range(0..=u64::MAX)); ui.radio_value(&mut self.config.refresh_unit, TimeUnit::Seconds, "Seconds"); ui.radio_value(&mut self.config.refresh_unit, TimeUnit::Minutes, "Minutes"); ui.radio_value(&mut self.config.refresh_unit, TimeUnit::Hours, "Hours"); });
                ui.separator();
                ui.label("Landscape Base Dimensions:");
                ui.add(egui::Slider::new(&mut self.config.landscape_width, 200.0..=1000.0).text("Width"));
                ui.add(egui::Slider::new(&mut self.config.landscape_height, 200.0..=1000.0).text("Height (Cover only)"));
                ui.separator();
                ui.label("Portrait Base Dimensions:");
                ui.add(egui::Slider::new(&mut self.config.portrait_width, 200.0..=1000.0).text("Width"));
                ui.add(egui::Slider::new(&mut self.config.portrait_height, 200.0..=1000.0).text("Height (Cover only)"));
                ui.separator();
                ui.label("Image Fit Mode:");
                ui.horizontal(|ui| { ui.radio_value(&mut self.config.fit_mode, FitMode::Cover, "Cover (Fill and Crop)"); ui.radio_value(&mut self.config.fit_mode, FitMode::Contain, "Contain (Fit and Resize Window)"); });
                ui.separator();
                ui.label("Resize Anchor Point:");
                ui.horizontal(|ui| { ui.radio_value(&mut self.config.resize_anchor, ResizeAnchor::Center, "Keep Center"); ui.radio_value(&mut self.config.resize_anchor, ResizeAnchor::TopLeft, "Keep Top-Left"); });
                ui.horizontal(|ui| { ui.radio_value(&mut self.config.resize_anchor, ResizeAnchor::TopRight, "Keep Top-Right"); ui.radio_value(&mut self.config.resize_anchor, ResizeAnchor::BottomLeft, "Keep Bottom-Left"); });
                ui.horizontal(|ui| { ui.radio_value(&mut self.config.resize_anchor, ResizeAnchor::BottomRight, "Keep Bottom-Right"); });
                ui.separator();

                if ui.button("Save and Close").clicked() {
                    let multiplier = match self.config.refresh_unit { TimeUnit::Seconds => 1, TimeUnit::Minutes => 60, TimeUnit::Hours => 3600, };
                    self.config.refresh_interval = self.config.refresh_value * multiplier;

                    // --- 新增：在保存设置时，同时保存当前窗口的位置 ---
                    if let Some(pos) = frame.info().window_info.position {
                        self.config.window_pos = Some((pos.x, pos.y));
                    }

                    save_config(&self.config);
                    self.show_settings = false;
                    frame.set_decorations(false);
                    self.scan_image_files();
                    self.load_random_image();
                    self.last_update = Instant::now();
                }
            });
        } else {
            egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
                if let Some(texture) = &self.current_image {
                    let available_size = ui.available_size();
                    let (uv, size) = match self.config.fit_mode {
                        FitMode::Cover => { let texture_size=texture.size_vec2(); let aspect_ratio=texture_size.x/texture_size.y; let available_aspect_ratio=available_size.x/available_size.y; let uv_rect=if aspect_ratio > available_aspect_ratio { let uv_width=available_aspect_ratio/aspect_ratio; let uv_x=(1.0-uv_width)/2.0; egui::Rect::from_min_max(egui::pos2(uv_x,0.0),egui::pos2(uv_x+uv_width,1.0)) } else { let uv_height=aspect_ratio/available_aspect_ratio; let uv_y=(1.0-uv_height)/2.0; egui::Rect::from_min_max(egui::pos2(0.0,uv_y),egui::pos2(1.0,uv_y+uv_height)) }; (uv_rect, available_size) }
                        FitMode::Contain => (egui::Rect::from_min_max(egui::pos2(0.0,0.0),egui::pos2(1.0,1.0)), available_size),
                    };
                    
                    let image_response = ui.add(egui::Image::new((texture.id(), size)).uv(uv).sense(Sense::click()));

                    if image_response.clicked() { self.load_random_image(); self.last_update = Instant::now(); }
                    
                    let mut drag_handle_response: Option<egui::Response> = None;
                    if self.show_drag_bar {
                        egui::Area::new("drag_bar_area").anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 5.0)).show(ctx, |ui| {
                            let bar_frame = Frame::none().rounding(5.0).inner_margin(egui::style::Margin::symmetric(10.0, 5.0)).fill(Color32::from_rgba_unmultiplied(30, 30, 30, 200));
                            bar_frame.show(ui, |ui| {
                                ui.label(egui::RichText::new("Drag to move").color(Color32::WHITE));
                                let response = ui.interact(ui.max_rect(), ui.id().with("drag_handle"), Sense::drag());
                                if response.dragged() { frame.drag_window(); }

                                // --- 新增：当拖动结束后，立即保存窗口位置 ---
                                if response.drag_released() {
                                    if let Some(pos) = frame.info().window_info.position {
                                        self.config.window_pos = Some((pos.x, pos.y));
                                        save_config(&self.config);
                                    }
                                }
                                
                                drag_handle_response = Some(response);
                            });
                        });
                    }
                    
                    let is_pointer_over_ui = image_response.hovered() || drag_handle_response.as_ref().map_or(false, |r| r.hovered());
                    
                    if is_pointer_over_ui { self.show_drag_bar = true; self.hover_leave_time = None; } 
                    else {
                        if self.hover_leave_time.is_none() { self.hover_leave_time = Some(Instant::now()); }
                        if let Some(leave_time) = self.hover_leave_time { if leave_time.elapsed() > Duration::from_millis(100) { self.show_drag_bar = false; } }
                    }
                    
                    if image_response.hovered() {
                        egui::Area::new("tooltip_area").anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(5.0, -5.0)).show(ctx, |ui| {
                            let tooltip_frame = Frame::none().rounding(3.0).inner_margin(egui::style::Margin::symmetric(4.0, 2.0)).fill(Color32::from_rgba_unmultiplied(20, 20, 20, 180));
                            tooltip_frame.show(ui, |ui| { ui.label(egui::RichText::new("Left-click: Next | Right-click: Settings").color(Color32::LIGHT_GRAY).small()); });
                        });
                    }

                    if image_response.secondary_clicked() { self.show_settings = true; frame.set_decorations(true); }
                } else {
                    ui.label("No images found. Please add a folder in the settings.");
                    if ui.button("Open Settings").clicked() { self.show_settings = true; frame.set_decorations(true); }
                }
            });
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

fn save_config(config: &AppConfig) { if let Ok(json) = serde_json::to_string_pretty(config) { let _ = fs::write(CONFIG_FILE, json); } }
fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> { let json_str = fs::read_to_string(CONFIG_FILE)?; let config = serde_json::from_str(&json_str)?; Ok(config) }

fn load_icon() -> eframe::IconData {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::load_from_memory(include_bytes!("../icon.ico")).expect("Failed to open icon path").into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };

    eframe::IconData {
        rgba: icon_rgba,
        width: icon_width,
        height: icon_height,
    }
}

fn main() -> Result<(), eframe::Error> {
    let (tx, rx) = mpsc::channel();
    let settings_item = MenuItem::new("Settings", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let settings_id = settings_item.id().clone();
    let quit_id = quit_item.id().clone();
    let menu = Menu::new();
    menu.append_items(&[&settings_item, &quit_item]).unwrap();

    let icon = load_icon();

    let tray_icon_data = tray_icon::Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height).expect("Failed to create tray icon");

    let _tray_icon = TrayIconBuilder::new().with_tooltip("Photo Widget").with_icon(tray_icon_data).with_menu(Box::new(menu)).build().unwrap();

    // --- 修改事件监听线程 ---
    thread::spawn(move || {
        loop {
            // 检查菜单事件
            if let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == settings_id { 
                    let _ = tx.send(TrayMessage::ShowSettings); 
                } 
                else if event.id == quit_id { 
                    let _ = tx.send(TrayMessage::Quit); 
                    break; 
                }
            }

            // --- 新增: 检查图标点击事件 ---
            if let Ok(event) = TrayIconEvent::receiver().try_recv() {
                // 如果是左键单击
                if event.click_type == ClickType::Left {
                    // 发送消息以显示设置 (您可以按需更改此处的逻辑)
                    // 例如，您可以定义一个新的 TrayMessage 来处理此事件
                    let _ = tx.send(TrayMessage::FocusWindow);
                }
            }

            thread::sleep(Duration::from_millis(100));
        }
    });
    // --- 线程修改结束 ---

    // --- 修改：在启动时加载配置，以获取初始位置 ---
    let config = load_config().unwrap_or_default();
    let initial_pos = config.window_pos.map(|(x, y)| egui::pos2(x, y));

    let native_options = eframe::NativeOptions {
        initial_window_size: Some(Vec2::new(400.0, 300.0)),
        decorated: false,
        transparent: true,
        icon_data: Some(icon),
        // --- 新增：设置窗口的初始位置 ---
        initial_window_pos: initial_pos,
        window_builder: Some(Box::new(|wb| {
            use winit::platform::windows::WindowBuilderExtWindows;
            wb.with_skip_taskbar(true)
        })),
        ..Default::default()
    };
    
    eframe::run_native("Photo Widget", native_options, Box::new(move |cc| {
        // ... (省略字体和样式的设置) ...
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert("noto_sans".to_owned(), FontData::from_static(include_bytes!("../fonts/NotoSans-Regular.ttf")).tweak(egui::FontTweak { scale: 1.0, ..Default::default() }));
        fonts.font_data.insert("noto_sans_sc".to_owned(), FontData::from_static(include_bytes!("../fonts/NotoSansSC-Regular.ttf")).tweak(egui::FontTweak { scale: 1.0, ..Default::default() }));
        fonts.font_data.insert("noto_sans_jp".to_owned(), FontData::from_static(include_bytes!("../fonts/NotoSansJP-Regular.ttf")).tweak(egui::FontTweak { scale: 1.0, ..Default::default() }));
        fonts.font_data.insert("noto_sans_kr".to_owned(), FontData::from_static(include_bytes!("../fonts/NotoSansKR-Regular.ttf")).tweak(egui::FontTweak { scale: 1.0, ..Default::default() }));
        
        fonts.families.entry(FontFamily::Proportional).or_default().extend(vec!["noto_sans".to_owned(), "noto_sans_sc".to_owned(), "noto_sans_jp".to_owned(), "noto_sans_kr".to_owned()]);
        fonts.families.entry(FontFamily::Monospace).or_default().extend(vec!["noto_sans".to_owned(), "noto_sans_sc".to_owned(), "noto_sans_jp".to_owned(), "noto_sans_kr".to_owned()]);

        cc.egui_ctx.set_fonts(fonts);
        let mut visuals = Visuals::dark();
        visuals.window_fill = Color32::TRANSPARENT;
        visuals.window_stroke.color = Color32::TRANSPARENT;
        cc.egui_ctx.set_visuals(visuals);

        // --- 修改：将加载好的 config 传递给 PhotoWidget ---
        Box::new(PhotoWidget::new(cc, rx, config))
    }))
}

#[derive(Clone, Copy, Debug)]
enum TrayMessage { ShowSettings, Quit, FocusWindow, }