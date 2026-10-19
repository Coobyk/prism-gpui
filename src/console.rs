//! Console window: launches an instance and streams its output.
//! Mirrors launcher/ui/ViewLogWindow + launcher/console.

use crate::launch::LaunchProfile;
use crate::theme::Theme;
use gpui::{
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
            let mut cmd = profile.jvm_command();
            match cmd.spawn() {
                Ok(mut child) => {
                    let out = child.stdout.take();
                    let err = child.stderr.take();
                    let l1 = lines_bg.clone();
                    let t1 = std::thread::spawn(move || {
                        if let Some(out) = out {
                            for line in BufReader::new(out).lines().map_while(Result::ok) {
                                l1.lock().unwrap().push(line);
                            }
                        }
                    });
                    let l2 = lines_bg.clone();
                    let t2 = std::thread::spawn(move || {
                        if let Some(err) = err {
                            for line in BufReader::new(err).lines().map_while(Result::ok) {
                                l2.lock().unwrap().push(line);
                            }
                        }
                    });
                    let _ = t1.join();
                    let _ = t2.join();
                    let code = child.wait().map(|s| s.to_string()).unwrap_or_default();
                    lines_bg.lock().unwrap().push(format!("--- process exited: {code} ---"));
                }
                Err(e) => {
                    lines_bg.lock().unwrap().push(format!("Failed to start: {e}"));
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
                    .h(px(36.0))
                    .px_4()
                    .gap_4()
                    .bg(rgb(theme.surface))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child("Console")
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
