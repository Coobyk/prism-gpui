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
    Logs,
    Version,
    Mods,
    ResourcePacks,
    DataPacks,
    TexturePacks,
    ShaderPacks,
    Notes,
    Worlds,
    Servers,
    Screenshots,
    Settings,
    CrashReports,
}

impl Tab {
    /// The pages of the instance window, in the order Prism lists them.
    const ALL: [Tab; 13] = [
        Tab::Logs,
        Tab::Version,
        Tab::Mods,
        Tab::ResourcePacks,
        Tab::DataPacks,
        Tab::TexturePacks,
        Tab::ShaderPacks,
        Tab::Notes,
        Tab::Worlds,
        Tab::Servers,
        Tab::Screenshots,
        Tab::Settings,
        Tab::CrashReports,
    ];
    fn label(&self) -> String {
        crate::i18n::tr(match self {
            Tab::Logs => "Minecraft Log",
            Tab::Version => "Version",
            Tab::Mods => "Mods",
            Tab::ResourcePacks => "Resource Packs",
            Tab::DataPacks => "Data Packs",
            Tab::TexturePacks => "Texture packs",
            Tab::ShaderPacks => "Shader Packs",
            Tab::Notes => "Notes",
            Tab::Worlds => "Worlds",
            Tab::Servers => "Servers",
            Tab::Screenshots => "Screenshots",
            Tab::Settings => "Settings",
            Tab::CrashReports => "Other Logs",
        })
    }
    /// The icon Prism shows next to the page.
    fn icon(&self) -> &'static str {
        match self {
            Tab::Logs | Tab::CrashReports => "console",
            Tab::Version => "instance",
            Tab::Mods => "mod",
            Tab::ResourcePacks | Tab::TexturePacks => "picture",
            Tab::DataPacks => "mod",
            Tab::ShaderPacks => "wand",
            Tab::Notes => "notes",
            Tab::Worlds => "world",
            Tab::Servers => "servers",
            Tab::Screenshots => "screenshot",
            Tab::Settings => "settings",
        }
    }
    fn subdir(&self) -> &'static [&'static str] {
        match self {
            Tab::Mods => &["mods"],
            Tab::ResourcePacks => &["resourcepacks"],
            Tab::ShaderPacks => &["shaderpacks"],
            Tab::DataPacks => &["datapacks"],
            Tab::TexturePacks => &["texturepacks"],
            Tab::Worlds => &["saves"],
            Tab::Screenshots => &["screenshots"],
            Tab::Logs => &["logs"],
            Tab::CrashReports => &["crash-reports"],
            // The remaining pages are not folders.
            Tab::Notes | Tab::Version | Tab::Servers | Tab::Settings => &[],
        }
    }
    /// Pages that show something else than a file list.
    fn is_special(&self) -> bool {
        matches!(
            self,
            Tab::Notes | Tab::Version | Tab::Servers | Tab::Settings
        )
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
    /// The instance's notes, edited in the Notes tab.
    notes: String,
    /// The components the instance is made of (the Version tab).
    components: Vec<(String, String, String)>,
    /// The text editor used by the Notes tab.
    notes_input: Entity<TextInput>,
    /// The metadata of the selected mod.
    mod_info: Option<crate::modinfo::ModInfo>,
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
            tab: Tab::Logs,
            entries: Vec::new(),
            status: "Ready".into(),
            theme,
            selected: 0,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::default(),
            auto_focused: false,
            pending_delete: false,
            notes: String::new(),
            components: Vec::new(),
            notes_input: cx.new(|cx| {
                let mut t = TextInput::new("Notes", cx);
                t.set_theme(theme, cx);
                t
            }),
            mod_info: None,
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
            Tab::Mods | Tab::ResourcePacks | Tab::ShaderPacks | Tab::DataPacks
                if !entry.ends_with('/') =>
            {
                self.toggle_mod(&entry);
                cx.notify();
            }
            _ => {}
        }
    }

    /// The entry that is selected in the list.
    fn selected_entry(&self) -> Option<String> {
        self.entries.get(self.selected).cloned()
    }

    /// Read the metadata of the selected mod.
    fn load_mod_info(&mut self) {
        self.mod_info = None;
        if self.tab != Tab::Mods {
            return;
        }
        let Some(entry) = self.selected_entry() else {
            return;
        };
        if entry.ends_with('/') {
            return;
        }
        let path = self.current_dir().join(&entry);
        self.mod_info = crate::modinfo::read_mod_info(&path);
    }

    /// Open the folder of the selected entry.
    fn open_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let path = self.current_dir().join(&entry);
        let target = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(&self.current_dir()).to_path_buf()
        };
        let _ = crate::desktop::open_path(&target);
        self.status = format!("Opened {}", target.display()).into();
        cx.notify();
    }

    /// Delete the selected entry.
    fn delete_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let path = self.current_dir().join(&entry);
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|e| e.to_string())
        } else {
            std::fs::remove_file(&path).map_err(|e| e.to_string())
        };
        self.status = match result {
            Ok(()) => format!("Removed {entry}").into(),
            Err(e) => format!("Could not remove {entry}: {e}").into(),
        };
        self.refresh();
        cx.notify();
    }

    /// Rename the selected world.
    fn rename_selected_world(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        if self.tab != Tab::Worlds {
            return;
        }
        // The new name is asked for in a small window, like Prism's dialog.
        let world = entry.trim_end_matches('/').to_string();
        crate::ui::open_rename_world(
            self.dir.clone(),
            world,
            self.data_root.clone(),
            self.theme,
            cx,
        );
    }

    /// Pick the icon of the instance.
    fn choose_icon(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select an instance icon".into()),
        });
        let dir = self.dir.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.first().cloned() else {
                return;
            };
            let target = dir.join("icon.png");
            let copied = std::fs::copy(&path, &target);
            this.update(cx, |this, cx| {
                match copied {
                    Ok(_) => {
                        if let Some(mut inst) = crate::instance::Instance::load(&this.dir) {
                            inst.set("IconKey", "icon.png");
                            let _ = inst.save();
                        }
                        this.status = "The icon was changed".into();
                    }
                    Err(e) => this.status = format!("Could not copy the icon: {e}").into(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Enable or disable a mod, resource pack, shader or data pack.
    ///
    /// Minecraft skips the files that end in `.disabled`, which is how Prism
    /// disables them too.
    fn toggle_mod(&mut self, file: &str) {
        let dir = self.current_dir().join(file);
        let target = if file.ends_with(".disabled") {
            dir.with_file_name(file.trim_end_matches(".disabled"))
        } else {
            dir.with_file_name(format!("{file}.disabled"))
        };
        let _ = std::fs::rename(&dir, &target);
        // The first enabled resource pack and shader become the active ones.
        if matches!(self.tab, Tab::ResourcePacks | Tab::ShaderPacks) {
            self.update_active_pack();
        }
        self.refresh();
    }

    /// Write the active resource pack and shader into `options.txt`.
    ///
    /// Minecraft reads `resourcePacks:` and `shaderPack:` from the game
    /// directory, so the first enabled pack of each kind is used.
    fn update_active_pack(&mut self) {
        let options = self.dir.join("minecraft").join("options.txt");
        let Ok(text) = std::fs::read_to_string(&options) else {
            return;
        };
        let resource_pack = self
            .first_enabled("resourcepacks")
            .map(|n| format!("resourcePacks:[\"{n}\"]"))
            .unwrap_or_else(|| "resourcePacks:[]".to_string());
        let shader = self
            .first_enabled("shaderpacks")
            .map(|n| format!("shaderPack:\"{n}\""))
            .unwrap_or_else(|| "shaderPack:\"\"".to_string());
        let mut lines: Vec<String> = text
            .lines()
            .filter(|l| !l.starts_with("resourcePacks:") && !l.starts_with("shaderPack:"))
            .map(|l| l.to_string())
            .collect();
        lines.push(resource_pack);
        lines.push(shader);
        let _ = std::fs::write(
            &options,
            lines.join(
                "
",
            ) + "
",
        );
    }

    /// The first file in a folder that is not disabled.
    fn first_enabled(&self, folder: &str) -> Option<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.dir.join("minecraft").join(folder))
            .ok()?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| !n.ends_with(".disabled"))
            .collect();
        names.sort();
        names.into_iter().next()
    }

    fn current_dir(&self) -> PathBuf {
        let mut p = self.dir.join("minecraft");
        for s in self.tab.subdir() {
            p = p.join(s);
        }
        p
    }

    pub fn refresh(&mut self) {
        // The notes and the version list are not file listings.
        if self.tab == Tab::Notes {
            let notes = crate::instance::Instance::load(&self.dir)
                .and_then(|i| i.settings.get("notes").cloned())
                .unwrap_or_default();
            if notes != self.notes {
                self.notes = notes.clone();
            }
            self.status = "Notes are stored in the instance settings".into();
            return;
        }
        if self.tab == Tab::Version {
            self.components = crate::meta::read_component_pack(&self.dir)
                .map(|p| {
                    p.components
                        .into_iter()
                        .map(|c| {
                            let label = crate::metaindex::component_label(&c.uid).to_string();
                            (c.uid, c.version, label)
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.status = format!("{} components", self.components.len()).into();
            return;
        }
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
        if self.tab.is_special() {
            self.status = self.tab.label().into();
        } else {
            self.status = format!("{} in {}", self.entries.len(), dir.display()).into();
        }
        self.load_mod_info();
    }

    /// Load the notes of the instance into the editor.
    fn load_notes(&mut self, cx: &mut Context<Self>) {
        let notes = crate::instance::Instance::load(&self.dir)
            .and_then(|i| i.settings.get("notes").cloned())
            .unwrap_or_default();
        self.notes = notes.clone();
        self.notes_input.update(cx, |t, cx| t.set_text(notes, cx));
    }

    /// Store the notes in the instance settings, like Prism's `BaseInstance`.
    fn save_notes(&mut self, cx: &mut Context<Self>) {
        let text = self.notes_input.read(cx).text().to_string();
        self.notes = text.clone();
        let mut inst = match crate::instance::Instance::load(&self.dir) {
            Some(i) => i,
            None => {
                self.status = "Could not load instance.cfg".into();
                cx.notify();
                return;
            }
        };
        inst.set("notes", &text);
        match inst.save() {
            Ok(()) => self.status = "Notes saved".into(),
            Err(e) => self.status = format!("Could not save the notes: {e}").into(),
        }
        cx.notify();
    }

    /// The Notes tab: an editor for the instance's notes.
    fn render_notes(&mut self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
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
                    .h(px(40.0))
                    .px_4()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(self.name.clone())
                    .child(
                        div()
                            .id("save-notes")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.accent))
                            .text_color(rgb(theme.on_accent))
                            .cursor_pointer()
                            .child(crate::i18n::tr("Save"))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.save_notes(cx);
                            })),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .text_sm()
                            .text_color(rgb(theme.subtext))
                            .child(self.status.clone()),
                    ),
            )
            .child(div().flex_1().p_3().child(self.notes_input.clone()))
            .into_any_element()
    }

    /// The Servers page: the servers of the instance's `servers.dat`.
    fn render_servers(&mut self, theme: Theme, _cx: &mut Context<Self>) -> AnyElement {
        let servers = crate::servers::list_servers(&self.dir);
        let rows: Vec<AnyElement> = servers
            .iter()
            .map(|server| {
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
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_base().child(server.name.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.subtext))
                                    .child(server.ip.clone()),
                            ),
                    )
                    .into_any_element()
            })
            .collect();

        div()
            .flex_1()
            .flex()
            .flex_col()
            .overflow_hidden()
            .children(rows)
            .into_any_element()
    }

    /// The Version tab: the components the instance is made of.
    fn render_version(&mut self, theme: Theme, _cx: &mut Context<Self>) -> AnyElement {
        let rows: Vec<AnyElement> = self
            .components
            .iter()
            .map(|(uid, version, label)| {
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
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_base().child(label.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.subtext))
                                    .child(uid.clone()),
                            ),
                    )
                    .child(div().text_sm().child(version.clone()))
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
                    .h(px(40.0))
                    .px_4()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(self.name.clone())
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
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .children(rows),
            )
            .into_any_element()
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

        // The page list on the left, like Prism's `PageView`.
        let pages: Vec<AnyElement> = Tab::ALL
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let selected = *t == self.tab;
                let t2 = *t;
                div()
                    .id(("page", i))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(26.0))
                    .px_3()
                    .cursor_pointer()
                    .bg(if selected {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.base)
                    })
                    .text_color(if selected {
                        rgb(theme.on_accent)
                    } else {
                        rgb(theme.text)
                    })
                    .hover(|style| style.bg(theme.rgba(theme.accent, 0.25)))
                    // The page icon Prism shows next to the name.
                    .child(
                        match crate::icons::icon(
                            t.icon(),
                            if selected {
                                theme.on_accent
                            } else {
                                theme.text
                            },
                        ) {
                            Some(image) => {
                                gpui::img(image).w(px(16.)).h(px(16.)).into_any_element()
                            }
                            None => div().w(px(16.)).h(px(16.)).into_any_element(),
                        },
                    )
                    .child(div().text_sm().child(t.label()))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.tab = t2;
                        if t2 == Tab::Notes {
                            this.load_notes(cx);
                        }
                        if t2 == Tab::Settings
                            && let Some(inst) = crate::instance::Instance::load(&this.dir)
                        {
                            crate::ui::open_instance_settings(this.dir.clone(), inst, theme, cx);
                        }
                        this.refresh();
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();

        // The content of the selected page.
        let content: AnyElement = match self.tab {
            Tab::Notes => self.render_notes(theme, cx),
            Tab::Version => self.render_version(theme, cx),
            Tab::Servers => self.render_servers(theme, cx),
            _ => {
                // Virtualised list: only the visible slice of entries is built.
                let tab = self.tab;
                let entries = self.entries.clone();
                let selected = self.selected;
                let entity = cx.entity();
                if entries.is_empty() {
                    div()
                        .p_3()
                        .text_color(rgb(theme.subtext))
                        .child(crate::i18n::tr("(empty)"))
                        .into_any_element()
                } else {
                    uniform_list(
                        "instance-entries",
                        entries.len(),
                        move |range, _window, _cx| {
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
                        },
                    )
                    .track_scroll(self.scroll.clone())
                    .flex_1()
                    .into_any_element()
                }
            }
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .track_focus(&self.focus)
            .on_key_down(
                cx.listener(move |this: &mut Self, ev: &KeyDownEvent, _window, cx| {
                    match ev.keystroke.key.as_str() {
                        "up" => this.move_selection(-1, cx),
                        "down" => this.move_selection(1, cx),
                        "left" => this.cycle_tab(-1, cx),
                        "right" => this.cycle_tab(1, cx),
                        "enter" => this.activate_selected(cx),
                        "l" => {
                            let java = crate::java::ensure_for_instance(&this.data_root, &this.dir);
                            if let Ok(java) = java {
                                let account = crate::accounts::account_for_instance(
                                    &this.data_root,
                                    &this.dir,
                                );
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
                }),
            )
            // ---- the page list and the page content -----------------------
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("page-list")
                            .w(px(crate::theme::metrics::PAGE_LIST_WIDTH))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .py_1()
                            .bg(rgb(theme.base))
                            .border_r_1()
                            .border_color(rgb(theme.border))
                            .children(pages),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            // The bold page title Prism puts above the page.
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .h(px(30.0))
                                    .px_4()
                                    .text_lg()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(self.tab.label()),
                            )
                            .child(content),
                    ),
            )
            // ---- the buttons at the bottom --------------------------------
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(40.0))
                    .px_4()
                    .bg(rgb(theme.surface))
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    // The metadata of the selected mod.
                    .child(match &self.mod_info {
                        Some(info) => div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px_4()
                            .py_2()
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().text_sm().child(info.name.clone()))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(theme.subtext))
                                            .child(info.summary()),
                                    ),
                            )
                            .into_any_element(),
                        None => div().into_any_element(),
                    })
                    // The actions for the selected entry.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("entry-open-folder")
                                    .px_3()
                                    .py_1()
                                    .cursor_pointer()
                                    .text_color(rgb(theme.text))
                                    .child(crate::i18n::tr("Open folder"))
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.open_selected_entry(cx)
                                    })),
                            )
                            .child(
                                div()
                                    .id("entry-rename")
                                    .px_3()
                                    .py_1()
                                    .cursor_pointer()
                                    .text_color(rgb(theme.text))
                                    .child(crate::i18n::tr("Rename"))
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.rename_selected_world(cx)
                                    })),
                            )
                            .child(
                                div()
                                    .id("entry-icon")
                                    .px_3()
                                    .py_1()
                                    .cursor_pointer()
                                    .text_color(rgb(theme.text))
                                    .child(crate::i18n::tr("Icon"))
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.choose_icon(cx)
                                    })),
                            )
                            .child(
                                div()
                                    .id("entry-delete")
                                    .px_3()
                                    .py_1()
                                    .cursor_pointer()
                                    .text_color(rgb(theme.text))
                                    .child(crate::i18n::tr("Delete"))
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.delete_selected_entry(cx)
                                    })),
                            ),
                    )
                    .child(crate::ui::tool_button(
                        "help",
                        &crate::i18n::tr("Help"),
                        theme,
                        move |_window, _cx| {
                            let _ = crate::desktop::open_url("https://prismlauncher.org/wiki/");
                        },
                    ))
                    .child(
                        div()
                            .ml_auto()
                            .text_sm()
                            .text_color(rgb(theme.subtext))
                            .child(self.status.clone()),
                    )
                    .child(
                        div()
                            .id("instance-launch")
                            .px_3()
                            .py_1()
                            .cursor_pointer()
                            .text_color(rgb(theme.text))
                            .child(crate::i18n::tr("Launch"))
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                match crate::java::ensure_for_instance(&this.data_root, &this.dir) {
                                    Ok(java) => {
                                        let account = crate::accounts::account_for_instance(
                                            &this.data_root,
                                            &this.dir,
                                        );
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
                            .id("instance-restart")
                            .px_3()
                            .py_1()
                            .cursor_pointer()
                            .text_color(rgb(theme.text))
                            .child(crate::i18n::tr("Restart")),
                    )
                    .child(
                        div()
                            .id("instance-kill")
                            .px_3()
                            .py_1()
                            .cursor_pointer()
                            .text_color(rgb(theme.text))
                            .child(crate::i18n::tr("Kill")),
                    )
                    .child(
                        div()
                            .id("instance-close")
                            .px_3()
                            .py_1()
                            .cursor_pointer()
                            .text_color(rgb(theme.text))
                            .child(crate::i18n::tr("Close"))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.quit();
                            })),
                    ),
            )
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
        .bg(if selected {
            rgb(theme.surface)
        } else {
            rgb(theme.background)
        })
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
pub fn open_instance(
    name: &str,
    dir: &Path,
    data_root: &Path,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<InstanceView>> {
    let name = name.to_string();
    let dir = dir.to_path_buf();
    let data_root = data_root.to_path_buf();
    let bounds = Bounds::centered(None, size(px(800.), px(600.)), cx);
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(format!("Console window for {name}").into()),
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

/// The pages of the "New Instance" dialog, in the order Prism lists them.
const NEW_INSTANCE_PAGES: &[&str] = &[
    "Custom",
    "Import",
    "Modrinth",
    "CurseForge",
    "FTB",
    "ATLauncher",
    "Technic",
];

pub struct NewInstanceView {
    /// The page that is shown.
    page: usize,
    /// The name of the new instance.
    name_input: Entity<TextInput>,
    /// The Minecraft version of the new instance.
    version_input: Entity<TextInput>,
    /// The search field of the platform pages.
    search_input: Entity<TextInput>,
    /// The modpacks the platform pages found.
    hits: Vec<(String, String, String)>,
    /// The platform the results come from.
    platform: Option<crate::modplatform::Platform>,
    status: SharedString,
    theme: Theme,
    data_root: PathBuf,
    focus: FocusHandle,
    auto_focused: bool,
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
        let search_input = cx.new(|cx| {
            let mut t = TextInput::new("Search for a modpack", cx);
            t.set_theme(theme, cx);
            t
        });
        Self {
            page: 0,
            name_input,
            version_input,
            search_input,
            hits: Vec::new(),
            platform: None,
            status: "Ready".into(),
            theme,
            data_root,
            focus: cx.focus_handle(),
            auto_focused: false,
        }
    }

    /// Create a vanilla instance with the name and the version of the form.
    fn create(&mut self, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).text().trim().to_string();
        let version = self.version_input.read(cx).text().trim().to_string();
        if name.is_empty() || version.is_empty() {
            self.status = "Name and version are required".into();
            cx.notify();
            return;
        }
        match create_instance(&self.data_root, &name, &version) {
            Ok(dir) => {
                self.status = format!("Created {}", dir.display()).into();
                cx.quit();
            }
            Err(e) => self.status = format!("Error: {e}").into(),
        }
        cx.notify();
    }

    /// Import a pack archive into a new instance.
    fn import(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select a modpack or an instance archive".into()),
        });
        let root = self.data_root.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.first().cloned() else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { crate::pack::import_archive(&path, &root, None) })
                .await;
            this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(result) => format!("Imported {} as {}", result.summary, result.name).into(),
                    Err(e) => format!("Import failed: {e}").into(),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Search a platform for modpacks.
    fn search(&mut self, cx: &mut Context<Self>) {
        let Some(platform) = self.platform else {
            return;
        };
        let query = self.search_input.read(cx).text().trim().to_string();
        if query.is_empty() {
            self.status = "Enter a query".into();
            cx.notify();
            return;
        }
        self.status = format!("Searching {}…", platform.label()).into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    crate::modplatform::platform::search(
                        platform,
                        crate::modplatform::ContentKind::Modpack,
                        &crate::modplatform::SearchQuery {
                            text: query,
                            loader: None,
                            game_version: None,
                        },
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(hits) => {
                        this.hits = hits.into_iter().map(|h| (h.title, h.id, h.slug)).collect();
                        this.status = format!("{} results", this.hits.len()).into();
                    }
                    Err(e) => this.status = format!("Search failed: {e}").into(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Install the selected modpack as a new instance.
    fn install(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(platform) = self.platform else {
            return;
        };
        let Some((title, id, slug)) = self.hits.get(index).cloned() else {
            return;
        };
        let root = self.data_root.clone();
        self.status = format!("Installing {title}…").into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let message = cx
                .background_executor()
                .spawn(async move {
                    let instances_root = crate::instance::instances_dir(&root);
                    let dir_name = slug
                        .chars()
                        .map(|c| {
                            if c.is_ascii_alphanumeric() || c == '-' {
                                c
                            } else {
                                '_'
                            }
                        })
                        .collect::<String>();
                    let inst_dir = instances_root.join(&dir_name);
                    if inst_dir.exists() {
                        return format!("instance already exists: {dir_name}");
                    }
                    if let Err(e) = std::fs::create_dir_all(inst_dir.join("minecraft")) {
                        return format!("mkdir failed: {e}");
                    }
                    let cfg = format!(
                        "[General]\nInstanceType=OneSix\nConfigVersion=1.3\nname={dir_name}\n"
                    );
                    let _ = std::fs::write(inst_dir.join("instance.cfg"), cfg);
                    let hit = crate::modplatform::PlatformHit {
                        platform,
                        kind: crate::modplatform::ContentKind::Modpack,
                        id,
                        slug: dir_name.clone(),
                        title: title.clone(),
                        description: String::new(),
                        downloads: 0,
                        extra: String::new(),
                    };
                    match crate::modplatform::platform::install(&hit, &inst_dir, None, None) {
                        Ok(outcome) => {
                            for (uid, version) in &outcome.components {
                                let _ = crate::meta::set_component(&inst_dir, uid, version);
                            }
                            outcome.message
                        }
                        Err(e) => format!("Install failed: {e}"),
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.status = message.into();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for NewInstanceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        if !self.auto_focused {
            self.focus.focus(window);
            self.auto_focused = true;
        }
        let page_name = NEW_INSTANCE_PAGES
            .get(self.page)
            .copied()
            .unwrap_or("Custom");

        // The page list on the left, like Prism's new instance dialog.
        let pages: Vec<AnyElement> = NEW_INSTANCE_PAGES
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let selected = i == self.page;
                div()
                    .id(gpui::ElementId::Name(SharedString::from(format!(
                        "new-page-{p}"
                    ))))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(32.0))
                    .px_3()
                    .cursor_pointer()
                    .bg(if selected {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.background)
                    })
                    .text_color(if selected {
                        rgb(theme.on_accent)
                    } else {
                        rgb(theme.text)
                    })
                    .hover(|style| style.bg(rgb(theme.alternate)))
                    .child(div().w(px(24.)).h(px(24.)).rounded_md().bg(if selected {
                        rgb(theme.on_accent)
                    } else {
                        rgb(theme.alternate)
                    }))
                    .child(div().text_sm().child(crate::i18n::tr(p)))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.page = i;
                        // The platform pages need to know their platform.
                        this.platform = match *p {
                            "Modrinth" => Some(crate::modplatform::Platform::Modrinth),
                            "CurseForge" => Some(crate::modplatform::Platform::CurseForge),
                            "FTB" => Some(crate::modplatform::Platform::Ftb),
                            "ATLauncher" => Some(crate::modplatform::Platform::AtLauncher),
                            "Technic" => Some(crate::modplatform::Platform::Technic),
                            _ => None,
                        };
                        this.hits.clear();
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();

        // The content of the selected page.
        let content: AnyElement = match page_name {
            "Custom" => div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .child(div().text_sm().child(crate::i18n::tr("Instance name")))
                .child(div().w(px(360.)).child(self.name_input.clone()))
                .child(div().text_sm().child(crate::i18n::tr("Minecraft version")))
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
                        .child(crate::i18n::tr("Create"))
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| this.create(cx))),
                )
                .into_any_element(),
            "Import" => div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .child(div().text_sm().child(crate::i18n::tr(
                    "Import a .mrpack, a CurseForge pack or a Prism instance archive.",
                )))
                .child(
                    div()
                        .id("import-instance")
                        .px_3()
                        .py_1()
                        .w(px(200.))
                        .rounded_md()
                        .bg(rgb(theme.accent))
                        .text_color(rgb(theme.on_accent))
                        .cursor_pointer()
                        .child(crate::i18n::tr("Choose a file…"))
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| this.import(cx))),
                )
                .into_any_element(),
            _ => {
                // The platform pages: a search field and the modpacks it found.
                let hits = self.hits.clone();
                let entity = cx.entity();
                let list: AnyElement = if hits.is_empty() {
                    div()
                        .p_4()
                        .text_color(rgb(theme.subtext))
                        .child(crate::i18n::tr("No results yet."))
                        .into_any_element()
                } else {
                    div()
                        .id("new-hit-list")
                        .flex_1()
                        .flex()
                        .flex_col()
                        .overflow_y_scroll()
                        .children(hits.iter().enumerate().map(|(i, (title, _, slug))| {
                            let entity = entity.clone();
                            div()
                                .id(gpui::ElementId::Name(SharedString::from(format!(
                                    "new-hit-{slug}"
                                ))))
                                .flex()
                                .items_center()
                                .justify_between()
                                .px_3()
                                .py_2()
                                .border_b_1()
                                .border_color(rgb(theme.border))
                                .cursor_pointer()
                                .hover(|style| style.bg(rgb(theme.alternate)))
                                .child(div().text_sm().child(title.clone()))
                                .child(
                                    div()
                                        .id(gpui::ElementId::Name(SharedString::from(format!(
                                            "new-install-{slug}"
                                        ))))
                                        .px_3()
                                        .py_1()
                                        .rounded_md()
                                        .bg(rgb(theme.accent))
                                        .text_color(rgb(theme.on_accent))
                                        .cursor_pointer()
                                        .child(crate::i18n::tr("Install"))
                                        .on_click(move |_, _, cx| {
                                            entity.update(cx, |this, cx| this.install(i, cx));
                                        }),
                                )
                                .into_any_element()
                        }))
                        .into_any_element()
                };
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().w(px(360.)).child(self.search_input.clone()))
                            .child(
                                div()
                                    .id("new-search")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(theme.accent))
                                    .text_color(rgb(theme.on_accent))
                                    .cursor_pointer()
                                    .child(crate::i18n::tr("Search"))
                                    .on_click(
                                        cx.listener(|this: &mut Self, _, _, cx| this.search(cx)),
                                    ),
                            ),
                    )
                    .child(list)
                    .into_any_element()
            }
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .track_focus(&self.focus)
            .on_key_down(
                cx.listener(|this: &mut Self, ev: &KeyDownEvent, _window, cx| {
                    let pages = NEW_INSTANCE_PAGES;
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
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("new-instance-pages")
                            .w(px(crate::theme::metrics::PAGE_LIST_WIDTH))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .py_1()
                            .bg(rgb(theme.base))
                            .border_r_1()
                            .border_color(rgb(theme.border))
                            .children(pages),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .h(px(30.0))
                                    .px_4()
                                    .text_lg()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(crate::i18n::tr(page_name)),
                            )
                            .child(content),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(40.0))
                    .px_4()
                    .bg(rgb(theme.surface))
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme.subtext))
                            .child(self.status.clone()),
                    )
                    .child(
                        div()
                            .id("new-instance-close")
                            .ml_auto()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .child(crate::i18n::tr("Close"))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.quit();
                            })),
                    ),
            )
    }
}

// ---- Rename instance window ----

/// The rename dialog, mirroring Prism's inline rename editor.
pub struct RenameInstanceView {
    name_input: Entity<TextInput>,
    status: SharedString,
    theme: Theme,
    data_root: PathBuf,
    old_name: String,
}

impl RenameInstanceView {
    pub fn new(data_root: PathBuf, old_name: String, theme: Theme, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| {
            let mut t = TextInput::new("Instance name", cx);
            t.set_text(old_name.clone(), cx);
            t.set_theme(theme, cx);
            t
        });
        Self {
            name_input,
            status: "Ready".into(),
            theme,
            data_root,
            old_name,
        }
    }

    fn rename(&mut self, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).text().trim().to_string();
        if name.is_empty() || name == self.old_name {
            self.status = "The name did not change".into();
            cx.notify();
            return;
        }
        match crate::instance::rename_instance(&self.data_root, &self.old_name, &name) {
            Ok(dir) => {
                self.status = format!("Renamed to {}", dir.display()).into();
                cx.quit();
            }
            Err(e) => self.status = format!("Rename failed: {e}").into(),
        }
        cx.notify();
    }
}

impl Render for RenameInstanceView {
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
            .child(div().text_lg().child(crate::i18n::tr("Rename Instance")))
            .child(div().w(px(360.)).child(self.name_input.clone()))
            .child(
                div()
                    .id("rename-instance")
                    .px_3()
                    .py_1()
                    .w(px(120.))
                    .rounded_md()
                    .bg(rgb(theme.accent))
                    .text_color(rgb(theme.on_accent))
                    .cursor_pointer()
                    .child(crate::i18n::tr("Rename"))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.rename(cx))),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.subtext))
                    .child(self.status.clone()),
            )
    }
}

/// Open the rename dialog for an instance.
pub fn open_rename_instance(
    data_root: PathBuf,
    name: String,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<RenameInstanceView>> {
    let bounds = Bounds::centered(None, size(px(420.), px(220.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some(format!("Rename {name} — Prism Launcher").into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| {
            cx.new(|cx| RenameInstanceView::new(data_root.clone(), name.clone(), theme, cx))
        },
    )
    .ok()
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

use crate::settings::{GlobalSettings, SETTINGS, ValueKind};

pub struct SettingsView {
    settings: GlobalSettings,
    /// The data root, used to find the installed Java versions.
    data_root: PathBuf,
    page: usize,
    /// Text inputs for every text/path/secret setting.
    inputs: Vec<(&'static crate::settings::SettingDef, Entity<TextInput>)>,
    status: SharedString,
    theme: Theme,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
}

impl SettingsView {
    pub fn new(
        settings: GlobalSettings,
        data_root: PathBuf,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
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
            data_root,
            page: 0,
            inputs,
            status: "Changes are saved when you press Save".into(),
            theme,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::default(),
        }
    }

    /// Cycle an enum setting through its allowed values.
    fn cycle_enum(
        &mut self,
        key: &'static str,
        options: &'static [&'static str],
        cx: &mut Context<Self>,
    ) {
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
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(path) = paths.first()
            {
                let text = path.display().to_string();
                this.update(cx, |this, cx| {
                    this.settings.set(key, &text);
                    this.sync_input(key, &text, cx);
                    cx.notify();
                })
                .ok();
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
        let all_pages = crate::settings::PAGES;
        let page_name = all_pages.get(self.page).copied().unwrap_or("General");

        // The page list on the left, like Prism's settings dialog.
        let page_list: Vec<AnyElement> = all_pages
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let selected = i == self.page;
                div()
                    .id(("settings-page", i))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(26.0))
                    .px_3()
                    .cursor_pointer()
                    .bg(if selected {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.base)
                    })
                    .text_color(if selected {
                        rgb(theme.on_accent)
                    } else {
                        rgb(theme.text)
                    })
                    .hover(|style| style.bg(theme.rgba(theme.accent, 0.25)))
                    .child(div().w(px(24.)).h(px(24.)).rounded_md().bg(if selected {
                        rgb(theme.on_accent)
                    } else {
                        rgb(theme.alternate)
                    }))
                    .child(div().text_sm().child(crate::i18n::tr(p)))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.page = i;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();

        // One row per setting; virtualised so long pages scroll.
        let rows: Vec<(
            &'static crate::settings::SettingDef,
            Option<Entity<TextInput>>,
            String,
        )> = crate::settings::SETTINGS
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
        // The Java page lists the Java installations that were found.
        let java_list: AnyElement = if page_name == "Java" {
            let installs = crate::java::discover_java(&self.data_root);
            let java_entity = cx.entity();
            if installs.is_empty() {
                div()
                    .p_3()
                    .text_sm()
                    .text_color(rgb(theme.subtext))
                    .child(crate::i18n::tr(
                        "No Java installation was found. The launcher can download one.",
                    ))
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(crate::i18n::tr("Java installations")),
                    )
                    .children(installs.iter().map(|install| {
                        let path = install.path.display().to_string();
                        let version = install.version.clone();
                        let path_use = path.clone();
                        let java_entity = java_entity.clone();
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .py_1()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().text_sm().child(version))
                                    .child(
                                        div().text_xs().text_color(rgb(theme.subtext)).child(path),
                                    ),
                            )
                            .child(
                                div()
                                    .id(gpui::ElementId::Name(SharedString::from(format!(
                                        "java-use-{path_use}"
                                    ))))
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(theme.surface))
                                    .border_1()
                                    .border_color(rgb(theme.border))
                                    .child(crate::i18n::tr("Use"))
                                    .on_click(move |_, _, cx| {
                                        let path = path_use.clone();
                                        java_entity.update(cx, |this, cx| {
                                            this.settings.set("JavaPath", &path);
                                            this.status = format!("Using {path}").into();
                                            cx.notify();
                                        });
                                    }),
                            )
                    }))
                    .into_any_element()
            }
        } else {
            div().into_any_element()
        };

        let entity = cx.entity();
        let entity_rows = entity.clone();
        let list = uniform_list("settings-rows", rows.len(), move |range, _window, _cx| {
            range
                .filter_map(|i| {
                    let (def, input, value) = rows.get(i)?.clone();
                    Some(settings_row(def, input, value, theme, entity_rows.clone()))
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
            .on_key_down(
                cx.listener(|this: &mut Self, ev: &KeyDownEvent, _window, cx| {
                    let pages = crate::settings::PAGES;
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
                }),
            )
            // The page list and the page content.
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("settings-page-list")
                            .w(px(crate::theme::metrics::PAGE_LIST_WIDTH))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .py_1()
                            .bg(rgb(theme.base))
                            .border_r_1()
                            .border_color(rgb(theme.border))
                            .children(page_list),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .h(px(30.0))
                                    .px_4()
                                    .text_lg()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(crate::i18n::tr(page_name)),
                            )
                            .child(java_list)
                            .child(list),
                    ),
            )
            // The buttons at the bottom, like Prism's settings dialog.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(40.0))
                    .px_4()
                    .bg(rgb(theme.surface))
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .child(crate::ui::tool_button(
                        "help",
                        &crate::i18n::tr("Help"),
                        theme,
                        move |_window, _cx| {
                            let _ = crate::desktop::open_url("https://prismlauncher.org/wiki/");
                        },
                    ))
                    .child(
                        div()
                            .ml_auto()
                            .text_sm()
                            .text_color(rgb(theme.subtext))
                            .child(self.status.clone()),
                    )
                    .child(
                        div()
                            .id("settings-ok")
                            .px_3()
                            .py_1()
                            .cursor_pointer()
                            .text_color(rgb(theme.text))
                            .child(crate::i18n::tr("OK"))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| this.save(cx))),
                    )
                    .child(
                        div()
                            .id("settings-cancel")
                            .px_3()
                            .py_1()
                            .cursor_pointer()
                            .text_color(rgb(theme.text))
                            .child(crate::i18n::tr("Cancel"))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.quit();
                            })),
                    ),
            )
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
    let label = div().w(px(240.)).flex_shrink_0().text_sm().child(def.label);
    let key = def.key;
    let control: AnyElement = match def.kind {
        ValueKind::Bool => {
            let on = matches!(
                value.to_ascii_lowercase().as_str(),
                "true" | "1" | "yes" | "on"
            );
            let e = entity.clone();
            div()
                .id(gpui::ElementId::Name(SharedString::from(format!(
                    "bool-{key}"
                ))))
                .w(px(20.))
                .h(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .child(crate::ui::checkbox(on, theme))
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
                .id(gpui::ElementId::Name(SharedString::from(format!(
                    "enum-{key}"
                ))))
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
                .id(gpui::ElementId::Name(SharedString::from(format!(
                    "browse-{key}"
                ))))
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
                    e.update(cx, |this, cx| this.browse_path(key, key != "JavaPath", cx));
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
        .id(gpui::ElementId::Name(SharedString::from(format!(
            "setting-{key}"
        ))))
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
    data_root: PathBuf,
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
        move |_, cx| cx.new(|cx| SettingsView::new(settings, data_root.clone(), theme, cx)),
    )
    .ok()
}

// ---- Instance settings window ----

pub struct InstanceSettingsView {
    dir: PathBuf,
    /// The instance's current settings, edited in place.
    settings: std::collections::BTreeMap<String, String>,
    /// Text inputs for the non-boolean settings.
    inputs: Vec<(&'static crate::settings::SettingDef, Entity<TextInput>)>,
    page: usize,
    status: SharedString,
    theme: Theme,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    auto_focused: bool,
}

impl InstanceSettingsView {
    pub fn new(
        dir: PathBuf,
        instance: &crate::instance::Instance,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut settings = instance.settings.clone();
        // Seed the defaults for known keys so the fields show sensible values.
        for def in crate::settings::INSTANCE_SETTINGS {
            settings
                .entry(def.key.to_string())
                .or_insert_with(|| def.default.to_string());
        }
        let inputs = crate::settings::INSTANCE_SETTINGS
            .iter()
            .filter(|d| d.kind != crate::settings::ValueKind::Bool)
            .map(|def| {
                let initial = settings
                    .get(def.key)
                    .cloned()
                    .unwrap_or_else(|| def.default.to_string());
                let input = cx.new(|cx| {
                    let mut t = TextInput::new(def.label, cx);
                    t.set_text(initial, cx);
                    t.set_theme(theme, cx);
                    t
                });
                (def, input)
            })
            .collect();
        Self {
            dir,
            settings,
            inputs,
            page: 0,
            status: "Changes are saved when you press Save".into(),
            theme,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::default(),
            auto_focused: false,
        }
    }

    /// Open a native file/directory picker for a path setting.
    fn browse_path(&mut self, key: &'static str, cx: &mut Context<Self>) {
        let files = key == "JavaPath";
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files,
            directories: !files,
            multiple: false,
            prompt: Some("Select a path".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(path) = paths.first()
            {
                let text = path.display().to_string();
                this.update(cx, |this, cx| {
                    this.settings.insert(key.to_string(), text.clone());
                    if let Some((_, input)) = this.inputs.iter().find(|(d, _)| d.key == key) {
                        input.update(cx, |t, cx| t.set_text(text.clone(), cx));
                    }
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        for (def, input) in &self.inputs {
            let value = input.read(cx).text().trim().to_string();
            self.settings.insert(def.key.to_string(), value);
        }
        let mut inst = match crate::instance::Instance::load(&self.dir) {
            Some(i) => i,
            None => {
                self.status = "Could not load instance.cfg".into();
                cx.notify();
                return;
            }
        };
        for (k, v) in &self.settings {
            inst.set(k, v);
        }
        match inst.save() {
            Ok(()) => self.status = "Saved".into(),
            Err(e) => self.status = format!("Save failed: {e}").into(),
        }
        cx.notify();
    }

    fn toggle_bool(&mut self, key: &'static str, cx: &mut Context<Self>) {
        let current = matches!(
            self.settings
                .get(key)
                .map(|s| s.as_str())
                .unwrap_or("false"),
            "true" | "1" | "yes" | "on"
        );
        self.settings.insert(
            key.to_string(),
            if current { "false" } else { "true" }.into(),
        );
        cx.notify();
    }
}

impl Render for InstanceSettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        if !self.auto_focused {
            self.focus.focus(window);
            self.auto_focused = true;
        }
        let pages = crate::settings::instance_pages();
        let page_name = pages.get(self.page).copied().unwrap_or("Java");

        let tabs: Vec<_> = pages
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let selected = i == self.page;
                div()
                    .id(("inst-page", i))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if selected {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.surface)
                    })
                    .text_color(if selected {
                        rgb(theme.on_accent)
                    } else {
                        rgb(theme.text)
                    })
                    .child(*p)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.page = i;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();

        // Virtualised rows: only the visible settings are built.
        let rows: Vec<(
            &'static crate::settings::SettingDef,
            Option<Entity<TextInput>>,
            String,
        )> = crate::settings::INSTANCE_SETTINGS
            .iter()
            .filter(|d| d.page == page_name)
            .map(|d| {
                let input = self
                    .inputs
                    .iter()
                    .find(|(x, _)| x.key == d.key)
                    .map(|(_, i)| i.clone());
                let value = self.settings.get(d.key).cloned().unwrap_or_default();
                (d, input, value)
            })
            .collect();
        let entity = cx.entity();
        let list = uniform_list(
            "instance-settings",
            rows.len(),
            move |range, _window, _cx| {
                range
                    .filter_map(|i| {
                        let (def, input, value) = rows.get(i)?.clone();
                        Some(instance_setting_row(
                            def,
                            input,
                            value,
                            theme,
                            entity.clone(),
                        ))
                    })
                    .collect::<Vec<_>>()
            },
        )
        .track_scroll(self.scroll.clone())
        .flex_1();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .track_focus(&self.focus)
            .on_key_down(
                cx.listener(|this: &mut Self, ev: &KeyDownEvent, _window, cx| {
                    let pages = crate::settings::instance_pages();
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
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .p_3()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(div().text_lg().child(crate::i18n::tr("Instance Settings")))
                    .child(
                        div()
                            .id("save-instance-settings")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.accent))
                            .text_color(rgb(theme.on_accent))
                            .cursor_pointer()
                            .child(crate::i18n::tr("Save"))
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

/// One row of the per-instance settings window.
fn instance_setting_row(
    def: &'static crate::settings::SettingDef,
    input: Option<Entity<TextInput>>,
    value: String,
    theme: Theme,
    entity: Entity<InstanceSettingsView>,
) -> AnyElement {
    use crate::settings::ValueKind;
    let label = div().w(px(240.)).flex_shrink_0().text_sm().child(def.label);
    let control: AnyElement = match def.kind {
        ValueKind::Bool => {
            let on = matches!(
                value.to_ascii_lowercase().as_str(),
                "true" | "1" | "yes" | "on"
            );
            let key = def.key;
            let e = entity.clone();
            div()
                .id(gpui::ElementId::Name(SharedString::from(format!(
                    "bool-{key}"
                ))))
                .px_3()
                .py_1()
                .w(px(80.))
                .rounded_md()
                .cursor_pointer()
                .bg(if on {
                    rgb(theme.accent)
                } else {
                    rgb(theme.surface)
                })
                .text_color(if on {
                    rgb(theme.on_accent)
                } else {
                    rgb(theme.text)
                })
                .border_1()
                .border_color(rgb(theme.border))
                .child(if on { "On" } else { "Off" })
                .on_click(move |_, _, cx| {
                    e.update(cx, |this, cx| this.toggle_bool(key, cx));
                })
                .into_any_element()
        }
        ValueKind::Path => {
            let e = entity.clone();
            let key = def.key;
            let browse = div()
                .id(gpui::ElementId::Name(SharedString::from(format!(
                    "browse-{key}"
                ))))
                .px_2()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border))
                .child("Browse…")
                .on_click(move |_, _, cx| {
                    e.update(cx, |this, cx| this.browse_path(key, cx));
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
        .id(gpui::ElementId::Name(SharedString::from(format!(
            "inst-setting-{}",
            def.key
        ))))
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(rgb(theme.border))
        .child(label)
        .child(control)
        .into_any_element()
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
                    .child(crate::i18n::tr("News"))
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
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .children(items),
            )
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
                    .bg(if is_sel {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.surface)
                    })
                    .text_color(if is_sel {
                        rgb(theme.on_accent)
                    } else {
                        rgb(theme.text)
                    })
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
                .child(if count == 0 {
                    "No screenshots"
                } else {
                    "Select a screenshot"
                })
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
                            this.image = Some(Arc::new(Image::from_bytes(ImageFormat::Png, bytes)));
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
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(path) = paths.first()
            {
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
                    .child(crate::i18n::tr("Upload skin…"))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.upload_skin(cx);
                    })),
            )
            .child(
                div()
                    .id("skin-model")
                    .px_3()
                    .py_1()
                    .bg(rgb(theme.surface))
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
                    .child(crate::i18n::tr("Add to library…"))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.install_local_skin(cx);
                    })),
            )
            .child(
                div()
                    .id("skin-refresh")
                    .px_3()
                    .py_1()
                    .bg(rgb(theme.surface))
                    .cursor_pointer()
                    .child(crate::i18n::tr("Refresh"))
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
                                .child(div().text_xs().text_color(rgb(theme.subtext)).child(
                                    format!(
                                        "{} · {}",
                                        skin.state,
                                        skin.variant.unwrap_or_default()
                                    ),
                                )),
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
                                .child(crate::i18n::tr("Use"))
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
                                .child(crate::i18n::tr("Upload"))
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

// ---- Group window ----

/// Manage the instance groups, like Prism's group dialog.
pub struct GroupView {
    data_root: PathBuf,
    /// The instance the dialog was opened for.
    instance: String,
    /// The groups and their members.
    groups: Vec<(String, Vec<String>)>,
    /// The group the instance is in.
    current: Option<String>,
    /// The name of the group that is being renamed.
    renaming: Option<String>,
    name_input: Entity<TextInput>,
    status: SharedString,
    theme: Theme,
}

impl GroupView {
    pub fn new(
        data_root: PathBuf,
        instance: String,
        current: Option<String>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| {
            let mut t = TextInput::new("Group name", cx);
            t.set_theme(theme, cx);
            t
        });
        let mut view = Self {
            data_root,
            instance,
            groups: Vec::new(),
            current,
            renaming: None,
            name_input,
            status: "Ready".into(),
            theme,
        };
        view.reload();
        view
    }

    fn reload(&mut self) {
        let store = crate::groups::GroupStore::load(&self.data_root);
        self.groups = store
            .groups
            .iter()
            .map(|(name, group)| (name.clone(), group.instances.clone()))
            .collect();
    }

    /// Move the instance into a group.
    fn assign(&mut self, group: Option<String>, cx: &mut Context<Self>) {
        let mut store = crate::groups::GroupStore::load(&self.data_root);
        store.set_group(&self.instance_name(), group.as_deref());
        let _ = store.save(&self.data_root);
        self.current = group;
        self.status = "The instance was moved".into();
        cx.notify();
    }

    /// The instance the dialog was opened for.
    fn instance_name(&self) -> String {
        self.instance.clone()
    }

    /// Create a group with the name of the input field.
    fn create(&mut self, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).text().trim().to_string();
        if name.is_empty() {
            self.status = "The name must not be empty".into();
            cx.notify();
            return;
        }
        let mut store = crate::groups::GroupStore::load(&self.data_root);
        let created = store.create_group(&name);
        let _ = store.save(&self.data_root);
        self.name_input.update(cx, |t, cx| t.set_text("", cx));
        self.reload();
        self.status = format!("Created the group {created}").into();
        cx.notify();
    }

    /// Rename the group that is being renamed.
    fn rename(&mut self, cx: &mut Context<Self>) {
        let Some(from) = self.renaming.clone() else {
            return;
        };
        let name = self.name_input.read(cx).text().trim().to_string();
        if name.is_empty() {
            self.status = "The name must not be empty".into();
            cx.notify();
            return;
        }
        let mut store = crate::groups::GroupStore::load(&self.data_root);
        match store.rename_group(&from, &name) {
            Ok(newname) => {
                let _ = store.save(&self.data_root);
                if self.current.as_deref() == Some(from.as_str()) {
                    self.current = Some(newname.clone());
                }
                self.renaming = None;
                self.name_input.update(cx, |t, cx| t.set_text("", cx));
                self.reload();
                self.status = format!("Renamed the group to {newname}").into();
            }
            Err(e) => self.status = format!("Rename failed: {e}").into(),
        }
        cx.notify();
    }

    /// Delete a group; its instances become ungrouped.
    fn delete(&mut self, name: &str, cx: &mut Context<Self>) {
        let mut store = crate::groups::GroupStore::load(&self.data_root);
        match store.delete_group(name) {
            Ok(()) => {
                let _ = store.save(&self.data_root);
                if self.current.as_deref() == Some(name) {
                    self.current = None;
                }
                self.reload();
                self.status = format!("Deleted the group {name}").into();
            }
            Err(e) => self.status = format!("Delete failed: {e}").into(),
        }
        cx.notify();
    }
}

impl Render for GroupView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let current = self.current.clone();
        let rows: Vec<AnyElement> = self
            .groups
            .iter()
            .map(|(name, members)| {
                let is_current = current.as_deref() == Some(name.as_str());
                let name = name.clone();
                let name_use = name.clone();
                let name_rename = name.clone();
                let name_delete = name.clone();
                let entity = cx.entity();
                let entity_use = entity.clone();
                let entity_rename = entity.clone();
                let entity_delete = entity.clone();
                div()
                    .id(gpui::ElementId::Name(SharedString::from(format!(
                        "group-{name}"
                    ))))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_base().child(name.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.subtext))
                                    .child(format!("{} instances", members.len())),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id(gpui::ElementId::Name(SharedString::from(format!(
                                        "group-use-{name_use}"
                                    ))))
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(if is_current {
                                        rgb(theme.accent)
                                    } else {
                                        rgb(theme.surface)
                                    })
                                    .text_color(if is_current {
                                        rgb(theme.on_accent)
                                    } else {
                                        rgb(theme.text)
                                    })
                                    .border_1()
                                    .border_color(rgb(theme.border))
                                    .child(if is_current { "Current" } else { "Use" })
                                    .on_click(move |_, _, cx| {
                                        let name = name_use.clone();
                                        entity_use
                                            .update(cx, |this, cx| this.assign(Some(name), cx));
                                    }),
                            )
                            .child(
                                div()
                                    .id(gpui::ElementId::Name(SharedString::from(format!(
                                        "group-rename-{name_rename}"
                                    ))))
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(theme.surface))
                                    .border_1()
                                    .border_color(rgb(theme.border))
                                    .child("Rename")
                                    .on_click(move |_, _, cx| {
                                        let name = name_rename.clone();
                                        entity_rename.update(cx, |this, cx| {
                                            this.renaming = Some(name.clone());
                                            this.name_input
                                                .update(cx, |t, cx| t.set_text(name, cx));
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                div()
                                    .id(gpui::ElementId::Name(SharedString::from(format!(
                                        "group-delete-{name_delete}"
                                    ))))
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(theme.surface))
                                    .border_1()
                                    .border_color(rgb(theme.border))
                                    .child("Delete")
                                    .on_click(move |_, _, cx| {
                                        let name = name_delete.clone();
                                        entity_delete.update(cx, |this, cx| this.delete(&name, cx));
                                    }),
                            ),
                    )
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
                    .h(px(40.0))
                    .px_4()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(crate::i18n::tr("Change Group")),
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
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .children(rows),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(40.0))
                    .px_4()
                    .bg(rgb(theme.surface))
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .child(div().w(px(240.)).child(self.name_input.clone()))
                    .child(
                        div()
                            .id("group-create")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.accent))
                            .text_color(rgb(theme.on_accent))
                            .cursor_pointer()
                            .child(crate::i18n::tr("Create"))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| this.create(cx))),
                    )
                    .child(
                        div()
                            .id("group-rename")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .child(crate::i18n::tr("Rename"))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| this.rename(cx))),
                    )
                    .child(
                        div()
                            .id("group-none")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .child(crate::i18n::tr("No group"))
                            .on_click(
                                cx.listener(|this: &mut Self, _, _, cx| this.assign(None, cx)),
                            ),
                    )
                    .child(
                        div()
                            .id("group-close")
                            .ml_auto()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .child(crate::i18n::tr("Close"))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.quit();
                            })),
                    ),
            )
    }
}

/// Open the group dialog for an instance.
pub fn open_groups(
    data_root: PathBuf,
    instance: String,
    current: Option<String>,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<GroupView>> {
    let bounds = Bounds::centered(None, size(px(520.), px(460.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Change Group — Prism Launcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| {
            cx.new(|cx| {
                GroupView::new(
                    data_root.clone(),
                    instance.clone(),
                    current.clone(),
                    theme,
                    cx,
                )
            })
        },
    )
    .ok()
}

// ---- Rename world window ----

/// Rename a world of an instance, like Prism's rename dialog.
pub struct RenameWorldView {
    name_input: Entity<TextInput>,
    status: SharedString,
    theme: Theme,
    instance_dir: PathBuf,
    data_root: PathBuf,
    old_name: String,
}

impl RenameWorldView {
    pub fn new(
        instance_dir: PathBuf,
        data_root: PathBuf,
        old_name: String,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| {
            let mut t = TextInput::new("World name", cx);
            t.set_text(old_name.clone(), cx);
            t.set_theme(theme, cx);
            t
        });
        Self {
            name_input,
            status: "Ready".into(),
            theme,
            instance_dir,
            data_root,
            old_name,
        }
    }

    fn rename(&mut self, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).text().trim().to_string();
        if name.is_empty() || name == self.old_name {
            self.status = "The name did not change".into();
            cx.notify();
            return;
        }
        match crate::worlds::rename_world(&self.instance_dir, &self.old_name, &name) {
            Ok(dir) => {
                self.status = format!("Renamed to {}", dir.display()).into();
                cx.quit();
            }
            Err(e) => self.status = format!("Rename failed: {e}").into(),
        }
        cx.notify();
    }
}

impl Render for RenameWorldView {
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
            .child(div().text_lg().child(crate::i18n::tr("Rename World")))
            .child(div().w(px(360.)).child(self.name_input.clone()))
            .child(
                div()
                    .id("rename-world")
                    .px_3()
                    .py_1()
                    .w(px(120.))
                    .rounded_md()
                    .bg(rgb(theme.accent))
                    .text_color(rgb(theme.on_accent))
                    .cursor_pointer()
                    .child(crate::i18n::tr("Rename"))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.rename(cx))),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.subtext))
                    .child(self.status.clone()),
            )
    }
}

/// Open the rename dialog for a world.
pub fn open_rename_world(
    instance_dir: PathBuf,
    name: String,
    data_root: PathBuf,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<RenameWorldView>> {
    let bounds = Bounds::centered(None, size(px(420.), px(220.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some(format!("Rename {name} — Prism Launcher").into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| {
            cx.new(|cx| {
                RenameWorldView::new(
                    instance_dir.clone(),
                    data_root.clone(),
                    name.clone(),
                    theme,
                    cx,
                )
            })
        },
    )
    .ok()
}

/// A tool button, the way Qt's Fusion style draws it: an icon and a text
/// next to each other, flat, with a background only while the mouse hovers.
pub fn tool_button(
    icon_name: &str,
    label: &str,
    theme: Theme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let image = crate::icons::icon(icon_name, theme.text);
    div()
        .id(gpui::ElementId::Name(SharedString::from(format!(
            "tool-{icon_name}-{label}"
        ))))
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .h(px(24.0))
        .px_2()
        .cursor_pointer()
        .text_color(rgb(theme.text))
        .hover(|style| style.bg(theme.rgba(theme.accent, 0.25)))
        .child(if let Some(image) = image {
            gpui::img(image).w(px(16.)).h(px(16.)).into_any_element()
        } else {
            div().w(px(16.)).h(px(16.)).into_any_element()
        })
        .child(div().text_sm().child(label.to_string()))
        .on_click(move |_, window, cx| on_click(window, cx))
        .into_any_element()
}

/// A check box, the way Qt's Fusion style draws it.
pub fn checkbox(checked: bool, theme: Theme) -> AnyElement {
    let mark = if checked {
        crate::icons::icon("check", theme.on_accent)
    } else {
        None
    };
    div()
        .w(px(14.))
        .h(px(14.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .border_1()
        .border_color(rgb(if checked { theme.accent } else { theme.border }))
        .bg(if checked {
            rgb(theme.accent)
        } else {
            rgb(theme.base)
        })
        .child(if let Some(image) = mark {
            gpui::img(image).w(px(12.)).h(px(12.)).into_any_element()
        } else {
            div().into_any_element()
        })
        .into_any_element()
}

/// A tool button that is drawn dimmed, like a disabled Qt action.
pub fn tool_button_disabled(label: &str, theme: Theme) -> AnyElement {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .h(px(24.0))
        .px_2()
        .text_color(rgb(theme.subtext))
        .child(div().w(px(16.)).h(px(16.)).into_any_element())
        .child(div().text_sm().child(label.to_string()))
        .into_any_element()
}

/// A separator between the groups of a tool bar, as Qt draws them.
pub fn tool_separator(theme: Theme) -> AnyElement {
    div()
        .w(px(9.0))
        .h(px(18.0))
        .mx_1()
        .border_l_1()
        .border_color(rgb(theme.border))
        .into_any_element()
}

/// The background of a panel that lists things, using the base colour.
pub fn panel(theme: Theme) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .bg(rgb(theme.base))
        .overflow_hidden()
        .into_any_element()
}

// ---- Folders window ----

/// The folders Prism offers under the "Folders" button.
pub struct FoldersView {
    data_root: PathBuf,
    theme: Theme,
}

impl FoldersView {
    pub fn new(data_root: PathBuf, theme: Theme, _cx: &mut Context<Self>) -> Self {
        Self { data_root, theme }
    }
}

impl Render for FoldersView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        // The folders Prism shows, in the order of its menu.
        let folders: Vec<(&str, String)> = vec![
            (
                "View Launcher Root Folder",
                self.data_root.display().to_string(),
            ),
            (
                "View Logs Folder",
                self.data_root.join("logs").display().to_string(),
            ),
            (
                "View Icons Folder",
                self.data_root.join("icons").display().to_string(),
            ),
            (
                "View Central Mods Folder",
                self.data_root.join("mods").display().to_string(),
            ),
            (
                "View Java Folder",
                self.data_root.join("java").display().to_string(),
            ),
            (
                "View Skins Folder",
                self.data_root.join("skins").display().to_string(),
            ),
            (
                "View Widget Theme Folder",
                self.data_root.join("themes").display().to_string(),
            ),
            (
                "View Icon Theme Folder",
                self.data_root.join("icons").display().to_string(),
            ),
        ];

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
                    .h(px(24.0))
                    .px_3()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(crate::i18n::tr("Folders")),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .bg(rgb(theme.base))
                    .children(folders.into_iter().map(|(label, path)| {
                        div()
                            .id(gpui::ElementId::Name(SharedString::from(format!(
                                "folder-{label}"
                            ))))
                            .flex()
                            .items_center()
                            .gap_2()
                            .h(px(26.0))
                            .px_3()
                            .cursor_pointer()
                            .hover(|style| style.bg(theme.rgba(theme.accent, 0.25)))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(theme.text))
                                    .child(crate::i18n::tr(label)),
                            )
                            .child(
                                div()
                                    .ml_auto()
                                    .text_xs()
                                    .text_color(rgb(theme.subtext))
                                    .child(path.clone()),
                            )
                            .on_click(move |_, _, _cx| {
                                let _ = crate::desktop::open_path(std::path::Path::new(&path));
                            })
                    })),
            )
    }
}

/// Open the window with the shared folders.
pub fn open_folders(
    data_root: PathBuf,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<FoldersView>> {
    let bounds = Bounds::centered(None, size(px(460.), px(320.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Folders — Prism Launcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| FoldersView::new(data_root.clone(), theme, cx)),
    )
    .ok()
}

// ---- Accounts window ----

/// Manage the accounts the launcher can launch with, like Prism's
/// `AccountListPage`.
pub struct AccountsView {
    data_root: PathBuf,
    accounts: Vec<crate::auth::Account>,
    active: Option<String>,
    status: SharedString,
    theme: Theme,
}

impl AccountsView {
    pub fn new(data_root: PathBuf, theme: Theme, _cx: &mut Context<Self>) -> Self {
        let store = crate::accounts::AccountStore::load(&data_root);
        Self {
            data_root,
            accounts: store.accounts,
            active: store.active,
            status: "Ready".into(),
            theme,
        }
    }

    fn reload(&mut self) {
        let store = crate::accounts::AccountStore::load(&self.data_root);
        self.accounts = store.accounts;
        self.active = store.active;
    }

    /// Start the Microsoft device-code sign-in flow.
    fn sign_in(&mut self, cx: &mut Context<Self>) {
        self.status = "Waiting for the sign-in to finish…".into();
        cx.notify();
        let root = self.data_root.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let auth = crate::auth::AuthService::new();
                    auth.login_interactive(|dc| {
                        eprintln!(
                            "To sign in, open {} and enter the code: {}",
                            dc.verification_uri, dc.user_code
                        );
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(account) => {
                        let mut store = crate::accounts::AccountStore::load(&root);
                        store.upsert(account.clone());
                        let _ = store.save(&root);
                        this.reload();
                        this.status = format!("Signed in as {}", account.username).into();
                    }
                    Err(e) => this.status = format!("Sign-in failed: {e}").into(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Add an offline account with a generated UUID.
    fn add_offline(&mut self, name: String, cx: &mut Context<Self>) {
        if name.trim().is_empty() {
            return;
        }
        let account = crate::accounts::offline_account(name.trim());
        let mut store = crate::accounts::AccountStore::load(&self.data_root);
        store.upsert(account.clone());
        let _ = store.save(&self.data_root);
        self.reload();
        self.status = format!("Added offline account {}", account.username).into();
        cx.notify();
    }

    /// Make an account the active one.
    fn activate(&mut self, uuid: String, cx: &mut Context<Self>) {
        let mut store = crate::accounts::AccountStore::load(&self.data_root);
        store.active = Some(uuid.clone());
        let _ = store.save(&self.data_root);
        self.active = Some(uuid);
        self.status = "Active account updated".into();
        cx.notify();
    }

    /// Remove an account.
    fn remove(&mut self, uuid: String, cx: &mut Context<Self>) {
        let mut store = crate::accounts::AccountStore::load(&self.data_root);
        store.remove(&uuid);
        let _ = store.save(&self.data_root);
        self.reload();
        self.status = "Account removed".into();
        cx.notify();
    }
}

impl Render for AccountsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let entity = cx.entity();
        let active = self.active.clone();
        let rows: Vec<AnyElement> = self
            .accounts
            .iter()
            .map(|account| {
                let is_active = active.as_deref() == Some(account.uuid.as_str());
                let uuid_use = account.uuid.clone();
                let uuid_remove = account.uuid.clone();
                let e_use = cx.entity();
                let e_remove = e_use.clone();
                div()
                    .id(gpui::ElementId::Name(SharedString::from(format!(
                        "account-{}",
                        account.uuid
                    ))))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_base().child(if is_active {
                                format!("{} (active)", account.username)
                            } else {
                                account.username.clone()
                            }))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.subtext))
                                    .child(account.uuid.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id(gpui::ElementId::Name(SharedString::from(format!(
                                        "use-{}",
                                        account.uuid
                                    ))))
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(theme.surface))
                                    .border_1()
                                    .border_color(rgb(theme.border))
                                    .child(crate::i18n::tr("Use"))
                                    .on_click(move |_, _, cx| {
                                        let u = uuid_use.clone();
                                        e_use.update(cx, |this, cx| this.activate(u, cx));
                                    }),
                            )
                            .child(
                                div()
                                    .id(gpui::ElementId::Name(SharedString::from(format!(
                                        "remove-{}",
                                        account.uuid
                                    ))))
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(theme.surface))
                                    .border_1()
                                    .border_color(rgb(theme.border))
                                    .child(crate::i18n::tr("Remove"))
                                    .on_click(move |_, _, cx| {
                                        let u = uuid_remove.clone();
                                        e_remove.update(cx, |this, cx| this.remove(u, cx));
                                    }),
                            ),
                    )
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
                    .h(px(40.0))
                    .px_4()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(div().text_lg().child(crate::i18n::tr("Accounts")))
                    .child(crate::ui::tool_button(
                        "account",
                        &crate::i18n::tr("Add Microsoft account"),
                        theme,
                        move |_window, cx| {
                            entity.update(cx, |this, cx| this.sign_in(cx));
                        },
                    ))
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
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .children(rows),
            )
    }
}

pub fn open_accounts(
    data_root: PathBuf,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<AccountsView>> {
    let bounds = Bounds::centered(None, size(px(480.), px(420.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Accounts — Prism Launcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| AccountsView::new(data_root.clone(), theme, cx)),
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
            vec![
                div()
                    .p_3()
                    .text_color(rgb(theme.subtext))
                    .child("(no servers)")
                    .into_any_element(),
            ]
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
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .children(rows),
            )
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
