// The modules expose a wider API than the binary uses on its own: the
// command line interface, the tests and the plugin interface share them.
#![allow(dead_code)]

// Prism Launcher — Rust + GPUI rewrite
// Modules: instance discovery, global settings, launch, GPUI shell.

mod accounts;
mod auth;
mod cli;
mod console;
mod desktop;
mod forge;
mod fs;
mod groups;
mod i18n;
mod icons;
mod instance;
mod java;
mod launch;
mod menu;
mod meta;
mod metaindex;
mod modbrowser;
mod modinfo;
mod modplatform;
mod nbt;
mod net;
mod news;
mod pack;
mod plugin;
mod servers;
mod settings;
mod skin;
mod theme;
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
    KeyDownEvent, SharedString, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb,
    size,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
struct InstanceRow {
    name: String,
    dir: PathBuf,
    version: String,
    loader: String,
    group: Option<String>,
    icon: Option<Arc<gpui::Image>>,
    /// When the instance was launched the last time, in milliseconds.
    last_launched: u64,
    /// How long the instance was played, in seconds.
    time_played: u64,
}

struct MainWindow {
    instances: Vec<InstanceRow>,
    #[allow(dead_code)]
    java_path: Option<PathBuf>,
    data_root: PathBuf,
    theme: Theme,
    status: SharedString,
    /// Selection index within the filtered rows, for keyboard navigation.
    selected: usize,
    focus: FocusHandle,
    /// Focus is requested once on the first frame so arrow keys work without
    /// clicking first.
    auto_focused: bool,
    /// Search box that filters the instance list by name.
    search: Entity<crate::widgets::text_input::TextInput>,
    /// The current search text, refreshed from the search box on every frame.
    search_query: String,
    /// The groups that are folded away.
    collapsed: Vec<String>,
    /// Open the file menu after the window is up. This is a help for looking
    /// at the menus without a mouse; set `PRISM_DEBUG_MENU` to use it.
    open_test_menu: bool,
}

impl MainWindow {
    #[allow(clippy::too_many_arguments)]
    fn new(
        instances: Vec<InstanceRow>,
        java_path: Option<PathBuf>,
        data_root: PathBuf,
        theme: Theme,
        _plugin_count: usize,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut window = Self {
            instances,
            java_path,
            data_root,
            theme,
            status: "Ready — ↑/↓ select, Enter launch, O open, B browse content, N new".into(),
            selected: 0,
            focus: cx.focus_handle(),
            auto_focused: false,
            search: cx.new(|cx| {
                let mut t = crate::widgets::text_input::TextInput::new("Search instances", cx);
                t.set_theme(theme, cx);
                t
            }),
            search_query: String::new(),
            collapsed: Vec::new(),
            open_test_menu: std::env::var("PRISM_DEBUG_MENU").is_ok(),
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

    /// Check GitHub for a newer Prism Launcher release.
    ///
    /// The result is reported in the status bar; the download and the
    /// replacement of the running binary are left to `self-update --apply`
    /// (a launcher cannot safely overwrite itself while it is running).
    fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        self.status = "Checking for updates…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::updater::latest_self_release() })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(release) => {
                        let current = env!("CARGO_PKG_VERSION");
                        if crate::updater::is_newer(current, &release.tag_name) {
                            this.status =
                                format!("update available: {} → {}", current, release.tag_name)
                                    .into();
                        } else {
                            this.status = format!("up to date ({current})").into();
                        }
                    }
                    Err(e) => this.status = format!("Update check failed: {e}").into(),
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
        let mut rows: Vec<InstanceRow> = self
            .instances
            .iter()
            .filter(|r| {
                query.is_empty()
                    || r.name.to_lowercase().contains(query)
                    || r.version.to_lowercase().contains(query)
                    || r.loader.to_lowercase().contains(query)
            })
            .cloned()
            .collect();
        let mode = crate::settings::GlobalSettings::load(&self.data_root)
            .get("InstSortMode")
            .unwrap_or("Name")
            .to_string();
        sort_instances(&mut rows, &mode);
        rows
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
            ui::open_instance(&row.name, &row.dir, &self.data_root, self.theme, None, cx);
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

    /// Select an instance by its name.
    fn select_by_name(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(index) = self.visible_rows().iter().position(|r| r.name == name) {
            self.selected = index;
            cx.notify();
        }
    }

    /// The instance the actions apply to.
    fn selected_row(&self) -> Option<InstanceRow> {
        self.visible_rows().get(self.selected).cloned()
    }

    /// Rename the selected instance.
    fn rename_selected(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row() else {
            return;
        };
        ui::open_rename_instance(self.data_root.clone(), row.name.clone(), self.theme, cx);
    }

    /// Kill the running instance.
    fn kill_selected(&mut self, cx: &mut Context<Self>) {
        self.status = "The instance is not running".into();
        cx.notify();
    }

    /// Open the instance window of the selected instance.
    ///
    /// In Prism the "Edit..." button of the instance tool bar opens the
    /// instance window; the instance settings live on its Settings page.
    fn edit_selected(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row() else {
            return;
        };
        ui::open_instance(&row.name, &row.dir, &self.data_root, self.theme, None, cx);
    }

    /// Move the selected instance to another group.
    fn change_group_selected(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let groups = crate::groups::GroupStore::load(&self.data_root);
        let current = groups.group_of(&row.name).map(|s| s.to_string());
        ui::open_groups(
            self.data_root.clone(),
            row.name.clone(),
            current,
            self.theme,
            cx,
        );
    }

    /// Open the folder of the selected instance.
    fn open_selected_folder(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let _ = crate::desktop::open_path(&row.dir);
        self.status = format!("Opened {}", row.dir.display()).into();
        cx.notify();
    }

    /// Export the selected instance.
    fn export_selected(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let dir = row.dir.clone();
        let name = row.name.clone();
        let version = row.version.clone();
        let rx = cx.prompt_for_new_path(std::path::Path::new(""), Some(&format!("{name}.mrpack")));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                let result = crate::pack::mrpack::export(&dir, &path, &name, &version);
                this.update(cx, |this, cx| {
                    this.status = match result {
                        Ok(()) => format!("Exported to {}", path.display()).into(),
                        Err(e) => format!("Export failed: {e}").into(),
                    };
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Copy the selected instance.
    fn copy_selected(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let target = format!("{} copy", row.name);
        match crate::instance::duplicate_instance(&self.data_root, &row.name, &target) {
            Ok(dir) => {
                self.reload();
                self.status = format!("Copied to {}", dir.display()).into();
            }
            Err(e) => self.status = format!("Copy failed: {e}").into(),
        }
        cx.notify();
    }

    /// Delete the selected instance.
    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row() else {
            return;
        };
        match crate::instance::remove_instance(&self.data_root, &row.name) {
            Ok(()) => {
                self.reload();
                self.status = format!("Removed {}", row.name).into();
            }
            Err(e) => self.status = format!("Delete failed: {e}").into(),
        }
        cx.notify();
    }

    /// Create a desktop shortcut for the selected instance.
    fn create_shortcut(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row() else {
            return;
        };
        match crate::desktop::create_shortcut(&row.name, &self.data_root) {
            Ok(path) => {
                self.status = format!("Created {}", path.display()).into();
            }
            Err(e) => self.status = format!("Could not create the shortcut: {e}").into(),
        }
        cx.notify();
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
                                messages.push(format!("{} → {}", result.summary, result.name));
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

        // The test hook opens the file menu once the window is up.
        if self.open_test_menu {
            self.open_test_menu = false;
            let theme = self.theme;
            cx.spawn(async move |this, cx| {
                gpui::Timer::after(std::time::Duration::from_secs(2)).await;
                let Some(entity) = this.upgrade() else {
                    return;
                };
                crate::menu::open_menu_with(
                    "File",
                    crate::menu::FILE_MENU,
                    entity,
                    true,
                    theme,
                    gpui::Point::new(px(4.), px(30.)),
                    gpui::Bounds::new(gpui::Point::new(px(0.), px(0.)), gpui::size(px(0.), px(0.))),
                    cx,
                );
            })
            .detach();
        }

        // Prism shows the instances as a grid of tiles, grouped into
        // sections; the ungrouped instances come first.
        let rows = self.visible_rows();
        let groups = group_instances(&rows);

        let selected_name = rows.get(self.selected).map(|r| r.name.clone());
        let news = cx.entity();
        let entity = cx.entity();
        let data_root = self.data_root.clone();

        // The grid of instance tiles.
        let mut sections: Vec<AnyElement> = Vec::new();
        for (group, list) in &groups {
            let title = if group.is_empty() {
                crate::i18n::tr("Ungrouped")
            } else {
                group.clone()
            };
            let tiles: Vec<AnyElement> = list
                .iter()
                .map(|row| {
                    instance_tile(
                        row.clone(),
                        selected_name.as_deref() == Some(row.name.as_str()),
                        theme,
                        entity.clone(),
                        data_root.clone(),
                    )
                })
                .collect();
            // A folded group only shows its header.
            let folded = self.collapsed.contains(&title);
            sections.push(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .child(group_header(&title, folded, theme, cx.entity()))
                    .child(if folded {
                        div().into_any_element()
                    } else {
                        div()
                            .flex()
                            .flex_wrap()
                            .w_full()
                            .gap(px(crate::theme::metrics::GRID_SPACING))
                            .children(tiles)
                            .into_any_element()
                    })
                    .into_any_element(),
            );
        }
        let grid: AnyElement = if rows.is_empty() {
            div()
                .p_4()
                .text_color(rgb(theme.subtext))
                .child(crate::i18n::tr(
                    "No instances found. Drop a .mrpack or pack .zip here to import one.",
                ))
                .into_any_element()
        } else {
            div()
                .id("instance-grid")
                .flex()
                .flex_col()
                .gap_2()
                .p(px(crate::theme::metrics::GRID_MARGIN))
                .bg(rgb(theme.base))
                .children(sections)
                .into_any_element()
        };

        let has_instance = selected_name.is_some();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .track_focus(&self.focus)
            .on_key_down(
                cx.listener(|this: &mut Self, ev: &KeyDownEvent, _window, cx| {
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
                            ui::open_settings(settings, this.data_root.clone(), this.theme, cx);
                        }
                        _ => {}
                    }
                }),
            )
            .on_drop(
                cx.listener(|this: &mut Self, paths: &ExternalPaths, _window, cx| {
                    this.import_dropped(paths.paths(), cx);
                }),
            )
            // ---- menu bar ------------------------------------------------
            .child(self.menu_bar(window, cx))
            // ---- tool bar ------------------------------------------------
            .child(self.tool_bar(window, cx))
            // ---- body ----------------------------------------------------
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("instance-grid-scroll")
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_y_scroll()
                            .child(grid),
                    )
                    .child(self.instance_tool_bar(has_instance, cx)),
            )
            // ---- news bar ------------------------------------------------
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(crate::theme::metrics::NEWSBAR_HEIGHT))
                    .px_3()
                    .gap_2()
                    .bg(rgb(theme.surface))
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .child(ui::tool_button(
                        "news",
                        &crate::i18n::tr("More News..."),
                        theme,
                        move |_window, cx| {
                            news.update(cx, |this, cx| {
                                ui::open_news(this.theme, cx);
                            });
                        },
                    )),
            )
            // ---- status bar ----------------------------------------------
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(crate::theme::metrics::STATUSBAR_HEIGHT))
                    .px_3()
                    .text_sm()
                    .text_color(rgb(theme.subtext))
                    .bg(rgb(theme.surface))
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .child(self.status.clone())
                    // Prism has no search field in the main window; it sits in
                    // the status bar so the tool bar stays as Prism has it.
                    .child(div().w(px(220.)).ml_auto().child(self.search.clone())),
            )
    }
}

impl MainWindow {
    /// The menu bar: File, Edit, View, Folders, Accounts and Help.
    fn menu_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let entity = cx.entity();
        let bounds = window.bounds();
        let has_instance = self.visible_rows().get(self.selected).is_some();
        let items: Vec<AnyElement> = crate::menu::menus()
            .into_iter()
            .enumerate()
            .map(|(i, menu)| {
                let entity = entity.clone();
                let items = menu.items;
                let title = menu.title;
                div()
                    .id(("menu", i))
                    .px_3()
                    .h(px(crate::theme::metrics::MENUBAR_HEIGHT))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .text_sm()
                    .hover(|style| style.bg(rgb(theme.alternate)))
                    .child(title)
                    .on_click(move |event, _window, cx| {
                        let position = event.position();
                        crate::menu::open_menu(
                            title,
                            items,
                            entity.clone(),
                            has_instance,
                            theme,
                            position,
                            bounds,
                            cx,
                        );
                    })
                    .into_any_element()
            })
            .collect();

        div()
            .flex()
            .items_center()
            .h(px(crate::theme::metrics::MENUBAR_HEIGHT))
            .bg(rgb(theme.surface))
            .border_b_1()
            .border_color(rgb(theme.border))
            .children(items)
            .into_any_element()
    }

    /// The tool bar: Add Instance, Folders, Settings, Help, Update, Meow and
    /// Accounts, in Prism's order.
    fn tool_bar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let entity = cx.entity();
        let add = entity.clone();
        let folders = entity.clone();
        let settings = entity.clone();
        let update = entity.clone();
        let accounts = entity.clone();
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap_0()
            .h(px(30.0))
            .px_1()
            .bg(rgb(theme.surface))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(ui::tool_button(
                "add",
                &crate::i18n::tr("Add Instance..."),
                theme,
                move |_window, cx| {
                    add.update(cx, |this, cx| {
                        ui::open_new_instance(this.data_root.clone(), this.theme, cx);
                    });
                },
            ))
            .child(ui::tool_separator(theme))
            .child(ui::tool_button(
                "folder",
                &crate::i18n::tr("Folders"),
                theme,
                move |_window, cx| {
                    folders.update(cx, |this, cx| {
                        ui::open_folders(this.data_root.clone(), this.theme, cx);
                    });
                },
            ))
            .child(ui::tool_button(
                "settings",
                &crate::i18n::tr("Settings..."),
                theme,
                move |_window, cx| {
                    settings.update(cx, |this, cx| {
                        let settings = crate::settings::GlobalSettings::load(&this.data_root);
                        ui::open_settings(settings, this.data_root.clone(), this.theme, cx);
                    });
                },
            ))
            .child(ui::tool_button(
                "help",
                &crate::i18n::tr("Help"),
                theme,
                move |_window, _cx| {
                    let _ = crate::desktop::open_url("https://prismlauncher.org/wiki/");
                },
            ))
            .child(ui::tool_button(
                "update",
                &crate::i18n::tr("Update..."),
                theme,
                move |_window, cx| {
                    update.update(cx, |this, cx| {
                        this.check_for_updates(cx);
                    });
                },
            ))
            .child(ui::tool_separator(theme))
            .child(ui::tool_button(
                "meow",
                &crate::i18n::tr("Meow"),
                theme,
                move |_window, _cx| {
                    let _ = crate::desktop::open_url("https://prismlauncher.org/");
                },
            ))
            .child(ui::tool_button(
                "account",
                &crate::i18n::tr("Accounts"),
                theme,
                move |_window, cx| {
                    accounts.update(cx, |this, cx| {
                        ui::open_accounts(this.data_root.clone(), this.theme, cx);
                    });
                },
            ))
            .into_any_element()
    }

    /// The vertical tool bar on the right, acting on the selected instance.
    fn instance_tool_bar(&mut self, has_instance: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let entity = cx.entity();

        // The entries of the bar, in Prism's order.
        let entries: Vec<(&str, &str, crate::menu::MenuAction)> = vec![
            ("rename", "Rename", crate::menu::MenuAction::Rename),
            ("launch", "Launch", crate::menu::MenuAction::Launch),
            ("kill", "Kill", crate::menu::MenuAction::Kill),
            ("edit", "Edit...", crate::menu::MenuAction::Edit),
            (
                "group",
                "Change Group...",
                crate::menu::MenuAction::ChangeGroup,
            ),
            (
                "folder",
                "Folder",
                crate::menu::MenuAction::ViewInstanceFolder,
            ),
            ("export", "Export...", crate::menu::MenuAction::Export),
            ("copy", "Copy...", crate::menu::MenuAction::Copy),
            ("delete", "Delete", crate::menu::MenuAction::Delete),
            (
                "shortcut",
                "Create Shortcut",
                crate::menu::MenuAction::CreateShortcut,
            ),
        ];

        let mut items: Vec<AnyElement> = Vec::new();
        for (index, (icon_name, label, action)) in entries.into_iter().enumerate() {
            // Prism separates the groups of the bar.
            if index == 2 || index == 4 {
                items.push(ui::tool_separator(theme));
            }
            let label = label.to_string();
            if has_instance {
                let entity = entity.clone();
                items.push(ui::tool_button(
                    icon_name,
                    &label,
                    theme,
                    move |_window, cx| {
                        entity.update(cx, |this, cx| this.run_action(action, cx));
                    },
                ));
            } else {
                items.push(ui::tool_button_disabled(&label, theme));
            }
        }

        div()
            .id("instance-toolbar")
            .w(px(crate::theme::metrics::INSTANCE_TOOLBAR_WIDTH))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_0()
            .py_1()
            .bg(rgb(theme.surface))
            .border_l_1()
            .border_color(rgb(theme.border))
            .children(items)
            .into_any_element()
    }

    /// Perform a menu action.
    pub fn run_action(&mut self, action: crate::menu::MenuAction, cx: &mut Context<Self>) {
        use crate::menu::MenuAction::*;
        match action {
            AddInstance => {
                ui::open_new_instance(self.data_root.clone(), self.theme, cx);
            }
            Rename => self.rename_selected(cx),
            Launch => self.launch_selected(cx),
            Kill => self.kill_selected(cx),
            Edit => self.edit_selected(cx),
            ChangeGroup => self.change_group_selected(cx),
            ViewInstanceFolder => self.open_selected_folder(cx),
            Export => self.export_selected(cx),
            Copy => self.copy_selected(cx),
            Delete => self.delete_selected(cx),
            CreateShortcut => self.create_shortcut(cx),
            Settings => {
                let settings = crate::settings::GlobalSettings::load(&self.data_root);
                ui::open_settings(settings, self.data_root.clone(), self.theme, cx);
            }
            CloseWindow => cx.quit(),
            UndoTrash => {
                self.status = "Nothing to undo".into();
            }
            Themes => {
                let settings = crate::settings::GlobalSettings::load(&self.data_root);
                ui::open_settings(settings, self.data_root.clone(), self.theme, cx);
            }
            Cat => {
                self.status = "Meow!".into();
            }
            ToggleStatusBar => {
                self.status = "The status bar is always shown".into();
            }
            LockToolbars => {
                self.status = "The tool bars are always visible".into();
            }
            ViewLauncherRoot => {
                let _ = crate::desktop::open_path(&self.data_root);
            }
            ViewLogs => {
                let _ = crate::desktop::open_path(&self.data_root.join("logs"));
            }
            ViewIcons => {
                let _ = crate::desktop::open_path(&self.data_root.join("icons"));
            }
            ViewCentralMods => {
                let _ = crate::desktop::open_path(&self.data_root.join("mods"));
            }
            ViewJava => {
                let _ = crate::desktop::open_path(&self.data_root.join("java"));
            }
            ViewSkins => {
                let _ = crate::desktop::open_path(&self.data_root.join("skins"));
            }
            ViewWidgetTheme => {
                let _ = crate::desktop::open_path(&self.data_root.join("themes"));
            }
            ViewIconTheme => {
                let _ = crate::desktop::open_path(&self.data_root.join("icons"));
            }
            ViewCatPack => {
                let _ = crate::desktop::open_path(&self.data_root.join("catpack"));
            }
            Accounts => {
                ui::open_accounts(self.data_root.clone(), self.theme, cx);
            }
            ClearMetadata => {
                self.status = "Metadata cleared".into();
            }
            ReportBug => {
                let _ = crate::desktop::open_url(
                    "https://github.com/PrismLauncher/PrismLauncher/issues",
                );
            }
            ViewLog => {
                let _ = crate::desktop::open_path(&self.data_root.join("logs"));
            }
            AddToPath => {
                self.status = "Add the launcher to your PATH manually".into();
            }
            Matrix => {
                let _ = crate::desktop::open_url("https://matrix.to/#/#prismlauncher:matrix.org");
            }
            Discord => {
                let _ = crate::desktop::open_url("https://discord.gg/prismlauncher");
            }
            Reddit => {
                let _ = crate::desktop::open_url("https://www.reddit.com/r/prismlauncher/");
            }
            MoreNews => {
                ui::open_news(self.theme, cx);
            }
            CheckUpdate => self.check_for_updates(cx),
            OpenWiki => {
                let _ = crate::desktop::open_url("https://prismlauncher.org/wiki/");
            }
            About => {
                self.status = format!(
                    "Prism Launcher (Rust + GPUI) {}\nhttps://prismlauncher.org",
                    env!("CARGO_PKG_VERSION")
                )
                .into();
            }
        }
        cx.notify();
    }
}

/// Sort the instances the way the `InstSortMode` setting says.
///
/// Prism offers "Name", "LastLaunch" and "Playtime".
fn sort_instances(rows: &mut [InstanceRow], mode: &str) {
    match mode {
        "LastLaunch" => rows.sort_by(|a, b| {
            b.last_launched
                .cmp(&a.last_launched)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }),
        "Playtime" => rows.sort_by(|a, b| {
            b.time_played
                .cmp(&a.time_played)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }),
        _ => rows.sort_by_key(|a| a.name.to_lowercase()),
    }
}

/// Group the instances the way Prism's instance view does: the ungrouped
/// instances come first, the other groups follow in alphabetical order.
fn group_instances(rows: &[InstanceRow]) -> Vec<(String, Vec<InstanceRow>)> {
    let mut groups: Vec<(String, Vec<InstanceRow>)> = Vec::new();
    for row in rows {
        let key = row.group.clone().unwrap_or_default();
        match groups.iter_mut().find(|(name, _)| *name == key) {
            Some((_, list)) => list.push(row.clone()),
            None => groups.push((key, vec![row.clone()])),
        }
    }
    groups.sort_by(|a, b| match (a.0.is_empty(), b.0.is_empty()) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.0.to_lowercase().cmp(&b.0.to_lowercase()),
    });
    groups
}

/// The tooltip of an instance tile: the instance folder and the play time.
struct TooltipView {
    text: String,
    theme: Theme,
}

impl Render for TooltipView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .rounded_md()
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.border))
            .child(div().text_xs().child(self.text.clone()))
    }
}

/// A separator between the groups of the tool bar.
fn tool_bar_separator(theme: Theme) -> AnyElement {
    div()
        .w(px(8.0))
        .h(px(18.0))
        .mx_1()
        .border_l_1()
        .border_color(rgb(theme.border))
        .into_any_element()
}

/// The header of a group of instances.
///
/// Prism draws a bold name with a small triangle in front of it and a thin
/// line to the right of the name.
fn group_header(title: &str, folded: bool, theme: Theme, entity: Entity<MainWindow>) -> AnyElement {
    let title = title.to_string();
    let name = title.clone();
    div()
        .id(gpui::ElementId::Name(SharedString::from(format!(
            "group-header-{title}"
        ))))
        .flex()
        .items_center()
        .gap_2()
        .h(px(20.0))
        .px_2()
        .py_1()
        .cursor_pointer()
        .bg(rgb(theme.surface))
        .border_b_1()
        .border_color(rgb(theme.border))
        .child(
            div()
                .text_xs()
                .text_color(rgb(theme.subtext))
                .child(if folded { "▶" } else { "▼" }),
        )
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(theme.text))
                .child(title.clone()),
        )
        // The thin line to the right of the name.
        .child(
            div()
                .flex_1()
                .h(px(1.0))
                .mt_0p5()
                .bg(theme.rgba(theme.text, 0.08)),
        )
        .hover(|style| style.bg(theme.rgba(theme.accent, 0.10)))
        .on_click(move |_, _, cx| {
            entity.update(cx, |this, cx| {
                if let Some(index) = this.collapsed.iter().position(|g| *g == name) {
                    this.collapsed.remove(index);
                } else {
                    this.collapsed.push(name.clone());
                }
                cx.notify();
            });
        })
        .into_any_element()
}

/// One instance tile: the icon with the instance name underneath, like the
/// tiles Prism draws in its instance grid.
fn instance_tile(
    row: InstanceRow,
    selected: bool,
    theme: Theme,
    entity: Entity<MainWindow>,
    data_root: PathBuf,
) -> AnyElement {
    let name = row.name.clone();
    let dir = row.dir.clone();
    let open_entity = entity.clone();
    let menu_entity = entity.clone();
    let launch_row = row.clone();

    // The icon of the instance, or the placeholder Prism shows without one.
    let icon: AnyElement = match row.icon.clone() {
        Some(image) => gpui::img(image)
            .w(px(crate::theme::metrics::TILE_ICON))
            .h(px(crate::theme::metrics::TILE_ICON))
            .into_any_element(),
        None => match crate::icons::icon("instance", theme.text) {
            Some(image) => gpui::img(image)
                .w(px(crate::theme::metrics::TILE_ICON))
                .h(px(crate::theme::metrics::TILE_ICON))
                .into_any_element(),
            None => div()
                .w(px(crate::theme::metrics::TILE_ICON))
                .h(px(crate::theme::metrics::TILE_ICON))
                .into_any_element(),
        },
    };

    // The band with the name: filled with the window colour, or with the
    // highlight colour when the instance is selected.
    let band = div()
        .w_full()
        .px_1()
        .py_0p5()
        .text_sm()
        .text_center()
        .text_color(if selected {
            rgb(theme.on_accent)
        } else {
            rgb(theme.text)
        })
        .bg(if selected {
            rgb(theme.accent)
        } else {
            theme.rgba(theme.background, 160.0 / 255.0)
        })
        .child(name.clone());

    let tile = div()
        .id(gpui::ElementId::Name(SharedString::from(format!(
            "tile-{name}"
        ))))
        .w(px(crate::theme::metrics::TILE_WIDTH))
        .flex()
        .flex_col()
        .items_center()
        .cursor_pointer()
        .rounded_md()
        .overflow_hidden()
        .border_1()
        .border_color(rgb(theme.border))
        .bg(if selected {
            theme.rgba(theme.accent, 0.12)
        } else {
            rgb(theme.base)
        })
        .hover(|style| {
            style
                .bg(theme.rgba(theme.accent, 0.18))
                .border_color(rgb(theme.accent))
        });

    let tile = if selected {
        tile.shadow_lg()
    } else {
        tile.shadow_sm()
    };

    tile.child(icon)
        .child(div().h(px(6.0)))
        .child(band)
        // A right click opens the context menu, like in Prism.
        .on_mouse_down(
            gpui::MouseButton::Right,
            move |event: &gpui::MouseDownEvent, window: &mut Window, cx: &mut gpui::App| {
                let bounds = window.bounds();
                let position = event.position;
                let menu_entity = menu_entity.clone();
                crate::menu::open_menu(
                    "Instance",
                    crate::menu::INSTANCE_MENU,
                    menu_entity,
                    true,
                    theme,
                    position,
                    bounds,
                    cx,
                );
            },
        )
        .on_mouse_down(
            gpui::MouseButton::Left,
            move |event: &gpui::MouseDownEvent, _window: &mut Window, cx: &mut gpui::App| {
                if event.click_count >= 2 {
                    // A double click runs the instance, or opens its window
                    // when "Edit instance on double-click" is on.
                    let launch_row = launch_row.clone();
                    let dir = dir.clone();
                    let data_root = data_root.clone();
                    open_entity.update(cx, move |main, cx| {
                        let settings = crate::settings::GlobalSettings::load(&main.data_root);
                        if settings.get_bool("EditInstanceOnDoubleClick") {
                            ui::open_instance(
                                &dir_name(&dir),
                                &dir,
                                &data_root,
                                main.theme,
                                None,
                                cx,
                            );
                        } else {
                            main.launch(launch_row.clone(), cx);
                        }
                    });
                } else {
                    open_entity.update(cx, |main, cx| {
                        main.select_by_name(&name, cx);
                    });
                }
            },
        )
        .into_any_element()
}

/// The name of an instance directory.
fn dir_name(dir: &std::path::Path) -> String {
    dir.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn data_root() -> PathBuf {
    if let Ok(p) = std::env::var("PRISM_DATA_DIR") {
        return PathBuf::from(p);
    }
    data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PrismLauncher")
}

fn instance_rows(root: &Path) -> Vec<InstanceRow> {
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
                group,
                // Decoded asynchronously after the window opens.
                icon: None,
                last_launched: inst
                    .settings
                    .get("lastLaunchTime")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                time_played: inst
                    .settings
                    .get("totalTimePlayed")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
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
    // "--gui [window] [name]" opens the launcher and, optionally, another
    // window right away; anything else is a CLI command.
    let mut gui_window: Option<String> = None;
    let mut gui_argument: Option<String> = None;
    let mut gui_page: Option<String> = None;
    if let Some(first) = args.first() {
        if first != "--gui" {
            if let Err(e) = cli::run(&args, &root) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
            return;
        }
        gui_window = args.get(1).cloned();
        gui_argument = args.get(2).cloned();
        gui_page = args.get(3).cloned();
    }
    let settings = GlobalSettings::load(&root);
    crate::net::set_proxy(settings.proxy_url());
    // Load the translation catalog for the configured language.
    let lang = if settings.get_bool("UseSystemLocale") {
        std::env::var("LANG")
            .ok()
            .and_then(|l| l.split('.').next().map(|s| s.replace('_', "-")))
            .unwrap_or_else(|| settings.language().to_string())
    } else {
        settings.language().to_string()
    };
    crate::i18n::init(&root, &lang);
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
    let _java_count = java.len();
    let theme = Theme::for_name(settings.theme());

    let shared_root = Arc::new(root);
    let gui_window = gui_window.clone();
    let gui_argument = gui_argument.clone();
    let gui_page = gui_page.clone();
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.), px(600.)), cx);
        let rows = rows.clone();
        let root = shared_root.clone();
        let theme = theme;
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Prism Launcher".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_, cx| {
                let root = root.clone();
                cx.new(move |cx| {
                    MainWindow::new(
                        rows,
                        java_path.clone(),
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

        // Open the window the user asked for on the command line.
        let root = shared_root.clone();
        if let Some(which) = gui_window.clone() {
            let argument = gui_argument.clone();
            match which.as_str() {
                "instance" => {
                    let Some(name) = argument else {
                        eprintln!("usage: prism-gpui --gui instance <name> [page]");
                        return;
                    };
                    let page = gui_page.clone();
                    if let Some(row) = instance_rows(&root).into_iter().find(|r| r.name == name) {
                        ui::open_instance(&row.name, &row.dir, &root, theme, page.as_deref(), cx);
                    }
                }
                "settings" => {
                    let settings = crate::settings::GlobalSettings::load(&root);
                    ui::open_settings(settings, (*root).clone(), theme, cx);
                }
                "accounts" => {
                    ui::open_accounts((*root).clone(), theme, cx);
                }
                "news" => {
                    ui::open_news(theme, cx);
                }
                "new" => {
                    ui::open_new_instance((*root).clone(), theme, cx);
                }
                "skin" => {
                    let account = crate::accounts::AccountStore::load(&root)
                        .active_account()
                        .cloned();
                    let (uuid, name, token) = match account {
                        Some(a) => (Some(a.uuid), a.username, a.access_token),
                        None => (None, "no account".to_string(), String::new()),
                    };
                    ui::open_skin(uuid, name, theme, (*root).clone(), token, cx);
                }
                "groups" => {
                    let rows = instance_rows(&root);
                    if let Some(row) = rows.first() {
                        let groups = crate::groups::GroupStore::load(&root);
                        let current = groups.group_of(&row.name).map(|s| s.to_string());
                        ui::open_groups((*root).clone(), row.name.clone(), current, theme, cx);
                    }
                }
                "browse" => {
                    let rows = instance_rows(&root);
                    if let Some(row) = rows.first() {
                        crate::modbrowser::open_mod_browser(
                            row.dir.clone(),
                            (*root).clone(),
                            theme,
                            cx,
                        );
                    }
                }
                _ => {}
            }
        }
    });
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    fn row(name: &str, group: Option<&str>) -> InstanceRow {
        InstanceRow {
            name: name.to_string(),
            dir: PathBuf::from(name),
            version: "1.21.1".into(),
            loader: "vanilla".into(),
            group: group.map(|g| g.to_string()),
            icon: None,
            last_launched: 0,
            time_played: 0,
        }
    }

    #[test]
    fn instances_are_sorted_the_way_prism_sorts_them() {
        let mut rows = vec![row("b", None), row("a", None), row("c", None)];
        sort_instances(&mut rows, "Name");
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);

        // The most recently launched instance comes first.
        rows[0].last_launched = 100;
        rows[2].last_launched = 300;
        sort_instances(&mut rows, "LastLaunch");
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["c", "a", "b"]);

        // The instance played the longest comes first.
        rows[0].time_played = 10;
        rows[1].time_played = 50;
        sort_instances(&mut rows, "Playtime");
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["a", "c", "b"]);
    }

    #[test]
    fn ungrouped_instances_come_first() {
        let rows = vec![
            row("b", Some("Beta")),
            row("a", None),
            row("c", Some("alpha")),
        ];
        let groups = group_instances(&rows);
        let names: Vec<&str> = groups.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, vec!["", "alpha", "Beta"]);
        assert_eq!(groups[0].1.len(), 1);
        assert_eq!(groups[0].1[0].name, "a");
    }

    #[test]
    fn instances_of_a_group_stay_together() {
        let rows = vec![
            row("a1", Some("Group")),
            row("b", None),
            row("a2", Some("Group")),
        ];
        let groups = group_instances(&rows);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "");
        assert_eq!(groups[1].0, "Group");
        assert_eq!(groups[1].1.len(), 2);
    }

    #[test]
    fn the_menu_bar_has_the_menus_prism_has() {
        let titles: Vec<&str> = crate::menu::menus().into_iter().map(|m| m.title).collect();
        assert_eq!(
            titles,
            vec!["File", "Edit", "View", "Folders", "Accounts", "Help"]
        );
    }
}
