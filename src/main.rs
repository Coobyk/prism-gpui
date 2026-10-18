// Prism Launcher — Rust + GPUI rewrite
// Modules: instance discovery, global settings, GPUI shell.

mod instance;
mod java;
mod launch;
mod modplatform;
mod settings;

use crate::instance::discover_instances;
use crate::settings::GlobalSettings;
use dirs::data_dir;
use gpui::{
    App, Application, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};
use std::path::PathBuf;

struct MainWindow {
    title: SharedString,
    instances: Vec<String>,
    java_count: usize,
    data_root: PathBuf,
    theme: String,
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let instance_items = if self.instances.is_empty() {
            vec![div()
                .child("No instances found.")
                .into_any_element()]
        } else {
            self.instances
                .iter()
                .map(|n| {
                    div()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(rgb(0x313244))
                        .child(n.clone())
                        .into_any_element()
                })
                .collect()
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(40.0))
                    .px_4()
                    .bg(rgb(0x181825))
                    .border_b_1()
                    .border_color(rgb(0x313244))
                    .child(self.title.clone())
                    .child(
                        div()
                            .ml_4()
                            .text_sm()
                            .text_color(rgb(0xa6adc8))
                            .child(format!("{} instances", self.instances.len()))
                    .child(
                        div()
                            .ml_4()
                            .text_sm()
                            .text_color(rgb(0xa6adc8))
                            .child(format!("{} java", self.java_count)),
                    ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .children(instance_items),
            )
    }
}

fn data_root() -> PathBuf {
    // Prism default data dir; override via env PRISM_DATA_DIR
    if let Ok(p) = std::env::var("PRISM_DATA_DIR") {
        return PathBuf::from(p);
    }
    data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PrismLauncher")
}

fn main() {
    let root = data_root();
    let settings = GlobalSettings::load(&root);
    let instances = discover_instances(&root);
    let instance_names: Vec<String> = instances.iter().map(|i| i.name.clone()).collect();
    let java_count = java::discover_java(&root).len();
    let theme = settings.theme.clone();

    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1024.), px(720.)), cx);
        let names = instance_names.clone();
        let root = root.clone();
        let theme = theme.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Prism Launcher (Rust/GPUI)".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_, cx| {
                cx.new(|_| MainWindow {
                    title: "Prism Launcher".into(),
                    instances: names,
                    java_count: java_count,
                    data_root: root,
                    theme,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
