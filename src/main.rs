// Prism Launcher — Rust + GPUI rewrite
// Modules: instance discovery, global settings, launch, GPUI shell.

mod accounts;
mod auth;
mod cli;
mod desktop;
mod console;
mod forge;
mod fs;
mod i18n;
mod instance;
mod java;
mod launch;
mod meta;
mod modbrowser;
mod modplatform;
mod net;
mod news;
mod theme;
mod pack;
mod settings;
mod ui;
mod updater;
mod widgets;
mod worlds;

use crate::instance::discover_instances;
use crate::settings::GlobalSettings;
use crate::theme::Theme;
use dirs::data_dir;
use gpui::{
    App, Application, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
struct InstanceRow {
    name: String,
    dir: PathBuf,
    version: String,
    loader: String,
    mem: String,
}

struct MainWindow {
    title: SharedString,
    instances: Vec<InstanceRow>,
    java_path: Option<PathBuf>,
    java_count: usize,
    data_root: PathBuf,
    theme: Theme,
    status: SharedString,
}

impl MainWindow {
    fn launch(&mut self, row: InstanceRow, cx: &mut Context<Self>) {
        let Some(java) = self.java_path.clone() else {
            self.status = "No Java installation found".into();
            cx.notify();
            return;
        };
        let account = crate::accounts::AccountStore::load(&self.data_root)
            .active_account()
            .cloned();
        match launch::build_profile_with_account(&row.dir, &self.data_root, &java, account.as_ref())
        {
            Some(profile) => {
                console::open_console(profile, row.name.clone(), self.theme, cx);
                let who = account
                    .as_ref()
                    .map(|a| a.username.clone())
                    .unwrap_or_else(|| "offline".into());
                self.status = format!("Launched {} as {}", row.name, who).into();
            }
            None => {
                self.status = format!("Cannot launch {}: missing metadata", row.name).into();
            }
        }
        cx.notify();
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let root_for_open = self.data_root.clone();
        let rows: Vec<_> = self
            .instances
            .iter()
            .cloned()
            .map(|row| {
                let launch_row = row.clone();
                let open_name = row.name.clone();
                let open_dir = row.dir.clone();
                let this_theme = theme;
                let open_root = root_for_open.clone();
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .id(("open", index_of(&self.instances, &row.name)))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .cursor_pointer()
                            .on_click(cx.listener(move |_this, _, _, cx| {
                                ui::open_instance(&open_name, &open_dir, &open_root, this_theme, cx);
                            }))
                            .child(div().text_base().child(row.name.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.subtext))
                                    .child(format!("{} · {} · {}", row.version, row.loader, row.mem)),
                            ),
                    )
                    .child(
                        div()
                            .id(("launch", index_of(&self.instances, &row.name)))
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.accent))
                            .text_color(rgb(theme.on_accent))
                            .cursor_pointer()
                            .child("Launch")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.launch(launch_row.clone(), cx);
                            })),
                    )
                    .into_any_element()
            })
            .collect();

        let body = if rows.is_empty() {
            vec![div()
                .p_4()
                .text_color(rgb(theme.subtext))
                .child("No instances found. Set PRISM_DATA_DIR to a Prism data directory.")
                .into_any_element()]
        } else {
            rows
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(40.0))
                    .px_4()
                    .gap_4()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(self.title.clone())
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme.subtext))
                            .child(format!("{} instances", self.instances.len())),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme.subtext))
                            .child(format!("{} java", self.java_count)),
                    )
                    .child(
                        div()
                            .id("new-instance")
                            .ml_auto()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.accent))
                            .text_color(rgb(theme.on_accent))
                            .cursor_pointer()
                            .child("New Instance")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                ui::open_new_instance(this.data_root.clone(), this.theme, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("settings")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child("Settings")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let settings = crate::settings::GlobalSettings::load(&this.data_root);
                                ui::open_settings(settings, this.theme, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("news")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child("News")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                ui::open_news(this.theme, cx);
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme.subtext))
                            .child(self.status.clone()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .children(body),
            )
    }
}

fn index_of(rows: &[InstanceRow], name: &str) -> usize {
    rows.iter().position(|r| r.name == name).unwrap_or(0)
}

fn data_root() -> PathBuf {
    if let Ok(p) = std::env::var("PRISM_DATA_DIR") {
        return PathBuf::from(p);
    }
    data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PrismLauncher")
}

fn instance_rows(root: &PathBuf) -> Vec<InstanceRow> {
    discover_instances(root)
        .into_iter()
        .map(|inst| {
            let mut version = String::from("?");
            let mut loader = String::from("vanilla");
            if let Some(pack) = meta::read_component_pack(&inst.dir) {
                for c in &pack.components {
                    if c.uid == "net.minecraft" {
                        version = c.version.clone();
                    } else if c.uid != "org.lwjgl3" && !c.dependency_only {
                        loader = format!("{} {}", c.cached_name, c.version);
                    }
                }
            }
            InstanceRow {
                name: inst.name.clone(),
                dir: inst.dir.clone(),
                version,
                loader,
                mem: format!("{}–{} MB", inst.min_mem, inst.max_mem),
            }
        })
        .collect()
}

fn main() {
    let root = data_root();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(first) = args.first() {
        if first != "--gui" {
            if let Err(e) = cli::run(&args, &root) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
            return;
        }
    }
    let settings = GlobalSettings::load(&root);
    let rows = instance_rows(&root);
    let java = java::discover_java(&root);
    let java_path = java.first().map(|j| j.path.clone());
    let java_count = java.len();
    let theme = Theme::for_name(&settings.theme);

    let shared_root = Arc::new(root);
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1024.), px(720.)), cx);
        let rows = rows.clone();
        let root = shared_root.clone();
        let theme = theme;
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
                let root = root.clone();
                cx.new(move |_| MainWindow {
                    title: "Prism Launcher".into(),
                    instances: rows,
                    java_path: java_path.clone(),
                    java_count,
                    data_root: (*root).clone(),
                    theme,
                    status: "Ready".into(),
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
