//! GPUI views. Mirrors launcher/ui/*.

use crate::theme::Theme;
use gpui::{
    AnyElement, App, Bounds, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
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
                    let openable = matches!(
                        self.tab,
                        Tab::Screenshots | Tab::Logs | Tab::CrashReports
                    );
                    let label = if openable {
                        let file = e.clone();
                        div()
                            .id(i)
                            .cursor_pointer()
                            .child(e.clone())
                            .on_click(cx.listener(move |this: &mut Self, _, _, _cx| {
                                let _ = crate::desktop::open_path(&this.current_dir().join(&file));
                            }))
                            .into_any_element()
                    } else {
                        div().child(e.clone()).into_any_element()
                    };
                    let mut row = div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .child(label);
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

use crate::settings::{GlobalSettings, ValueKind, SETTINGS, pages};

pub struct SettingsView {
    settings: GlobalSettings,
    page: usize,
    /// Text inputs for every non-boolean setting, keyed by registry index.
    inputs: Vec<(&'static crate::settings::SettingDef, Entity<TextInput>)>,
    status: SharedString,
    theme: Theme,
}

impl SettingsView {
    pub fn new(settings: GlobalSettings, theme: Theme, cx: &mut Context<Self>) -> Self {
        let inputs = SETTINGS
            .iter()
            .filter(|d| d.kind != ValueKind::Bool)
            .map(|d| {
                let initial = settings.get(d.key).unwrap_or(d.default).to_string();
                let label = d.label;
                let input = cx.new(|cx| {
                    let mut t = TextInput::new(label, cx);
                    t.set_text(initial, cx);
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
        }
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

        let mut rows: Vec<AnyElement> = Vec::new();
        for (idx, def) in SETTINGS.iter().enumerate().filter(|(_, d)| d.page == page_name) {
            let label = div()
                .w(px(240.))
                .flex_shrink_0()
                .text_sm()
                .child(def.label.to_string());
            let control = if def.kind == ValueKind::Bool {
                let on = self.settings.get_bool(def.key);
                let key = def.key;
                div()
                    .id(("bool", idx))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if on { rgb(theme.accent) } else { rgb(theme.surface) })
                    .text_color(if on { rgb(theme.on_accent) } else { rgb(theme.text) })
                    .border_1()
                    .border_color(rgb(theme.border))
                    .child(if on { "On" } else { "Off" })
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.toggle_bool(key, cx);
                    }))
                    .into_any_element()
            } else if let Some((_, input)) = self.inputs.iter().find(|(d, _)| d.key == def.key) {
                div().w(px(420.)).child(input.clone()).into_any_element()
            } else {
                div().into_any_element()
            };
            let mut row = div()
                .id(("row", idx))
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(rgb(theme.border))
                .child(label)
                .child(control);
            if let Some(v) = self.settings.get(def.key) {
                row = row.child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme.subtext))
                        .child(format!("({v})")),
                );
            }
            rows.push(row.into_any_element());
        }

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
            .child(div().flex_1().flex().flex_col().overflow_hidden().children(rows))
    }
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
}

impl SkinView {
    pub fn new(
        uuid: Option<String>,
        username: String,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut v = Self {
            uuid: uuid.clone(),
            username: username.into(),
            image: None,
            status: "Loading skin…".into(),
            theme,
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
        v
    }
}

impl Render for SkinView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let content = match self.image.clone() {
            Some(image) => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(img(image).h(px(480.)))
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
            .child(content)
    }
}

pub fn open_skin(
    uuid: Option<String>,
    username: String,
    theme: Theme,
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
        move |_, cx| cx.new(|cx| SkinView::new(uuid.clone(), username.clone(), theme, cx)),
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
