//! Content browser: mods, modpacks, resource packs and shaders across every
//! supported platform. Mirrors launcher/ui/pages/modplatform.

use crate::meta;
use crate::modplatform::{ContentKind, Platform, PlatformHit, SearchQuery, platform};
use crate::theme::Theme;
use crate::widgets::text_input::TextInput;
use gpui::{
    AnyElement, App, Bounds, Context, Entity, IntoElement, KeyDownEvent, ParentElement, Render,
    SharedString, Styled, UniformListScrollHandle, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size, uniform_list,
};
use std::path::PathBuf;

#[derive(Clone)]
struct HitRow {
    title: String,
    id: String,
    slug: String,
    description: String,
    downloads: u64,
    platform: Platform,
    kind: ContentKind,
    extra: String,
    /// When set, the hit comes from a manifest plugin source (index into
    /// `plugin_sources`) rather than a built-in platform.
    plugin_source: Option<usize>,
}

pub struct ModBrowser {
    input: Entity<TextInput>,
    hits: Vec<HitRow>,
    kind: ContentKind,
    platform: Platform,
    /// Content sources declared by installed manifest plugins.
    plugin_sources: Vec<(String, crate::plugin::ContentSource)>,
    /// Selected plugin source, when one is active instead of `platform`.
    plugin_selected: Option<usize>,
    /// Minecraft version + loader of the target instance, used to filter.
    game_version: Option<String>,
    loader: Option<String>,
    data_root: PathBuf,
    instance_dir: PathBuf,
    status: SharedString,
    theme: Theme,
    scroll: UniformListScrollHandle,
}

impl ModBrowser {
    pub fn new(
        instance_dir: PathBuf,
        data_root: PathBuf,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| TextInput::new("Search for content…", cx));
        let (game_version, loader) = target_of(&instance_dir);
        let host = crate::plugin::PluginHost::discover(&data_root);
        let plugin_sources: Vec<(String, crate::plugin::ContentSource)> = host
            .sources()
            .into_iter()
            .map(|(plugin, source)| (plugin.to_string(), source.clone()))
            .collect();
        Self {
            input,
            hits: Vec::new(),
            kind: ContentKind::Mod,
            platform: Platform::Modrinth,
            plugin_sources,
            plugin_selected: None,
            game_version,
            loader,
            data_root,
            instance_dir,
            status: "Choose a category, type a query, press Search".into(),
            theme,
            scroll: UniformListScrollHandle::default(),
        }
    }

    fn install_dir(&self) -> PathBuf {
        let base = self.instance_dir.join("minecraft");
        match self.kind {
            ContentKind::Mod => base.join("mods"),
            ContentKind::ResourcePack => base.join("resourcepacks"),
            ContentKind::Shader => base.join("shaderpacks"),
            ContentKind::Modpack => base,
        }
    }

    /// Platforms available for the selected kind.
    fn platforms(&self) -> Vec<Platform> {
        Platform::for_kind(self.kind)
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().trim().to_string();
        if query.is_empty() {
            self.status = "Enter a query".into();
            cx.notify();
            return;
        }
        // A plugin source, when selected, replaces the built-in platform.
        if let Some(idx) = self.plugin_selected {
            let Some((plugin, source)) = self.plugin_sources.get(idx).cloned() else {
                self.status = "Plugin source is no longer available".into();
                cx.notify();
                return;
            };
            let kind = self.kind;
            self.status = format!("Searching {} ({plugin})…", source.name).into();
            cx.notify();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        crate::plugin::search_source(&source, &query).map(|hits| {
                            hits.into_iter()
                                .map(|h| HitRow {
                                    title: h.title,
                                    id: h.id,
                                    slug: String::new(),
                                    description: h.description,
                                    downloads: h.downloads,
                                    platform: Platform::Modrinth,
                                    kind,
                                    extra: String::new(),
                                    plugin_source: Some(idx),
                                })
                                .collect::<Vec<_>>()
                        })
                    })
                    .await;
                this.update(cx, |this, cx| {
                    match result {
                        Ok(hits) => {
                            this.status = format!("{} results", hits.len()).into();
                            this.hits = hits;
                        }
                        Err(e) => {
                            this.status = format!("Search failed: {e}").into();
                            this.hits.clear();
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return;
        }
        let platform = self.platform;
        let kind = self.kind;
        let q = SearchQuery::new(query)
            .with_game_version(self.game_version.clone())
            .with_loader(self.loader.clone());
        self.status = format!("Searching {}…", platform.label()).into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { platform::search(platform, kind, &q) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(hits) => {
                        this.status = format!("{} results", hits.len()).into();
                        this.hits = hits
                            .into_iter()
                            .map(|h| HitRow {
                                title: h.title,
                                id: h.id,
                                slug: h.slug,
                                description: h.description,
                                downloads: h.downloads,
                                platform: h.platform,
                                kind: h.kind,
                                extra: h.extra,
                                plugin_source: None,
                            })
                            .collect();
                    }
                    Err(e) => {
                        this.status = format!("Search failed: {e}").into();
                        this.hits.clear();
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn install(&mut self, row: HitRow, cx: &mut Context<Self>) {
        let dir = self.install_dir();
        let instances_root = crate::instance::instances_dir(&self.data_root);
        let settings = crate::settings::GlobalSettings::load(&self.data_root);
        let game_version = self.game_version.clone();
        let loader = self.loader.clone();
        let plugin_source = row
            .plugin_source
            .and_then(|i| self.plugin_sources.get(i).cloned());
        if let Some((_, _source)) = &plugin_source {
            if row.kind == ContentKind::Modpack {
                self.status =
                    "Modpacks cannot be installed from a plugin content source".into();
                cx.notify();
                return;
            }
        }
        self.status = format!("Installing {}…", row.title).into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let msg = cx
                .background_executor()
                .spawn(async move {
                    if let Some((_, source)) = plugin_source {
                        let hit = crate::plugin::SourceHit {
                            id: row.id.clone(),
                            title: row.title.clone(),
                            description: row.description.clone(),
                            downloads: row.downloads,
                        };
                        return match crate::plugin::install_source(&source, &hit, &dir) {
                            Ok(path) => format!("installed {}", path.display()),
                            Err(e) => format!("Install failed: {e}"),
                        };
                    }
                    let hit = PlatformHit {
                        platform: row.platform,
                        kind: row.kind,
                        id: row.id.clone(),
                        slug: row.slug.clone(),
                        title: row.title.clone(),
                        description: row.description.clone(),
                        downloads: row.downloads,
                        extra: row.extra.clone(),
                    };
                    let dest = if row.kind == ContentKind::Modpack {
                        let dir_name = sanitize(&row.slug);
                        let inst_dir = instances_root.join(&dir_name);
                        if inst_dir.exists() {
                            return format!("instance already exists: {dir_name}");
                        }
                        if let Err(e) = std::fs::create_dir_all(inst_dir.join("minecraft")) {
                            return format!("mkdir failed: {e}");
                        }
                        let cfg = format!(
                            "[General]\nInstanceType=OneSix\nConfigVersion=1.3\nname={dir_name}\nMinMemAlloc={}\nMaxMemAlloc={}\n",
                            settings.min_mem(),
                            settings.max_mem()
                        );
                        let _ = std::fs::write(inst_dir.join("instance.cfg"), cfg);
                        inst_dir
                    } else {
                        dir.clone()
                    };
                    match platform::install(
                        &hit,
                        &dest,
                        game_version.as_deref(),
                        loader.as_deref(),
                    ) {
                        Ok(outcome) => {
                            for (uid, version) in &outcome.components {
                                let _ = meta::set_component(&dest, uid, version);
                            }
                            outcome.message
                        }
                        Err(e) => format!("Install failed: {e}"),
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.status = msg.into();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn sanitize(name: &str) -> String {
    name.replace(['/', '\\', ':'], "_").trim().to_string()
}

/// Minecraft version + loader of an instance, for search filters.
fn target_of(instance_dir: &std::path::Path) -> (Option<String>, Option<String>) {
    let mut mc = None;
    let mut loader = None;
    if let Some(pack) = meta::read_component_pack(instance_dir) {
        for c in &pack.components {
            if c.uid == "net.minecraft" {
                mc = Some(c.version.clone());
            } else if !c.dependency_only {
                let name = match c.uid.as_str() {
                    "net.fabricmc.fabric-loader" => "fabric",
                    "net.minecraftforge" => "forge",
                    "net.neoforged" => "neoforge",
                    "org.quiltmc.quilt-loader" => "quilt",
                    _ => continue,
                };
                loader = Some(name.to_string());
            }
        }
    }
    (mc, loader)
}

fn compact(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

impl Render for ModBrowser {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let kinds = [
            ContentKind::Mod,
            ContentKind::Modpack,
            ContentKind::ResourcePack,
            ContentKind::Shader,
        ];

        let kind_tabs = kinds
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let selected = *k == self.kind;
                let k2 = *k;
                div()
                    .id(("kind", i))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if selected { rgb(theme.accent) } else { rgb(theme.surface) })
                    .text_color(if selected { rgb(theme.on_accent) } else { rgb(theme.text) })
                    .child(k.label())
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.kind = k2;
                        this.hits.clear();
                        let avail = this.platforms();
                        if !avail.contains(&this.platform) {
                            this.platform = avail[0];
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let platforms = self.platforms();
        let plugin_selected = self.plugin_selected;
        let platform_tabs = platforms
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let selected = plugin_selected.is_none() && *p == self.platform;
                let p2 = *p;
                div()
                    .id(("plat", i))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if selected { rgb(theme.accent) } else { rgb(theme.background) })
                    .text_color(if selected { rgb(theme.on_accent) } else { rgb(theme.subtext) })
                    .child(p.label())
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.platform = p2;
                        this.plugin_selected = None;
                        this.hits.clear();
                        this.status = format!("{} selected", p2.label()).into();
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        // Installed manifest plugins add their own content sources as extra
        // tabs, indistinguishable from built-in platforms to the user.
        let plugin_tabs = self
            .plugin_sources
            .iter()
            .enumerate()
            .map(|(i, (plugin, source))| {
                let selected = plugin_selected == Some(i);
                div()
                    .id(("plugin", i))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if selected { rgb(theme.accent) } else { rgb(theme.background) })
                    .text_color(if selected { rgb(theme.on_accent) } else { rgb(theme.subtext) })
                    .child(format!("{} · {}", source.name, plugin))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.plugin_selected = Some(i);
                        this.hits.clear();
                        this.status = this
                            .plugin_sources
                            .get(i)
                            .map(|(_, s)| format!("{} selected", s.name))
                            .unwrap_or_else(|| "plugin selected".into())
                            .into();
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        // Virtualised so a 10k-result search only lays out visible rows.
        let hits = self.hits.clone();
        let entity = cx.entity();
        let list: AnyElement = if hits.is_empty() {
            div()
                .p_4()
                .text_color(rgb(theme.subtext))
                .child("No results yet.")
                .into_any_element()
        } else {
            uniform_list("mod-results", hits.len(), move |range, _window, _cx| {
                range
                    .filter_map(|i| {
                        let h = hits.get(i)?.clone();
                        Some(hit_row(h, i, theme, entity.clone()))
                    })
                    .collect::<Vec<_>>()
            })
            .track_scroll(self.scroll.clone())
            .flex_1()
            .into_any_element()
        };

        let target = match (&self.game_version, &self.loader) {
            (Some(mc), Some(l)) => format!("{mc} · {l}"),
            (Some(mc), None) => mc.clone(),
            _ => "unknown target".to_string(),
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
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(div().flex().gap_2().children(kind_tabs))
                    .child(div().flex().flex_wrap().gap_2().children(platform_tabs))
                    .child(div().flex().flex_wrap().gap_2().children(plugin_tabs))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().w(px(360.)).child(self.input.clone()))
                            .child(
                                div()
                                    .id("search-btn")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(theme.accent))
                                    .text_color(rgb(theme.on_accent))
                                    .cursor_pointer()
                                    .child("Search")
                                    .on_click(
                                        cx.listener(|this: &mut Self, _, _, cx| this.search(cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(theme.subtext))
                                    .child(format!("target: {target}")),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(theme.subtext))
                                    .child(self.status.clone()),
                            ),
                    ),
            )
            .on_key_down(cx.listener(|this: &mut Self, ev: &KeyDownEvent, _window, cx| {
                if ev.keystroke.key == "enter" {
                    this.search(cx);
                }
            }))
            .child(div().flex_1().flex().flex_col().overflow_hidden().child(list))
    }
}

/// One virtualised search result row.
fn hit_row(h: HitRow, i: usize, theme: Theme, entity: Entity<ModBrowser>) -> AnyElement {
    let h2 = h.clone();
    div()
        .id(("hit", i))
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
                .max_w(px(520.))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(div().text_base().child(h.title.clone()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme.subtext))
                                .child(h.platform.label()),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme.subtext))
                        .child(format!("{} · {} dl", h.description, compact(h.downloads))),
                ),
        )
        .child(
            div()
                .id(("install", i))
                .px_3()
                .py_1()
                .rounded_md()
                .bg(rgb(theme.accent))
                .text_color(rgb(theme.on_accent))
                .cursor_pointer()
                .child("Install")
                .on_click(move |_, _, cx| {
                    let row = h2.clone();
                    entity.update(cx, |this, cx| this.install(row, cx));
                }),
        )
        .into_any_element()
}

pub fn open_mod_browser(
    instance_dir: PathBuf,
    data_root: PathBuf,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<ModBrowser>> {
    let bounds = Bounds::centered(None, size(px(860.), px(640.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Browse Content — Prism Launcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| ModBrowser::new(instance_dir, data_root, theme, cx)),
    )
    .ok()
}
