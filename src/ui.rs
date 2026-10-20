//! GPUI views. Mirrors launcher/ui/*.

use crate::theme::Theme;
use gpui::{
    AnyElement, App, Bounds, Context, Entity, FocusHandle, IntoElement, KeyDownEvent,
    ParentElement, Render, SharedString, Styled, UniformListScrollHandle, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, size, uniform_list,
};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Mods,
    ResourcePacks,
    ShaderPacks,
    Worlds,
    Screenshots,
    Logs,
    CrashReports,
}

impl Tab {
    const ALL: [Tab; 7] = [
        Tab::Mods,
        Tab::ResourcePacks,
        Tab::ShaderPacks,
        Tab::Worlds,
        Tab::Screenshots,
        Tab::Logs,
        Tab::CrashReports,
    ];
    fn label(&self) -> &'static str {
        match self {
            Tab::Mods => "Mods",
            Tab::ResourcePacks => "Resource Packs",
            Tab::ShaderPacks => "Shader Packs",
            Tab::Worlds => "Worlds",
            Tab::Screenshots => "Screenshots",
            Tab::Logs => "Logs",
            Tab::CrashReports => "Crash Reports",
        }
    }
    fn subdir(&self) -> &'static [&'static str] {
        match self {
            Tab::Mods => &["mods"],
            Tab::ResourcePacks => &["resourcepacks"],
            Tab::ShaderPacks => &["shaderpacks"],
            Tab::Worlds => &["saves"],
            Tab::Screenshots => &["screenshots"],
            Tab::Logs => &["logs"],
            Tab::CrashReports => &["crash-reports"],
        }
    }
}

pub struct InstanceView {
    pub name: SharedString,
    pub dir: PathBuf,
    pub data_root: PathBuf,
    pub tab: Tab,
    pub entries: Vec<String>,
    pub status: SharedString,
    pub theme: Theme,
    /// Selection index for keyboard navigation.
    selected: usize,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    auto_focused: bool,
    /// Set after the first Delete click; a second click removes the instance.
    pending_delete: bool,
}

impl InstanceView {
    pub fn new(
        name: impl Into<SharedString>,
        dir: PathBuf,
        data_root: PathBuf,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut v = Self {
            name: name.into(),
            dir,
            data_root,
            tab: Tab::Mods,
            entries: Vec::new(),
            status: "Ready".into(),
            theme,
            selected: 0,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::default(),
            auto_focused: false,
            pending_delete: false,
        };
        v.refresh();
        v
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let len = self.entries.len();
        if len == 0 {
            return;
        }
        let current = self.selected.min(len - 1) as isize;
        self.selected = (current + delta).clamp(0, len as isize - 1) as usize;
        cx.notify();
    }

    fn cycle_tab(&mut self, delta: isize, cx: &mut Context<Self>) {
        let all = Tab::ALL;
        let current = all.iter().position(|t| *t == self.tab).unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(all.len() as isize) as usize;
        self.tab = all[next];
        self.selected = 0;
        self.refresh();
        cx.notify();
    }

    /// Act on the selected entry: open files, toggle mods.
    fn activate_selected(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return;
        };
        match self.tab {
            Tab::Screenshots | Tab::Logs | Tab::CrashReports => {
                let _ = crate::desktop::open_path(&self.current_dir().join(&entry));
            }
            Tab::Mods if !entry.ends_with('/') => {
                self.toggle_mod(&entry);
                cx.notify();
            }
            _ => {}
        }
    }

    fn toggle_mod(&mut self, file: &str) {
        let dir = self.current_dir().join(file);
        let target = if file.ends_with(".disabled") {
            dir.with_file_name(file.trim_end_matches(".disabled").to_string())
        } else {
            dir.with_file_name(format!("{file}.disabled"))
        };
        let _ = std::fs::rename(&dir, &target);
        self.refresh();
    }

    fn current_dir(&self) -> PathBuf {
        let mut p = self.dir.join("minecraft");
        for s in self.tab.subdir() {
            p = p.join(s);
        }
        p
    }

    pub fn refresh(&mut self) {
        let dir = self.current_dir();
        let mut names: Vec<String> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                let n = e.file_name().to_string_lossy().to_string();
                if p.is_dir() {
                    names.push(format!("{n}/"));
                } else {
                    names.push(n);
                }
            }
        }
        names.sort();
        self.entries = names;
        if self.entries.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.entries.len() {
            self.selected = self.entries.len() - 1;
        }
        self.status = format!("{} in {}", self.entries.len(), dir.display()).into();
    }

    /// Minecraft version of this instance, from its component pack.
    fn mc_version(&self) -> String {
        crate::meta::read_component_pack(&self.dir)
            .and_then(|p| {
                p.components
                    .into_iter()
                    .find(|c| c.uid == "net.minecraft")
                    .map(|c| c.version)
            })
            .unwrap_or_default()
    }

    /// Open the instance folder in the desktop file manager.
    fn open_folder(&mut self) {
        let _ = crate::desktop::open_path(&self.dir);
    }

    /// Export the instance as a `.mrpack`, asking for a destination first.
    fn export(&mut self, cx: &mut Context<Self>) {
        let name = self.name.to_string();
        let version = self.mc_version();
        let dir = self.dir.clone();
        let rx = cx.prompt_for_new_path(
            std::path::Path::new(""),
            Some(&format!("{name}.mrpack")),
        );
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

    /// Copy the instance to a new directory.
    fn duplicate(&mut self, cx: &mut Context<Self>) {
        let name = self.name.to_string();
        let target = format!("{name} copy");
        match crate::instance::duplicate_instance(&self.data_root, &name, &target) {
            Ok(dir) => self.status = format!("Duplicated to {}", dir.display()).into(),
            Err(e) => self.status = format!("Duplicate failed: {e}").into(),
        }
        cx.notify();
    }

    /// Remove the instance. The first click arms the action, the second one
    /// deletes, so a stray click cannot destroy an instance.
    fn delete(&mut self, cx: &mut Context<Self>) {
        let name = self.name.to_string();
        if !self.pending_delete {
            self.pending_delete = true;
            self.status = "Click Delete again to remove this instance".into();
            cx.notify();
            return;
        }
        self.pending_delete = false;
        match crate::instance::remove_instance(&self.data_root, &name) {
            Ok(()) => {
                self.status = "Instance removed".into();
                self.refresh();
            }
            Err(e) => self.status = format!("Delete failed: {e}").into(),
        }
        cx.notify();
    }

}


impl Render for InstanceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Focus once so arrow keys/tab switching work without a click first.
        if !self.auto_focused {
            self.focus.focus(window);
            self.auto_focused = true;
        }
        let theme = self.theme;
        let this_pending_delete = self.pending_delete;
        let tabs = Tab::ALL
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let selected = *t == self.tab;
                let t2 = *t;
                div()
                    .id(i)
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if selected { rgb(theme.accent) } else { rgb(theme.border) })
                    .text_color(if selected { rgb(theme.on_accent) } else { rgb(theme.text) })
                    .child(t.label())
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.tab = t2;
                        this.refresh();
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        // Virtualised list: only the visible slice of entries is built.
        let tab = self.tab;
        let entries = self.entries.clone();
        let selected = self.selected;
        let entity = cx.entity();
        let list: AnyElement = if entries.is_empty() {
            div()
                .p_3()
                .text_color(rgb(theme.subtext))
                .child("(empty)")
                .into_any_element()
        } else {
            uniform_list("instance-entries", entries.len(), move |range, _window, _cx| {
                range
                    .filter_map(|i| {
                        let entry = entries.get(i)?.clone();
                        Some(entry_row(
                            entry,
                            i,
                            i == selected,
                            tab,
                            theme,
                            entity.clone(),
                        ))
                    })
                    .collect::<Vec<_>>()
            })
            .track_scroll(self.scroll.clone())
            .flex_1()
            .into_any_element()
        };

        let this_pending = self.pending_delete;

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(move |this: &mut Self, ev: &KeyDownEvent, _window, cx| {
                match ev.keystroke.key.as_str() {
                    "up" => this.move_selection(-1, cx),
                    "down" => this.move_selection(1, cx),
                    "left" => this.cycle_tab(-1, cx),
                    "right" => this.cycle_tab(1, cx),
                    "enter" => this.activate_selected(cx),
                    "l" => {
                        let java = crate::java::ensure_for_instance(&this.data_root, &this.dir);
                        if let Ok(java) = java {
                            let account = crate::accounts::AccountStore::load(&this.data_root)
                                .active_account()
                                .cloned();
                            if let Some(profile) = crate::launch::build_profile_with_account(
                                &this.dir,
                                &this.data_root,
                                &java,
                                account.as_ref(),
                            ) {
                                crate::console::open_console(
                                    profile,
                                    this.name.to_string(),
                                    theme,
                                    cx,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(40.0))
                    .px_4()
                    .gap_2()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(self.name.clone())
                    .child(
                        div()
                            .id("launch-instance")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.accent))
                            .text_color(rgb(theme.on_accent))
                            .cursor_pointer()
                            .child("Launch")
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                match crate::java::ensure_for_instance(&this.data_root, &this.dir) {
                                    Ok(java) => {
                                        let account =
                                            crate::accounts::AccountStore::load(&this.data_root)
                                                .active_account()
                                                .cloned();
                                        if let Some(profile) =
                                            crate::launch::build_profile_with_account(
                                                &this.dir,
                                                &this.data_root,
                                                &java,
                                                account.as_ref(),
                                            )
                                        {
                                            crate::console::open_console(
                                                profile,
                                                this.name.to_string(),
                                                theme,
                                                cx,
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("cannot launch: {e}");
                                    }
                                }
                            })),
                    )
                    .child(
                        div()
                            .id("browse-mods")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.accent))
                            .text_color(rgb(theme.on_accent))
                            .cursor_pointer()
                            .child("Browse Mods")
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                let dir = this.dir.clone();
                                let root = this.data_root.clone();
                                crate::modbrowser::open_mod_browser(dir, root, theme, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("inst-settings")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child("Settings")
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                if let Some(inst) = crate::instance::Instance::load(&this.dir) {
                                    crate::ui::open_instance_settings(this.dir.clone(), inst, theme, cx);
                                }
                            })),
                    )
                    .child(
                        div()
                            .id("inst-export")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child("Export…")
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.export(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("inst-duplicate")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child("Duplicate")
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.duplicate(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("inst-delete")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(if this_pending { rgb(theme.accent) } else { rgb(theme.surface) })
                            .text_color(if this_pending {
                                rgb(theme.on_accent)
                            } else {
                                rgb(theme.text)
                            })
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child(if this_pending { "Really delete?" } else { "Delete" })
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.delete(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("open-folder")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child("Open Folder")
                            .on_click(cx.listener(move |this: &mut Self, _, _, _cx| {
                                let _ = crate::desktop::open_path(&this.dir.join("minecraft"));
                            })),
                    )
                    .child(
                        div()
                            .id("export-instance")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child("Export…")
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.export(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("duplicate-instance")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child("Duplicate")
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.duplicate(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("delete-instance")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(if this_pending_delete { rgb(0x9e3b3b) } else { rgb(theme.surface) })
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child(if this_pending_delete { "Really delete?" } else { "Delete" })
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.delete(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("view-servers")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child("Servers")
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                crate::ui::open_servers(this.dir.clone(), theme, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("view-screenshots")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .cursor_pointer()
                            .child("Screenshots")
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                crate::ui::open_screenshots(this.dir.clone(), theme, cx);
                            })),
                    )
                    .child(div().ml_auto().text_sm().text_color(rgb(theme.subtext)).child(self.status.clone())),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .p_3()
                    .children(tabs),
            )
            .child(div().flex_1().flex().flex_col().overflow_hidden().child(list))
    }
}

/// One virtualised instance-content row.
fn entry_row(
    entry: String,
    i: usize,
    selected: bool,
    tab: Tab,
    theme: Theme,
    entity: Entity<InstanceView>,
) -> AnyElement {
    let openable = matches!(tab, Tab::Screenshots | Tab::Logs | Tab::CrashReports);
    let label_entity = entity.clone();
    let label_file = entry.clone();
    let label = if openable {
        div()
            .id(("entry", i))
            .cursor_pointer()
            .child(entry.clone())
            .on_click(move |_, _, cx| {
                let file = label_file.clone();
                label_entity.update(cx, |this, _cx| {
                    let _ = crate::desktop::open_path(&this.current_dir().join(&file));
                });
            })
            .into_any_element()
    } else {
        div().child(entry.clone()).into_any_element()
    };
    let mut row = div()
        .id(("entry-row", i))
        .flex()
        .items_center()
        .justify_between()
        .px_3()
        .py_1()
        .border_b_1()
        .border_color(rgb(theme.border))
        .bg(if selected { rgb(theme.surface) } else { rgb(theme.background) })
        .child(label);
    if tab == Tab::Mods && !entry.ends_with('/') {
        let file = entry.clone();
        let disabled = file.ends_with(".disabled");
        row = row.child(
            div()
                .id(("toggle", i))
                .px_2()
                .rounded_md()
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border))
                .cursor_pointer()
                .child(if disabled { "Enable" } else { "Disable" })
                .on_click(move |_, _, cx| {
                    let file = file.clone();
                    entity.update(cx, |this, cx| {
                        this.toggle_mod(&file);
                        cx.notify();
                    });
                }),
        );
    }
    row.into_any_element()
}

/// Open a new window showing an instance's contents.
pub fn open_instance(name: &str, dir: &Path, data_root: &Path, theme: Theme, cx: &mut App) -> Option<gpui::WindowHandle<InstanceView>> {
    let name = name.to_string();
    let dir = dir.to_path_buf();
    let data_root = data_root.to_path_buf();
    let bounds = Bounds::centered(None, size(px(800.), px(600.)), cx);
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(format!("{name} — Prism Launcher").into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|cx| {
                    InstanceView::new(name.clone(), dir.clone(), data_root.clone(), theme, cx)
                })
            },
        )
        .ok()?;
    Some(handle)
}

// ---- New instance window ----

use crate::instance::create_instance;
use crate::widgets::text_input::TextInput;

pub struct NewInstanceView {
    name_input: Entity<TextInput>,
    version_input: Entity<TextInput>,
    status: SharedString,
    theme: Theme,
    data_root: PathBuf,
}

impl NewInstanceView {
    pub fn new(data_root: PathBuf, theme: Theme, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| {
            let mut t = TextInput::new("Instance name", cx);
            t.set_theme(theme, cx);
            t
        });
        let version_input = cx.new(|cx| {
            let mut t = TextInput::new("Minecraft version (e.g. 1.21.1)", cx);
            t.set_theme(theme, cx);
            t
        });
        Self {
            name_input,
            version_input,
            status: "Ready".into(),
            theme,
            data_root,
        }
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).text().trim().to_string();
        let version = self.version_input.read(cx).text().trim().to_string();
        if name.is_empty() || version.is_empty() {
            self.status = "Name and version are required".into();
            cx.notify();
            return;
        }
        match create_instance(&self.data_root, &name, &version) {
            Ok(dir) => self.status = format!("Created {}", dir.display()).into(),
            Err(e) => self.status = format!("Error: {e}").into(),
        }
        cx.notify();
    }
}

impl Render for NewInstanceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .child(div().child("New Instance"))
            .child(div().w(px(360.)).child(self.name_input.clone()))
            .child(div().w(px(360.)).child(self.version_input.clone()))
            .child(
                div()
                    .id("create-instance")
                    .px_3()
                    .py_1()
                    .w(px(120.))
                    .rounded_md()
                    .bg(rgb(theme.accent))
                    .text_color(rgb(theme.on_accent))
                    .cursor_pointer()
                    .child("Create")
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.create(cx))),
            )
            .child(div().text_sm().text_color(rgb(theme.subtext)).child(self.status.clone()))
    }
}

pub fn open_new_instance(
    data_root: PathBuf,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<NewInstanceView>> {
    let bounds = Bounds::centered(None, size(px(440.), px(320.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("New Instance — Prism Launcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| NewInstanceView::new(data_root, theme, cx)),
    )
    .ok()
}

// ---- Settings window ----

use crate::settings::{GlobalSettings, ValueKind, SETTINGS, pages};

pub struct SettingsView {
    settings: GlobalSettings,
    page: usize,
    /// Text inputs for every text/path/secret setting.
    inputs: Vec<(&'static crate::settings::SettingDef, Entity<TextInput>)>,
    status: SharedString,
    theme: Theme,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
}

impl SettingsView {
    pub fn new(settings: GlobalSettings, theme: Theme, cx: &mut Context<Self>) -> Self {
        let inputs = SETTINGS
            .iter()
            .filter(|d| d.kind != ValueKind::Bool)
            .map(|d| {
                let initial = settings.get(d.key).unwrap_or(d.default).to_string();
                let label = d.label;
                let secret = d.kind == ValueKind::Secret;
                let input = cx.new(|cx| {
                    let mut t = TextInput::new(label, cx);
                    t.set_text(initial, cx);
                    t.set_theme(theme, cx);
                    t.set_masked(secret, cx);
                    t
                });
                (d, input)
            })
            .collect();
        Self {
            settings,
            page: 0,
            inputs,
            status: "Changes are saved when you press Save".into(),
            theme,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::default(),
        }
    }

    /// Cycle an enum setting through its allowed values.
    fn cycle_enum(&mut self, key: &'static str, options: &'static [&'static str], cx: &mut Context<Self>) {
        if options.is_empty() {
            return;
        }
        let current = self.settings.get_or(key, options[0]).to_string();
        let idx = options.iter().position(|o| *o == current).unwrap_or(0);
        let next = options[(idx + 1) % options.len()];
        self.settings.set(key, next);
        self.sync_input(key, next, cx);
        cx.notify();
    }

    /// Push a value back into the matching text input.
    fn sync_input(&mut self, key: &str, value: &str, cx: &mut Context<Self>) {
        if let Some((_, input)) = self.inputs.iter().find(|(d, _)| d.key == key) {
            let value = value.to_string();
            input.update(cx, |t, cx| t.set_text(value, cx));
        }
    }

    /// Open a platform file/directory picker for a path setting.
    fn browse_path(&mut self, key: &'static str, directories: bool, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: !directories,
            directories,
            multiple: false,
            prompt: Some("Select a path".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(path) = paths.first() {
                    let text = path.display().to_string();
                    this.update(cx, |this, cx| {
                        this.settings.set(key, &text);
                        this.sync_input(key, &text, cx);
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        for (def, input) in &self.inputs {
            let value = input.read(cx).text().trim().to_string();
            self.settings.set(def.key, &value);
        }
        match std::env::var("PRISM_MS_CLIENT_ID") {
            Ok(id) if !id.is_empty() => {
                self.settings.set("MSAClientIDOverride", &id);
            }
            _ => {}
        }
        match self.settings.save() {
            Ok(()) => {
                self.theme = Theme::for_name(self.settings.theme());
                self.status = "Saved".into();
            }
            Err(e) => self.status = format!("Save failed: {e}").into(),
        }
        cx.notify();
    }

    fn toggle_bool(&mut self, key: &'static str, cx: &mut Context<Self>) {
        let current = self.settings.get_bool(key);
        self.settings.set_bool(key, !current);
        cx.notify();
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let all_pages = pages();
        let page_name = all_pages.get(self.page).copied().unwrap_or("General");

        let tabs: Vec<_> = all_pages
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let selected = i == self.page;
                div()
                    .id(("page", i))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if selected { rgb(theme.accent) } else { rgb(theme.surface) })
                    .text_color(if selected { rgb(theme.on_accent) } else { rgb(theme.text) })
                    .child(*p)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.page = i;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();

        // One row per setting; virtualised so long pages (and 30+ rows) scroll.
        let rows: Vec<(
            &'static crate::settings::SettingDef,
            Option<Entity<TextInput>>,
            String,
        )> = SETTINGS
            .iter()
            .filter(|d| d.page == page_name)
            .map(|d| {
                let input = self
                    .inputs
                    .iter()
                    .find(|(x, _)| x.key == d.key)
                    .map(|(_, i)| i.clone());
                let value = self.settings.get(d.key).unwrap_or(d.default).to_string();
                (d, input, value)
            })
            .collect();
        let entity = cx.entity();
        let list = uniform_list("settings-rows", rows.len(), move |range, _window, _cx| {
            range
                .filter_map(|i| {
                    let (def, input, value) = rows.get(i)?.clone();
                    Some(settings_row(def, input, value, theme, entity.clone()))
                })
                .collect::<Vec<_>>()
        })
        .track_scroll(self.scroll.clone())
        .flex_1();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this: &mut Self, ev: &KeyDownEvent, _window, cx| {
                let pages = crate::settings::pages();
                match ev.keystroke.key.as_str() {
                    "left" => {
                        this.page = (this.page + pages.len() - 1) % pages.len();
                        cx.notify();
                    }
                    "right" => {
                        this.page = (this.page + 1) % pages.len();
                        cx.notify();
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .p_3()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(div().text_lg().child("Global Settings"))
                    .child(
                        div()
                            .id("save-settings")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.accent))
                            .text_color(rgb(theme.on_accent))
                            .cursor_pointer()
                            .child("Save")
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| this.save(cx))),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .text_sm()
                            .text_color(rgb(theme.subtext))
                            .child(self.status.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .p_3()
                    .bg(rgb(theme.background))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .children(tabs),
            )
            .child(list)
    }
}

/// One settings row: label, control and current value.
fn settings_row(
    def: &'static crate::settings::SettingDef,
    input: Option<Entity<TextInput>>,
    value: String,
    theme: Theme,
    entity: Entity<SettingsView>,
) -> AnyElement {
    use crate::settings::ValueKind;
    let label = div()
        .w(px(240.))
        .flex_shrink_0()
        .text_sm()
        .child(def.label);
    let key = def.key;
    let control: AnyElement = match def.kind {
        ValueKind::Bool => {
            let on = matches!(value.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on");
            let e = entity.clone();
            div()
                .id(gpui::ElementId::Name(SharedString::from(format!("bool-{key}"))))
                .px_3()
                .py_1()
                .w(px(80.))
                .rounded_md()
                .cursor_pointer()
                .bg(if on { rgb(theme.accent) } else { rgb(theme.surface) })
                .text_color(if on { rgb(theme.on_accent) } else { rgb(theme.text) })
                .border_1()
                .border_color(rgb(theme.border))
                .child(if on { "On" } else { "Off" })
                .on_click(move |_, _, cx| {
                    e.update(cx, |this, cx| this.toggle_bool(key, cx));
                })
                .into_any_element()
        }
        ValueKind::Enum(options) => {
            let shown = if value.is_empty() {
                "(default)".to_string()
            } else {
                value.clone()
            };
            let e = entity.clone();
            div()
                .id(gpui::ElementId::Name(SharedString::from(format!("enum-{key}"))))
                .px_3()
                .py_1()
                .w(px(180.))
                .rounded_md()
                .cursor_pointer()
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border))
                .child(shown)
                .on_click(move |_, _, cx| {
                    e.update(cx, |this, cx| this.cycle_enum(key, options, cx));
                })
                .into_any_element()
        }
        ValueKind::Path => {
            let e = entity.clone();
            let browse = div()
                .id(gpui::ElementId::Name(SharedString::from(format!("browse-{key}"))))
                .px_2()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border))
                .child("Browse…")
                .on_click(move |_, _, cx| {
                    // A Java binary is a file; every other path setting is a
                    // directory (instance dir, icons dir, ...).
                    e.update(cx, |this, cx| {
                        this.browse_path(key, key != "JavaPath", cx)
                    });
                });
            let mut row = div().flex().items_center().gap_2().w(px(420.));
            if let Some(input) = input {
                row = row.child(div().flex_1().child(input));
            }
            row.child(browse).into_any_element()
        }
        _ => match input {
            Some(input) => div().w(px(420.)).child(input).into_any_element(),
            None => div().into_any_element(),
        },
    };
    div()
        .id(gpui::ElementId::Name(SharedString::from(format!("setting-{key}"))))
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(rgb(theme.border))
        .child(label)
        .child(control)
        .child(
            div()
                .text_xs()
                .text_color(rgb(theme.subtext))
                .child("")
                .into_any_element(),
        )
        .into_any_element()
}

pub fn open_settings(
    settings: GlobalSettings,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<SettingsView>> {
    let bounds = Bounds::centered(None, size(px(860.), px(640.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Settings — Prism Launcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| SettingsView::new(settings, theme, cx)),
    )
    .ok()
}

// ---- Instance settings window ----

pub struct InstanceSettingsView {
    dir: PathBuf,
    min_input: Entity<TextInput>,
    max_input: Entity<TextInput>,
    java_input: Entity<TextInput>,
    status: SharedString,
    theme: Theme,
}

impl InstanceSettingsView {
    pub fn new(dir: PathBuf, instance: &crate::instance::Instance, theme: Theme, cx: &mut Context<Self>) -> Self {
        let min_input = cx.new(|cx| {
            let mut t = TextInput::new("Min memory (MB)", cx);
            t.set_text(instance.min_mem.to_string(), cx);
            t.set_theme(theme, cx);
            t
        });
        let max_input = cx.new(|cx| {
            let mut t = TextInput::new("Max memory (MB)", cx);
            t.set_text(instance.max_mem.to_string(), cx);
            t.set_theme(theme, cx);
            t
        });
        let java_input = cx.new(|cx| {
            let mut t = TextInput::new("Java path", cx);
            t.set_text(instance.java_path.clone(), cx);
            t.set_theme(theme, cx);
            t
        });
        Self {
            dir,
            min_input,
            max_input,
            java_input,
            status: "Ready".into(),
            theme,
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(mut inst) = crate::instance::Instance::load(&self.dir) else {
            self.status = "Could not load instance.cfg".into();
            cx.notify();
            return;
        };
        let min = self.min_input.read(cx).text().trim().to_string();
        let max = self.max_input.read(cx).text().trim().to_string();
        let java = self.java_input.read(cx).text().trim().to_string();
        inst.set("MinMemAlloc", &min);
        inst.set("MaxMemAlloc", &max);
        inst.set("JavaPath", &java);
        match inst.save() {
            Ok(()) => self.status = "Saved".into(),
            Err(e) => self.status = format!("Save failed: {e}").into(),
        }
        cx.notify();
    }
}

impl Render for InstanceSettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .child("Instance Settings")
            .child(div().w(px(320.)).child(self.min_input.clone()))
            .child(div().w(px(320.)).child(self.max_input.clone()))
            .child(div().w(px(520.)).child(self.java_input.clone()))
            .child(
                div()
                    .id("save-instance-settings")
                    .px_3()
                    .py_1()
                    .w(px(100.))
                    .rounded_md()
                    .bg(rgb(theme.accent))
                    .text_color(rgb(theme.on_accent))
                    .cursor_pointer()
                    .child("Save")
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.save(cx))),
            )
            .child(div().text_sm().text_color(rgb(theme.subtext)).child(self.status.clone()))
    }
}

pub fn open_instance_settings(
    dir: PathBuf,
    instance: crate::instance::Instance,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<InstanceSettingsView>> {
    let bounds = Bounds::centered(None, size(px(600.), px(360.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Instance Settings — Prism Launcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| InstanceSettingsView::new(dir, &instance, theme, cx)),
    )
    .ok()
}

// ---- News window ----

pub struct NewsView {
    entries: Vec<crate::news::NewsEntry>,
    status: SharedString,
    theme: Theme,
}

impl NewsView {
    pub fn new(theme: Theme, cx: &mut Context<Self>) -> Self {
        let mut v = Self {
            entries: Vec::new(),
            status: "Loading news…".into(),
            theme,
        };
        v.reload(cx);
        v
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.status = "Loading news…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::news::fetch() })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(entries) => {
                        this.status = format!("{} entries", entries.len()).into();
                        this.entries = entries;
                    }
                    Err(e) => this.status = format!("Failed: {e}").into(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for NewsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let items: Vec<_> = self
            .entries
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, e)| {
                let url = e.url.clone();
                div()
                    .id(i)
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .cursor_pointer()
                    .child(div().text_base().child(e.title.clone()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme.subtext))
                            .child(format!("{} · {}", e.date, url)),
                    )
                    .on_click(cx.listener(move |_this: &mut Self, _, _, _cx| {
                        let _ = crate::desktop::open_url(&url);
                    }))
                    .into_any_element()
            })
            .collect();

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
                    .h(px(36.0))
                    .px_4()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child("News")
                    .child(
                        div()
                            .ml_auto()
                            .text_sm()
                            .text_color(rgb(theme.subtext))
                            .child(self.status.clone()),
                    ),
            )
            .child(div().flex_1().flex().flex_col().overflow_hidden().children(items))
    }
}

pub fn open_news(theme: Theme, cx: &mut App) -> Option<gpui::WindowHandle<NewsView>> {
    let bounds = Bounds::centered(None, size(px(620.), px(600.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("News — Prism Launcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| NewsView::new(theme, cx)),
    )
    .ok()
}

// ---- Screenshot viewer ----

use gpui::{Image, ImageFormat, img};
use std::sync::Arc;

pub struct ScreenshotView {
    dir: PathBuf,
    files: Vec<String>,
    selected: Option<usize>,
    theme: Theme,
}

impl ScreenshotView {
    pub fn new(dir: PathBuf, theme: Theme) -> Self {
        let files = list_image_files(&dir);
        let selected = if files.is_empty() { None } else { Some(0) };
        Self {
            dir,
            files,
            selected,
            theme,
        }
    }

    fn current_bytes(&self, cx: &Context<Self>) -> Option<Arc<Image>> {
        let i = self.selected?;
        let name = self.files.get(i)?;
        let path = self.dir.join(name);
        let bytes = std::fs::read(&path).ok()?;
        let format = match path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .as_deref()
        {
            Some("jpg") | Some("jpeg") => ImageFormat::Jpeg,
            Some("webp") => ImageFormat::Webp,
            Some("gif") => ImageFormat::Gif,
            _ => ImageFormat::Png,
        };
        let _ = cx;
        Some(Arc::new(Image::from_bytes(format, bytes)))
    }
}

fn list_image_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if lower.ends_with(".png")
                || lower.ends_with(".jpg")
                || lower.ends_with(".jpeg")
                || lower.ends_with(".webp")
            {
                out.push(name);
            }
        }
    }
    out.sort();
    out
}

impl Render for ScreenshotView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let image = self.current_bytes(cx);
        let count = self.files.len();
        let selected = self.selected;

        let list: Vec<_> = self
            .files
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, name)| {
                let is_sel = selected == Some(i);
                div()
                    .id(i)
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .bg(if is_sel { rgb(theme.accent) } else { rgb(theme.surface) })
                    .text_color(if is_sel { rgb(theme.on_accent) } else { rgb(theme.text) })
                    .child(name)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.selected = Some(i);
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();

        let preview = match image {
            Some(img_arc) => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(img(img_arc).max_w(px(720.)).max_h(px(540.)))
                .into_any_element(),
            None => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(theme.subtext))
                .child(if count == 0 { "No screenshots" } else { "Select a screenshot" })
                .into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .h(px(36.0))
                    .px_4()
                    .flex()
                    .items_center()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(format!("Screenshots ({count})")),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .w(px(220.))
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .border_r_1()
                            .border_color(rgb(theme.border))
                            .children(list),
                    )
                    .child(preview),
            )
    }
}

pub fn open_screenshots(
    instance_dir: PathBuf,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<ScreenshotView>> {
    let dir = instance_dir.join("minecraft").join("screenshots");
    let bounds = Bounds::centered(None, size(px(960.), px(620.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Screenshots — Prism Launcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|_| ScreenshotView::new(dir, theme)),
    )
    .ok()
}

// ---- Skin viewer window ----

pub struct SkinView {
    uuid: Option<String>,
    username: SharedString,
    image: Option<Arc<Image>>,
    status: SharedString,
    theme: Theme,
    /// Data root, used for the local skin library.
    data_root: PathBuf,
    /// Access token of the signed-in account (empty for offline accounts).
    access_token: String,
    /// Skins stored on the Mojang profile.
    mojang_skins: Vec<crate::skin::MojangSkin>,
    /// Skins kept in the launcher's local library.
    local_skins: Vec<PathBuf>,
    /// The model a newly uploaded skin uses.
    variant: crate::skin::SkinVariant,
    /// Set after the first click on "Reset", which then really resets.
    pending_reset: bool,
}

impl SkinView {
    pub fn new(
        uuid: Option<String>,
        username: String,
        theme: Theme,
        data_root: PathBuf,
        access_token: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let local_skins = crate::skin::list_local_skins(&data_root);
        let mut v = Self {
            uuid: uuid.clone(),
            username: username.into(),
            image: None,
            status: "Loading skin…".into(),
            theme,
            data_root,
            access_token,
            mojang_skins: Vec::new(),
            local_skins,
            variant: crate::skin::SkinVariant::Classic,
            pending_reset: false,
        };
        if let Some(uuid) = uuid {
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { crate::skin::SkinService::body_bytes(&uuid) })
                    .await;
                this.update(cx, |this, cx| {
                    match result {
                        Ok(bytes) => {
                            this.image =
                                Some(Arc::new(Image::from_bytes(ImageFormat::Png, bytes)));
                            this.status = "Loaded".into();
                        }
                        Err(e) => this.status = format!("Failed: {e}").into(),
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        } else {
            v.status = "No account signed in".into();
        }
        // Load the account's skin list in the background.
        v.load_mojang_skins(cx);
        v
    }

    /// Load the account's Mojang skins in the background.
    fn load_mojang_skins(&mut self, cx: &mut Context<Self>) {
        if self.access_token.is_empty() {
            return;
        }
        let token = self.access_token.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::skin::list_skins(&token) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(skins) => this.mojang_skins = skins,
                    Err(e) => this.status = format!("Could not load skins: {e}").into(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Pick a skin PNG and upload it to Mojang.
    fn upload_skin(&mut self, cx: &mut Context<Self>) {
        if self.access_token.is_empty() {
            self.status = "Sign in with a Microsoft account to upload skins".into();
            cx.notify();
            return;
        }
        let token = self.access_token.clone();
        let variant = self.variant;
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select a skin texture".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.first().cloned() else {
                return;
            };
            let name = path.display().to_string();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let png = std::fs::read(&path).map_err(|e| e.to_string())?;
                    crate::skin::upload_skin(&token, &png, variant)
                })
                .await;
            this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(()) => format!("Uploaded {name}").into(),
                    Err(e) => format!("Upload failed: {e}").into(),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Restore the default skin. The first click arms the action.
    fn reset_skin(&mut self, cx: &mut Context<Self>) {
        if self.access_token.is_empty() {
            self.status = "Sign in with a Microsoft account to change skins".into();
            cx.notify();
            return;
        }
        if !self.pending_reset {
            self.pending_reset = true;
            self.status = "Click Reset again to restore the default skin".into();
            cx.notify();
            return;
        }
        self.pending_reset = false;
        let token = self.access_token.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::skin::reset_skin(&token) })
                .await;
            this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(()) => "Skin reset to the default".into(),
                    Err(e) => format!("Reset failed: {e}").into(),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Activate one of the account's skins.
    fn activate_skin(&mut self, skin_id: String, cx: &mut Context<Self>) {
        let token = self.access_token.clone();
        let skin_id_for_task = skin_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::skin::activate_skin(&token, &skin_id_for_task) })
                .await;
            this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(()) => format!("Activated skin {skin_id}").into(),
                    Err(e) => format!("Could not activate skin: {e}").into(),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Install a skin file into the local library.
    fn install_local_skin(&mut self, cx: &mut Context<Self>) {
        let root = self.data_root.clone();
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select a skin texture".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.first().cloned() else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { crate::skin::install_skin(&root, &path) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(installed) => {
                        this.local_skins = crate::skin::list_local_skins(&this.data_root);
                        this.status = format!("Installed {}", installed.display()).into();
                    }
                    Err(e) => this.status = format!("Could not install skin: {e}").into(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Upload a skin from the local library.
    fn upload_local_skin(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.access_token.is_empty() {
            self.status = "Sign in with a Microsoft account to upload skins".into();
            cx.notify();
            return;
        }
        let token = self.access_token.clone();
        let variant = self.variant;
        let shown = path.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let png = std::fs::read(&path).map_err(|e| e.to_string())?;
                    crate::skin::upload_skin(&token, &png, variant)
                })
                .await;
            this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(()) => format!("Uploaded {}", shown.display()).into(),
                    Err(e) => format!("Upload failed: {e}").into(),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}


impl SkinView {
    /// Reload the account's skin list from Mojang.
    fn refresh_mojang_skins(&mut self, cx: &mut Context<Self>) {
        if self.access_token.is_empty() {
            return;
        }
        let token = self.access_token.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::skin::list_skins(&token) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(skins) => this.mojang_skins = skins,
                    Err(e) => this.status = format!("Could not list skins: {e}").into(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Upload a skin file to the Mojang account.
    fn upload(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.access_token.is_empty() {
            self.status = "Sign in with a Microsoft account to upload skins".into();
            cx.notify();
            return;
        }
        let token = self.access_token.clone();
        let variant = self.variant;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let png = std::fs::read(&path).map_err(|e| e.to_string())?;
                    crate::skin::upload_skin(&token, &png, variant)
                })
                .await;
            this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(()) => "Skin uploaded".into(),
                    Err(e) => format!("Upload failed: {e}").into(),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Restore the default skin. The first click arms the action.
    fn reset(&mut self, cx: &mut Context<Self>) {
        if self.access_token.is_empty() {
            self.status = "Sign in with a Microsoft account to change skins".into();
            cx.notify();
            return;
        }
        if !self.pending_reset {
            self.pending_reset = true;
            self.status = "Click Reset again to restore the default skin".into();
            cx.notify();
            return;
        }
        self.pending_reset = false;
        let token = self.access_token.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::skin::reset_skin(&token) })
                .await;
            this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(()) => "Skin reset to the default".into(),
                    Err(e) => format!("Reset failed: {e}").into(),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Activate one of the account's skins.
    fn activate(&mut self, id: String, cx: &mut Context<Self>) {
        let token = self.access_token.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::skin::activate_skin(&token, &id) })
                .await;
            this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(()) => "Skin activated".into(),
                    Err(e) => format!("Could not activate skin: {e}").into(),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Pick a skin file to upload.
    fn pick_skin(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select a skin texture".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(path) = paths.first() {
                    let path = path.clone();
                    this.update(cx, |this, cx| {
                        match crate::skin::validate_skin_file(&path) {
                            Ok(_) => this.upload(path, cx),
                            Err(e) => {
                                this.status = format!("Not a valid skin: {e}").into();
                                cx.notify();
                            }
                        }
                    })
                    .ok();
                }
            }
        })
        .detach();
    }
}

impl Render for SkinView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let preview = match self.image.clone() {
            Some(image) => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(img(image).h(px(420.)))
                .into_any_element(),
            None => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(theme.subtext))
                .child(self.status.clone())
                .into_any_element(),
        };

        // Actions: upload a skin file, toggle the model, reset the skin.
        let variant_label = match self.variant {
            crate::skin::SkinVariant::Classic => "Classic (Steve)",
            crate::skin::SkinVariant::Slim => "Slim (Alex)",
        };
        let actions = div()
            .flex()
            .items_center()
            .gap_2()
            .p_3()
            .bg(rgb(theme.surface))
            .border_t_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .id("skin-upload")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(theme.accent))
                    .text_color(rgb(theme.on_accent))
                    .cursor_pointer()
                    .child("Upload skin…")
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.upload_skin(cx);
                    })),
            )
            .child(
                div()
                    .id("skin-model")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .cursor_pointer()
                    .child(variant_label)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.variant = match this.variant {
                            crate::skin::SkinVariant::Classic => crate::skin::SkinVariant::Slim,
                            crate::skin::SkinVariant::Slim => crate::skin::SkinVariant::Classic,
                        };
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("skin-reset")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(if self.pending_reset {
                        rgb(0x9e3b3b)
                    } else {
                        rgb(theme.surface)
                    })
                    .text_color(if self.pending_reset {
                        rgb(0xffffff)
                    } else {
                        rgb(theme.text)
                    })
                    .border_1()
                    .border_color(rgb(theme.border))
                    .cursor_pointer()
                    .child(if self.pending_reset {
                        "Really reset?"
                    } else {
                        "Reset skin"
                    })
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.reset_skin(cx);
                    })),
            )
            .child(
                div()
                    .id("skin-install-local")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .cursor_pointer()
                    .child("Add to library…")
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.install_local_skin(cx);
                    })),
            )
            .child(
                div()
                    .id("skin-refresh")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .cursor_pointer()
                    .child("Refresh")
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.load_mojang_skins(cx);
                    })),
            );

        // The account's skins and the local library.
        let mut lists: Vec<AnyElement> = Vec::new();
        if !self.mojang_skins.is_empty() {
            lists.push(
                div()
                    .text_sm()
                    .text_color(rgb(theme.subtext))
                    .child("Mojang skins")
                    .into_any_element(),
            );
            for skin in self.mojang_skins.clone() {
                let id = skin.id.clone();
                let e = cx.entity();
                lists.push(
                    div()
                        .id(gpui::ElementId::Name(SharedString::from(format!(
                            "mojang-skin-{id}"
                        ))))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(div().text_sm().child(id.clone()))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(theme.subtext))
                                        .child(format!(
                                            "{} · {}",
                                            skin.state,
                                            skin.variant.unwrap_or_default()
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .id(gpui::ElementId::Name(SharedString::from(format!(
                                    "activate-{id}"
                                ))))
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .bg(rgb(theme.surface))
                                .border_1()
                                .border_color(rgb(theme.border))
                                .child("Use")
                                .on_click(move |_, _, cx| {
                                    e.update(cx, |this, cx| this.activate_skin(id.clone(), cx));
                                }),
                        )
                        .into_any_element(),
                );
            }
        }
        if !self.local_skins.is_empty() {
            lists.push(
                div()
                    .text_sm()
                    .text_color(rgb(theme.subtext))
                    .child("Local library")
                    .into_any_element(),
            );
            for skin in self.local_skins.clone() {
                let path = skin.clone();
                let e = cx.entity();
                let name = skin
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                lists.push(
                    div()
                        .id(gpui::ElementId::Name(SharedString::from(format!(
                            "local-skin-{name}"
                        ))))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .child(div().text_sm().child(name.clone()))
                        .child(
                            div()
                                .id(gpui::ElementId::Name(SharedString::from(format!(
                                    "upload-local-{name}"
                                ))))
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .bg(rgb(theme.surface))
                                .border_1()
                                .border_color(rgb(theme.border))
                                .child("Upload")
                                .on_click(move |_, _, cx| {
                                    let p = path.clone();
                                    e.update(cx, |this, cx| this.upload_local_skin(p, cx));
                                }),
                        )
                        .into_any_element(),
                );
            }
        }
        let side = div()
            .id("skin-side")
            .w(px(280.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(rgb(theme.border))
            .overflow_y_scroll()
            .children(lists);

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .h(px(36.0))
                    .px_4()
                    .flex()
                    .items_center()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(self.username.clone())
                    .child(
                        div()
                            .ml_auto()
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
                    .child(preview)
                    .child(side),
            )
            .child(actions)
    }
}

pub fn open_skin(
    uuid: Option<String>,
    username: String,
    theme: Theme,
    data_root: PathBuf,
    access_token: String,
    cx: &mut App,
) -> Option<gpui::WindowHandle<SkinView>> {
    let bounds = Bounds::centered(None, size(px(420.), px(600.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some(format!("Skin — {username}").into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| {
            cx.new(|cx| {
                SkinView::new(
                    uuid.clone(),
                    username.clone(),
                    theme,
                    data_root.clone(),
                    access_token.clone(),
                    cx,
                )
            })
        },
    )
    .ok()
}

// ---- Server list window ----

pub struct ServersView {
    servers: Vec<crate::servers::Server>,
    theme: Theme,
}

impl ServersView {
    pub fn new(instance_dir: &Path, theme: Theme) -> Self {
        Self {
            servers: crate::servers::list_servers(instance_dir),
            theme,
        }
    }
}

impl Render for ServersView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let rows: Vec<_> = if self.servers.is_empty() {
            vec![div()
                .p_3()
                .text_color(rgb(theme.subtext))
                .child("(no servers)")
                .into_any_element()]
        } else {
            self.servers
                .iter()
                .map(|s| {
                    div()
                        .flex()
                        .justify_between()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .child(s.name.clone())
                        .child(div().text_color(rgb(theme.subtext)).child(s.ip.clone()))
                        .into_any_element()
                })
                .collect()
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .h(px(36.0))
                    .px_4()
                    .flex()
                    .items_center()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(format!("Servers ({})", self.servers.len())),
            )
            .child(div().flex_1().flex().flex_col().overflow_hidden().children(rows))
    }
}

pub fn open_servers(
    instance_dir: PathBuf,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<ServersView>> {
    let bounds = Bounds::centered(None, size(px(560.), px(480.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Servers — Prism Launcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|_| ServersView::new(&instance_dir, theme)),
    )
    .ok()
}
