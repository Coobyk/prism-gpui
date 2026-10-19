//! Mod browser view (Modrinth). Mirrors launcher/ui/pages/modplatform.

use crate::modplatform::{ModHit, ModProvider, modrinth::Modrinth};
use crate::theme::Theme;
use crate::widgets::text_input::TextInput;
use gpui::{
    App, Bounds, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use std::path::PathBuf;

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
    target_dir: PathBuf,
    status: SharedString,
    theme: Theme,
}

impl ModBrowser {
    pub fn new(target_dir: PathBuf, theme: Theme, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new("Search Modrinth…", cx));
        Self {
            input,
            hits: Vec::new(),
            target_dir,
            status: "Type a query and press Search".into(),
            theme,
        }
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().trim().to_string();
        if query.is_empty() {
            self.status = "Enter a query".into();
            cx.notify();
            return;
        }
        self.status = format!("Searching {query}…").into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let hits = cx
                .background_executor()
                .spawn(async move { Modrinth.search(&query) })
                .await;
            let rows: Vec<HitRow> = hits
                .into_iter()
                .map(|h: ModHit| HitRow {
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
        let dir = self.target_dir.clone();
        self.status = format!("Installing {slug}…").into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let msg = cx
                .background_executor()
                .spawn(async move {
                    let provider = Modrinth;
                    match provider.versions(&slug, None, None) {
                        Ok(versions) => match versions.first() {
                            Some(v) => match provider.download_version_file(v, &dir) {
                                Ok(p) => format!("Installed {}", p.display()),
                                Err(e) => format!("Download failed: {e}"),
                            },
                            None => "No versions found".to_string(),
                        },
                        Err(e) => format!("Lookup failed: {e}"),
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
                            .max_w(px(520.))
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
                    .items_center()
                    .gap_3()
                    .p_3()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
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
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| this.search(cx))),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme.subtext))
                            .child(self.status.clone()),
                    ),
            )
            .child(div().flex_1().flex().flex_col().overflow_hidden().children(rows))
    }
}

pub fn open_mod_browser(
    target_dir: PathBuf,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<ModBrowser>> {
    let bounds = Bounds::centered(None, size(px(720.), px(600.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Browse Mods — Prism Launcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| ModBrowser::new(target_dir, theme, cx)),
    )
    .ok()
}
