//! Mod / resource-pack / shader / modpack browser (Modrinth).
//! Mirrors launcher/ui/pages/modplatform.

use crate::modplatform::modrinth::Modrinth;
use crate::theme::Theme;
use crate::widgets::text_input::TextInput;
use gpui::{
    App, Bounds, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Mod,
    Modpack,
    ResourcePack,
    Shader,
}

impl Kind {
    const ALL: [Kind; 4] = [Kind::Mod, Kind::Modpack, Kind::ResourcePack, Kind::Shader];

    fn label(self) -> &'static str {
        match self {
            Kind::Mod => "Mods",
            Kind::Modpack => "Modpacks",
            Kind::ResourcePack => "Resource Packs",
            Kind::Shader => "Shaders",
        }
    }

    fn facet(self) -> &'static str {
        match self {
            Kind::Mod => "mod",
            Kind::Modpack => "modpack",
            Kind::ResourcePack => "resourcepack",
            Kind::Shader => "shader",
        }
    }

    /// Subdirectory of `minecraft/` where the installed file belongs.
    fn subdir(self) -> &'static str {
        match self {
            Kind::Mod => "mods",
            Kind::Modpack => "",
            Kind::ResourcePack => "resourcepacks",
            Kind::Shader => "shaderpacks",
        }
    }
}

#[derive(Clone)]
struct HitRow {
    title: String,
    slug: String,
    description: String,
    downloads: u64,
}

pub struct ModBrowser {
    input: Entity<TextInput>,
    hits: Vec<HitRow>,
    kind: Kind,
    data_root: PathBuf,
    instance_dir: PathBuf,
    status: SharedString,
    theme: Theme,
}

impl ModBrowser {
    pub fn new(
        instance_dir: PathBuf,
        data_root: PathBuf,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| TextInput::new("Search Modrinth…", cx));
        Self {
            input,
            hits: Vec::new(),
            kind: Kind::Mod,
            data_root,
            instance_dir,
            status: "Choose a category, type a query, press Search".into(),
            theme,
        }
    }

    fn install_dir(&self) -> PathBuf {
        let base = self.instance_dir.join("minecraft");
        let sub = self.kind.subdir();
        if sub.is_empty() {
            base
        } else {
            base.join(sub)
        }
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().trim().to_string();
        if query.is_empty() {
            self.status = "Enter a query".into();
            cx.notify();
            return;
        }
        let facet = self.kind.facet().to_string();
        self.status = format!("Searching {}…", self.kind.label()).into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let hits = cx
                .background_executor()
                .spawn(async move {
                    Modrinth
                        .search_typed(&query, Some(&facet), None, None)
                        .unwrap_or_default()
                })
                .await;
            let rows: Vec<HitRow> = hits
                .into_iter()
                .map(|h| HitRow {
                    title: h.title,
                    slug: h.slug,
                    description: h.description,
                    downloads: h.downloads,
                })
                .collect();
            this.update(cx, |this, cx| {
                this.status = format!("{} results", rows.len()).into();
                this.hits = rows;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn install(&mut self, slug: String, cx: &mut Context<Self>) {
        let kind = self.kind;
        let dir = self.install_dir();
        let instances_root = self.data_root.join("instances");
        self.status = format!("Installing {slug}…").into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let msg = cx
                .background_executor()
                .spawn(async move {
                    let provider = Modrinth;
                    let versions = match provider.versions(&slug, None, None) {
                        Ok(v) => v,
                        Err(e) => return format!("Lookup failed: {e}"),
                    };
                    let Some(v) = versions.first() else {
                        return "No versions found".to_string();
                    };
                    if kind == Kind::Modpack {
                        // Download the .mrpack and unpack into a fresh instance.
                        let inst_dir = instances_root.join(&slug);
                        if let Err(e) = std::fs::create_dir_all(&inst_dir) {
                            return format!("mkdir failed: {e}");
                        }
                        if let Err(e) = provider.download_version_file(v, &inst_dir) {
                            return format!("Download failed: {e}");
                        }
                        let Some(pack) = crate::fs::list_files(&inst_dir)
                            .into_iter()
                            .find(|p| p.extension().map(|e| e == "mrpack").unwrap_or(false))
                        else {
                            return "pack download produced no .mrpack".to_string();
                        };
                        match crate::pack::mrpack::install(&pack, &inst_dir) {
                            Ok(_) => {
                                let cfg = "[General]\nInstanceType=OneSix\nConfigVersion=1.3\n";
                                let _ = std::fs::write(inst_dir.join("instance.cfg"), cfg);
                                format!("Installed modpack into {}", inst_dir.display())
                            }
                            Err(e) => format!("Extract failed: {e}"),
                        }
                    } else {
                        match provider.download_version_file(v, &dir) {
                            Ok(p) => format!("Installed {}", p.display()),
                            Err(e) => format!("Download failed: {e}"),
                        }
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

impl Render for ModBrowser {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;

        let kind_tabs = Kind::ALL
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let selected = *k == self.kind;
                let k2 = *k;
                div()
                    .id(i)
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
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let rows: Vec<_> = self
            .hits
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, h)| {
                let slug = h.slug.clone();
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
                            .max_w(px(500.))
                            .child(div().text_base().child(h.title.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.subtext))
                                    .child(format!("{} · {} dl", h.description, h.downloads)),
                            ),
                    )
                    .child(
                        div()
                            .id(i)
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(theme.accent))
                            .text_color(rgb(theme.on_accent))
                            .cursor_pointer()
                            .child("Install")
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.install(slug.clone(), cx);
                            })),
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
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(div().flex().gap_2().children(kind_tabs))
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
                                    .child(self.status.clone()),
                            ),
                    ),
            )
            .child(div().flex_1().flex().flex_col().overflow_hidden().children(rows))
    }
}

pub fn open_mod_browser(
    instance_dir: PathBuf,
    data_root: PathBuf,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<ModBrowser>> {
    let bounds = Bounds::centered(None, size(px(760.), px(600.)), cx);
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
