// Prism Launcher — Rust + GPUI rewrite
// Modules: instance discovery, global settings, launch, GPUI shell.

mod accounts;
mod auth;
mod cli;
mod desktop;
mod console;
mod forge;
mod fs;
mod groups;
mod i18n;
mod instance;
mod java;
mod launch;
mod meta;
mod metaindex;
mod modbrowser;
mod modplatform;
mod nbt;
mod net;
mod news;
mod plugin;
mod theme;
mod pack;
mod servers;
mod settings;
mod skin;
mod ui;
mod updater;
mod widgets;
mod worlds;

use crate::instance::discover_instances;
use crate::settings::GlobalSettings;
use crate::theme::Theme;
use dirs::data_dir;
use gpui::{
    AnyElement, App, Application, Bounds, Context, Entity, ExternalPaths, FocusHandle,
    KeyDownEvent, SharedString, UniformListScrollHandle, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size, uniform_list,
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
    group: Option<String>,
    icon: Option<Arc<gpui::Image>>,
}

struct MainWindow {
    title: SharedString,
    instances: Vec<InstanceRow>,
    #[allow(dead_code)]
    java_path: Option<PathBuf>,
    java_count: usize,
    data_root: PathBuf,
    theme: Theme,
    status: SharedString,
    /// Selected group filter; `None` = show all.
    filter: Option<String>,
    /// Selection index within the filtered rows, for keyboard navigation.
    selected: usize,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    /// Focus is requested once on the first frame so arrow keys work without
    /// clicking first.
    auto_focused: bool,
    plugin_count: usize,
    /// Search box that filters the instance list by name.
    search: Entity<crate::widgets::text_input::TextInput>,
    /// The current search text, refreshed from the search box on every frame.
    search_query: String,
}

impl MainWindow {
    #[allow(clippy::too_many_arguments)]
    fn new(
        title: SharedString,
        instances: Vec<InstanceRow>,
        java_path: Option<PathBuf>,
        java_count: usize,
        data_root: PathBuf,
        theme: Theme,
        plugin_count: usize,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut window = Self {
            title,
            instances,
            java_path,
            java_count,
            data_root,
            theme,
            status: "Ready — ↑/↓ select, Enter launch, O open, B browse content, N new".into(),
            filter: None,
            selected: 0,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::default(),
            auto_focused: false,
            plugin_count,
            search: cx.new(|cx| {
                let mut t = crate::widgets::text_input::TextInput::new("Search instances", cx);
                t.set_theme(theme, cx);
                t
            }),
            search_query: String::new(),
        };
        // Icons decode in the background so start-up stays instant.
        let dirs: Vec<PathBuf> = window.instances.iter().map(|r| r.dir.clone()).collect();
        window.load_icons(dirs, cx);
        window
    }

    /// Load instance icons in the background, then patch them into the rows.
    fn load_icons(&mut self, dirs: Vec<PathBuf>, cx: &mut Context<Self>) {
        if dirs.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let icons = cx
                .background_executor()
                .spawn(async move {
                    dirs.iter()
                        .map(|d| load_instance_icon(d))
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |this, cx| {
                for (row, icon) in this.instances.iter_mut().zip(icons) {
                    if row.icon.is_none() {
                        row.icon = icon;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Reload instance rows from disk (after an import or deletion).
    fn reload(&mut self) {
        self.instances = instance_rows(&self.data_root);
        self.clamp_selection();
    }

    fn visible_rows(&self) -> Vec<InstanceRow> {
        let query = self.search_query.as_str();
        self.instances
            .iter()
            .filter(|r| self.filter.is_none() || r.group == self.filter)
            .filter(|r| {
                query.is_empty()
                    || r.name.to_lowercase().contains(&query)
                    || r.version.to_lowercase().contains(&query)
                    || r.loader.to_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_rows().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let len = self.visible_rows().len();
        if len == 0 {
            return;
        }
        let current = self.selected.min(len - 1) as isize;
        let next = (current + delta).clamp(0, len as isize - 1);
        self.selected = next as usize;
        cx.notify();
    }

    fn launch_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(row) = self.visible_rows().get(self.selected).cloned() {
            self.launch(row, cx);
        }
    }

    fn open_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(row) = self.visible_rows().get(self.selected).cloned() {
            ui::open_instance(&row.name, &row.dir, &self.data_root, self.theme, cx);
        }
    }

    fn browse_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(row) = self.visible_rows().get(self.selected).cloned() {
            crate::modbrowser::open_mod_browser(
                row.dir.clone(),
                self.data_root.clone(),
                self.theme,
                cx,
            );
        }
    }

    /// Import pack archives dropped onto the window.
    fn import_dropped(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let archives: Vec<PathBuf> = paths
            .iter()
            .filter(|p| {
                matches!(
                    p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()),
                    Some(ref e) if e == "zip" || e == "mrpack"
                )
            })
            .cloned()
            .collect();
        if archives.is_empty() {
            self.status = "Drop a .mrpack or pack .zip to import it".into();
            cx.notify();
            return;
        }
        let root = self.data_root.clone();
        self.status = format!("Importing {} pack(s)…", archives.len()).into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut messages = Vec::new();
                    let mut imported = Vec::new();
                    for archive in archives {
                        match crate::pack::import_archive(&archive, &root, None) {
                            Ok(result) => {
                                messages.push(format!(
                                    "{} → {}",
                                    result.summary, result.name
                                ));
                                imported.push(result);
                            }
                            Err(e) => messages.push(format!(
                                "{}: {e}",
                                archive.file_name().unwrap_or_default().to_string_lossy()
                            )),
                        }
                    }
                    // Materialise component metadata in the background so the
                    // imported instance can launch immediately.
                    for result in &imported {
                        for (uid, version) in &result.components {
                            if uid == "net.minecraft" {
                                let _ = crate::meta::ensure_minecraft(&root, version);
                            } else {
                                let _ = crate::metaindex::ensure_component(&root, uid, version);
                            }
                        }
                    }
                    messages
                })
                .await;
            this.update(cx, |this, cx| {
                this.reload();
                this.status = result.join("; ").into();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn launch(&mut self, row: InstanceRow, cx: &mut Context<Self>) {
        let java = match java::ensure_for_instance(&self.data_root, &row.dir) {
            Ok(path) => path,
            Err(e) => {
                self.status = e.into();
                cx.notify();
                return;
            }
        };
        let account = crate::accounts::account_for_instance(&self.data_root, &row.dir);
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Keep the search text in sync with the search box.
        self.search_query = self.search.read(cx).text().trim().to_lowercase();
        // Grab keyboard focus on the first frame so arrow-key navigation works
        // without requiring a click first.
        if !self.auto_focused {
            self.focus.focus(window);
            self.auto_focused = true;
        }
        let theme = self.theme;

        // Collect the distinct groups for the sidebar.
        let mut group_names: Vec<String> = self
            .instances
            .iter()
            .filter_map(|r| r.group.clone())
            .collect();
        group_names.sort();
        group_names.dedup();

        let filter = self.filter.clone();
        let sidebar: Vec<_> = std::iter::once(None)
            .chain(group_names.iter().map(|g| Some(g.clone())))
            .enumerate()
            .map(|(i, g)| {
                let selected = filter == g;
                let label = g.clone().unwrap_or_else(|| "All instances".to_string());
                let value = g.clone();
                div()
                    .id(i)
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if selected { rgb(theme.accent) } else { rgb(theme.surface) })
                    .text_color(if selected { rgb(theme.on_accent) } else { rgb(theme.text) })
                    .child(label)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.filter = value.clone();
                        this.selected = 0;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        // Only visible rows are laid out, so thousands of instances stay fast.
        let items: Vec<(usize, InstanceRow)> =
            self.visible_rows().into_iter().enumerate().collect();
        let selected = self.selected;
        let row_root = self.data_root.clone();
        let entity = cx.entity();
        let list: AnyElement = if items.is_empty() {
            div()
                .p_4()
                .text_color(rgb(theme.subtext))
                .child("No instances found. Drop a .mrpack or pack .zip here to import one.")
                .into_any_element()
        } else {
            uniform_list(
                "instance-list",
                items.len(),
                move |range, _window, _cx| {
                    range
                        .filter_map(|i| {
                            let (idx, row) = items.get(i)?.clone();
                            Some(instance_row(
                                row,
                                idx,
                                idx == selected,
                                theme,
                                row_root.clone(),
                                entity.clone(),
                            ))
                        })
                        .collect::<Vec<_>>()
                },
            )
            .track_scroll(self.scroll.clone())
            .flex_1()
            .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this: &mut Self, ev: &KeyDownEvent, _window, cx| {
                match ev.keystroke.key.as_str() {
                    "up" => this.move_selection(-1, cx),
                    "down" => this.move_selection(1, cx),
                    "enter" => this.launch_selected(cx),
                    "o" => this.open_selected(cx),
                    "b" => this.browse_selected(cx),
                    "n" => {
                        ui::open_new_instance(this.data_root.clone(), this.theme, cx);
                    }
                    "s" => {
                        let settings = crate::settings::GlobalSettings::load(&this.data_root);
                        ui::open_settings(settings, this.theme, cx);
                    }
                    _ => {}
                }
            }))
            .on_drop(cx.listener(|this: &mut Self, paths: &ExternalPaths, _window, cx| {
                this.import_dropped(paths.paths(), cx);
            }))
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
                            .text_sm()
                            .text_color(rgb(theme.subtext))
                            .child(format!("{} plugins", self.plugin_count)),
                    )
                    .child(
                        div()
                            .w(px(220.))
                            .child(self.search.clone()),
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
                            .id("accounts")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child("Accounts")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                ui::open_accounts(this.data_root.clone(), this.theme, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("skin")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child("Skin")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let account = crate::accounts::AccountStore::load(&this.data_root)
                                    .active_account()
                                    .cloned();
                                let (uuid, name, token) = match account {
                                    Some(a) => (Some(a.uuid), a.username, a.access_token),
                                    None => (None, "no account".to_string(), String::new()),
                                };
                                ui::open_skin(
                                    uuid,
                                    name,
                                    this.theme,
                                    this.data_root.clone(),
                                    token,
                                    cx,
                                );
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
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .w(px(190.))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .p_2()
                            .bg(rgb(theme.surface))
                            .border_r_1()
                            .border_color(rgb(theme.border))
                            .children(sidebar),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .child(list),
                    ),
            )
    }
}

/// Build one instance row. Clicking the name opens the instance window, the
/// button launches it; both go through the view entity so the list can stay a
/// pure function of its data.
fn instance_row(
    row: InstanceRow,
    idx: usize,
    selected: bool,
    theme: Theme,
    data_root: PathBuf,
    entity: Entity<MainWindow>,
) -> AnyElement {
    let open_name = row.name.clone();
    let open_dir = row.dir.clone();
    let open_entity = entity.clone();
    let launch_row = row.clone();
    let launch_entity = entity;
    div()
        .id(("row", idx))
        .flex()
        .items_center()
        .justify_between()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(rgb(theme.border))
        .bg(if selected { rgb(theme.surface) } else { rgb(theme.background) })
        .child(
            div()
                .id(("open", idx))
                .flex()
                .items_center()
                .gap_3()
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    let name = open_name.clone();
                    let dir = open_dir.clone();
                    let root = data_root.clone();
                    open_entity.update(cx, |_this, cx| {
                        ui::open_instance(&name, &dir, &root, theme, cx);
                    });
                })
                .child(match row.icon.clone() {
                    Some(icon) => div()
                        .w(px(32.))
                        .h(px(32.))
                        .flex_shrink_0()
                        .child(gpui::img(icon).w(px(32.)).h(px(32.)))
                        .into_any_element(),
                    None => div().w(px(0.)).into_any_element(),
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_base().child(row.name.clone()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme.subtext))
                                .child(format!(
                                    "{} · {} · {}",
                                    row.version, row.loader, row.mem
                                )),
                        ),
                ),
        )
        .child(
            div()
                .id(("launch", idx))
                .px_3()
                .py_1()
                .rounded_md()
                .bg(rgb(theme.accent))
                .text_color(rgb(theme.on_accent))
                .cursor_pointer()
                .child("Launch")
                .on_click(move |_, _, cx| {
                    let row = launch_row.clone();
                    launch_entity.update(cx, |this, cx| this.launch(row, cx));
                }),
        )
        .into_any_element()
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
    let groups = groups::GroupStore::load(root);
    discover_instances(root)
        .into_iter()
        .map(|inst| {
            let id = inst
                .dir
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| inst.name.clone());
            let group = groups.group_of(&id).map(|s| s.to_string());
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
                group,
                // Decoded asynchronously after the window opens.
                icon: None,
            }
        })
        .collect()
}

/// Load an instance's icon (`icon.png`/`icon.jpg`) if present.
fn load_instance_icon(dir: &std::path::Path) -> Option<Arc<gpui::Image>> {
    for name in ["icon.png", "icon.jpg", "icon.jpeg"] {
        let path = dir.join(name);
        if let Ok(bytes) = std::fs::read(&path) {
            let format = if name.ends_with(".png") {
                gpui::ImageFormat::Png
            } else {
                gpui::ImageFormat::Jpeg
            };
            return Some(Arc::new(gpui::Image::from_bytes(format, bytes)));
        }
    }
    None
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
    crate::net::set_proxy(settings.proxy_url());
    // Load native plugins once; manifest plugins are read by the content
    // browser when it opens.
    let plugins = crate::plugin::PluginHost::discover(&root);
    for warning in &plugins.warnings {
        eprintln!("warning: plugin: {warning}");
    }
    for warning in plugins.init_natives(&root) {
        eprintln!("warning: plugin: {warning}");
    }
    let plugin_count = plugins.manifests.len() + plugins.natives.len();
    let rows = instance_rows(&root);
    let java = java::discover_java(&root);
    let java_path = java.first().map(|j| j.path.clone());
    let java_count = java.len();
    let theme = Theme::for_name(settings.theme());

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
                cx.new(move |cx| {
                    MainWindow::new(
                        "Prism Launcher".into(),
                        rows,
                        java_path.clone(),
                        java_count,
                        (*root).clone(),
                        theme,
                        plugin_count,
                        cx,
                    )
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
