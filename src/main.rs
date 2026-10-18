// Prism Launcher — Rust + GPUI rewrite (scaffold)
// Maps to: launcher/ui/MainWindow.{h,cpp}, launcher/ui/instanceview/*
// This is a minimal window shell to validate the toolchain; feature parity
// comes later per RUST_GPUI_MIGRATION.md.

use gpui::{
    App, Application, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};

struct MainWindow {
    title: SharedString,
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(
                // Title bar (mirrors MainWindow toolbar / InstanceWindow header)
                div()
                    .flex()
                    .items_center()
                    .h(px(40.0))
                    .px_4()
                    .bg(rgb(0x181825))
                    .border_b_1()
                    .border_color(rgb(0x313244))
                    .child(self.title.clone()),
            )
            .child(
                // Instance list placeholder (mirrors InstanceListView / InstanceProxyModel)
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("No instances yet — port InstanceList + BaseInstance next."),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1024.), px(720.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Prism Launcher (Rust/GPUI)".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| MainWindow {
                    title: "Prism Launcher".into(),
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
