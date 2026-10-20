//! Console window: launches an instance and streams its output.
//! Mirrors launcher/ui/ViewLogWindow + launcher/console.

use crate::launch::LaunchProfile;
use crate::theme::Theme;
use gpui::{
    ClipboardItem,
    App, Bounds, Context, IntoElement, ParentElement, Render, SharedString, Styled, Timer, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct ConsoleView {
    lines: Arc<Mutex<Vec<String>>>,
    done: Arc<AtomicBool>,
    status: SharedString,
    theme: Theme,
    rendered: usize,
}

impl ConsoleView {
    pub fn spawn_launch(profile: LaunchProfile, name: String, theme: Theme, cx: &mut Context<Self>) -> Self {
        let _ = &cx;
        let lines = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let lines_bg = lines.clone();
        let done_bg = done.clone();
        let status: SharedString = format!("Launching {name}…").into();

        std::thread::spawn(move || {
            let cap = profile.console_max_lines.max(100);
            let push = |lines: &Arc<Mutex<Vec<String>>>, s: String| {
                let mut v = lines.lock().unwrap();
                v.push(s);
                if v.len() > cap {
                    let excess = v.len() - cap;
                    v.drain(0..excess);
                }
            };
            // Assets and the vanilla jar are fetched here rather than when the
            // profile is built, so opening the console never blocks the UI.
            push(&lines_bg, "Checking assets…".to_string());
            match profile.ensure_ready() {
                Ok(0) => {}
                Ok(n) => push(&lines_bg, format!("Fetched {n} missing files")),
                Err(e) => push(&lines_bg, format!("warning: {e}")),
            }
            if !profile.pre_launch.trim().is_empty() {
                push(&lines_bg, format!("$ {}", profile.pre_launch));
                match crate::launch::run_hook(&profile.pre_launch, &profile.game_dir) {
                    Ok(()) => {}
                    Err(e) => push(&lines_bg, format!("pre-launch hook: {e}")),
                }
            }
            let mut cmd = profile.jvm_command();
            match cmd.spawn() {
                Ok(mut child) => {
                    let out = child.stdout.take();
                    let err = child.stderr.take();
                    let l1 = lines_bg.clone();
                    let t1 = std::thread::spawn(move || {
                        if let Some(out) = out {
                            for line in BufReader::new(out).lines().map_while(Result::ok) {
                                let mut v = l1.lock().unwrap();
                                v.push(line);
                                if v.len() > 100_000 {
                                    let excess = v.len() - 100_000;
                                    v.drain(0..excess);
                                }
                            }
                        }
                    });
                    let l2 = lines_bg.clone();
                    let t2 = std::thread::spawn(move || {
                        if let Some(err) = err {
                            for line in BufReader::new(err).lines().map_while(Result::ok) {
                                let mut v = l2.lock().unwrap();
                                v.push(line);
                                if v.len() > 100_000 {
                                    let excess = v.len() - 100_000;
                                    v.drain(0..excess);
                                }
                            }
                        }
                    });
                    let _ = t1.join();
                    let _ = t2.join();
                    let code = child.wait().map(|s| s.to_string()).unwrap_or_default();
                    push(&lines_bg, format!("--- process exited: {code} ---"));
                }
                Err(e) => {
                    push(&lines_bg, format!("Failed to start: {e}"));
                }
            }
            if !profile.post_exit.trim().is_empty() {
                push(&lines_bg, format!("$ {}", profile.post_exit));
                if let Err(e) = crate::launch::run_hook(&profile.post_exit, &profile.game_dir) {
                    push(&lines_bg, format!("post-exit hook: {e}"));
                }
            }
            done_bg.store(true, Ordering::SeqCst);
        });

        Self {
            lines,
            done,
            status,
            theme,
            rendered: 0,
        }
    }
}

impl Render for ConsoleView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        // Poll for new output.
        {
            let count = self.lines.lock().unwrap().len();
            if count != self.rendered {
                self.rendered = count;
                if self.done.load(Ordering::SeqCst) {
                    self.status = "Finished".into();
                } else {
                    self.status = format!("Running ({count} lines)").into();
                }
                cx.notify();
                if !self.done.load(Ordering::SeqCst) {
                    cx.spawn(async move |this, cx| {
                        Timer::after(Duration::from_millis(200)).await;
                        this.update(cx, |_this, cx| cx.notify()).ok();
                    })
                    .detach();
                }
            } else if !self.done.load(Ordering::SeqCst) {
                cx.spawn(async move |this, cx| {
                    Timer::after(Duration::from_millis(200)).await;
                    this.update(cx, |_this, cx| cx.notify()).ok();
                })
                .detach();
            }
        }

        let lines = self.lines.lock().unwrap();
        let tail: Vec<String> = lines.iter().rev().take(200).rev().cloned().collect();

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
                    .h(px(30.0))
                    .px_4()
                    .gap_4()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Console"),
                    )
                    .child(div().ml_auto().text_sm().text_color(rgb(theme.subtext)).child(self.status.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .p_2()
                    .font_family("monospace")
                    .text_sm()
                    .children(tail.into_iter().map(|l| {
                        div().child(l).into_any_element()
                    })),
            )
            // The button row of a log viewer.
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
                    .child(
                        div()
                            .id("console-copy")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .child("Copy")
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                let text = this.lines.lock().unwrap().join("\n");
                                cx.write_to_clipboard(ClipboardItem::new_string(text));
                                this.status = "Copied to the clipboard".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("console-close")
                            .ml_auto()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .child("Close")
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.quit();
                            })),
                    ),
            )
    }
}

impl crate::console::ConsoleView {
    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }
}

pub fn open_console(
    profile: LaunchProfile,
    name: String,
    theme: Theme,
    cx: &mut App,
) -> Option<gpui::WindowHandle<ConsoleView>> {
    let bounds = Bounds::centered(None, size(px(900.), px(500.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some(format!("Console — {name}").into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| ConsoleView::spawn_launch(profile, name.clone(), theme, cx)),
    )
    .ok()
}
