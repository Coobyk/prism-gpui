//! GPUI views. Mirrors launcher/ui/*.

use crate::theme::Theme;
use gpui::{
    App, Bounds, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
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
}

impl Tab {
    const ALL: [Tab; 6] = [
        Tab::Mods,
        Tab::ResourcePacks,
        Tab::ShaderPacks,
        Tab::Worlds,
        Tab::Screenshots,
        Tab::Logs,
    ];
    fn label(&self) -> &'static str {
        match self {
            Tab::Mods => "Mods",
            Tab::ResourcePacks => "Resource Packs",
            Tab::ShaderPacks => "Shader Packs",
            Tab::Worlds => "Worlds",
            Tab::Screenshots => "Screenshots",
            Tab::Logs => "Logs",
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
}

impl InstanceView {
    pub fn new(name: impl Into<SharedString>, dir: PathBuf, data_root: PathBuf, theme: Theme) -> Self {
        let mut v = Self {
            name: name.into(),
            dir,
            data_root,
            tab: Tab::Mods,
            entries: Vec::new(),
            status: "Ready".into(),
            theme,
        };
        v.refresh();
        v
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
        self.status = format!("{} in {}", self.entries.len(), dir.display()).into();
    }
}

impl Render for InstanceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
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

        let is_mods = self.tab == Tab::Mods;
        let entries = if self.entries.is_empty() {
            vec![div()
                .p_3()
                .text_color(rgb(theme.subtext))
                .child("(empty)")
                .into_any_element()]
        } else {
            self.entries
                .iter()
                .cloned()
                .enumerate()
                .map(|(i, e)| {
                    let mut row = div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .child(e.clone());
                    if is_mods && !e.ends_with('/') {
                        let file = e.clone();
                        let disabled = file.ends_with(".disabled");
                        row = row.child(
                            div()
                                .id(i)
                                .px_2()
                                .rounded_md()
                                .bg(rgb(theme.surface))
                                .border_1()
                                .border_color(rgb(theme.border))
                                .cursor_pointer()
                                .child(if disabled { "Enable" } else { "Disable" })
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    let dir = this.current_dir().join(&file);
                                    let target = if file.ends_with(".disabled") {
                                        dir.with_file_name(
                                            file.trim_end_matches(".disabled").to_string(),
                                        )
                                    } else {
                                        dir.with_file_name(format!("{file}.disabled"))
                                    };
                                    let _ = std::fs::rename(&dir, &target);
                                    this.refresh();
                                    cx.notify();
                                })),
                        );
                    }
                    row.into_any_element()
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
                                let java = crate::java::discover_java(&this.data_root)
                                    .into_iter()
                                    .next();
                                if let Some(java) = java {
                                    let account = crate::accounts::AccountStore::load(&this.data_root)
                                        .active_account()
                                        .cloned();
                                    if let Some(profile) = crate::launch::build_profile_with_account(
                                        &this.dir,
                                        &this.data_root,
                                        &java.path,
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
                    .child(div().ml_auto().text_sm().text_color(rgb(theme.subtext)).child(self.status.clone())),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .p_3()
                    .children(tabs),
            )
            .child(div().flex_1().flex().flex_col().overflow_hidden().children(entries))
    }
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
            |_, cx| cx.new(|_| InstanceView::new(name.clone(), dir.clone(), data_root.clone(), theme)),
        )
        .ok()?;
    Some(handle)
}

// ---- New instance window ----

use crate::instance::create_instance;
use crate::theme::ThemeMode;
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
        let name_input = cx.new(|cx| TextInput::new("Instance name", cx));
        let version_input = cx.new(|cx| TextInput::new("Minecraft version (e.g. 1.21.1)", cx));
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

pub struct SettingsView {
    settings: crate::settings::GlobalSettings,
    status: SharedString,
    theme: Theme,
}

impl SettingsView {
    pub fn new(settings: crate::settings::GlobalSettings, theme: Theme) -> Self {
        Self {
            settings,
            status: "Ready".into(),
            theme,
        }
    }

    fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        let next = match self.theme.mode {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        };
        self.theme = Theme::for_mode(next);
        let name = match next {
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        };
        self.settings.set("ApplicationTheme", name);
        match self.settings.save() {
            Ok(()) => self.status = format!("Saved theme: {name}").into(),
            Err(e) => self.status = format!("Save failed: {e}").into(),
        }
        cx.notify();
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let entries: Vec<_> = self
            .settings
            .raw
            .iter()
            .map(|(k, v)| {
                div()
                    .flex()
                    .justify_between()
                    .px_3()
                    .py_1()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(k.clone())
                    .child(div().text_color(rgb(theme.subtext)).child(v.clone()))
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
                    .gap_3()
                    .p_3()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child("Global Settings")
                    .child(
                        div()
                            .id("toggle-theme")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.accent))
                            .text_color(rgb(theme.on_accent))
                            .cursor_pointer()
                            .child("Toggle Theme")
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| this.toggle_theme(cx))),
                    )
                    .child(div().ml_auto().text_sm().text_color(rgb(theme.subtext)).child(self.status.clone())),
            )
            .child(div().flex_1().flex().flex_col().overflow_hidden().children(entries))
    }
}

pub fn open_settings(
    settings: crate::settings::GlobalSettings,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<SettingsView>> {
    let bounds = Bounds::centered(None, size(px(640.), px(560.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Settings — Prism Launcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|_| SettingsView::new(settings, theme)),
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
            t
        });
        let max_input = cx.new(|cx| {
            let mut t = TextInput::new("Max memory (MB)", cx);
            t.set_text(instance.max_mem.to_string(), cx);
            t
        });
        let java_input = cx.new(|cx| {
            let mut t = TextInput::new("Java path", cx);
            t.set_text(instance.java_path.clone(), cx);
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
