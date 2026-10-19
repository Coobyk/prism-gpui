//! GPUI views. Mirrors launcher/ui/*.

use crate::fs;
use gpui::{
    App, Bounds, Context, IntoElement, ParentElement, Render, SharedString, Styled,
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
    pub tab: Tab,
    pub entries: Vec<String>,
    pub status: SharedString,
}

impl InstanceView {
    pub fn new(name: impl Into<SharedString>, dir: PathBuf) -> Self {
        let mut v = Self {
            name: name.into(),
            dir,
            tab: Tab::Mods,
            entries: Vec::new(),
            status: "Ready".into(),
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
                    .bg(if selected { rgb(0x89b4fa) } else { rgb(0x313244) })
                    .text_color(if selected { rgb(0x11111b) } else { rgb(0xcdd6f4) })
                    .child(t.label())
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.tab = t2;
                        this.refresh();
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let entries = if self.entries.is_empty() {
            vec![div()
                .p_3()
                .text_color(rgb(0x6c7086))
                .child("(empty)")
                .into_any_element()]
        } else {
            self.entries
                .iter()
                .map(|e| {
                    div()
                        .px_3()
                        .py_1()
                        .border_b_1()
                        .border_color(rgb(0x313244))
                        .child(e.clone())
                        .into_any_element()
                })
                .collect()
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(40.0))
                    .px_4()
                    .gap_2()
                    .bg(rgb(0x181825))
                    .border_b_1()
                    .border_color(rgb(0x313244))
                    .child(self.name.clone())
                    .child(div().ml_auto().text_sm().text_color(rgb(0xa6adc8)).child(self.status.clone())),
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
pub fn open_instance(name: &str, dir: &Path, cx: &mut App) -> Option<gpui::WindowHandle<InstanceView>> {
    let name = name.to_string();
    let dir = dir.to_path_buf();
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
            |_, cx| cx.new(|_| InstanceView::new(name.clone(), dir.clone())),
        )
        .ok()?;
    Some(handle)
}
