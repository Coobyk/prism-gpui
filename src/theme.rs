//! Themes. Mirrors launcher/ui/themes (Catppuccin-based palettes).

use gpui::Rgba;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    pub fn from_name(name: &str) -> Self {
        if name.eq_ignore_ascii_case("light") {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub mode: ThemeMode,
    pub background: u32,
    pub surface: u32,
    pub text: u32,
    pub subtext: u32,
    pub accent: u32,
    pub border: u32,
    pub on_accent: u32,
}

impl Theme {
    pub fn dark() -> Self {
        Theme {
            mode: ThemeMode::Dark,
            background: 0x1e1e2e,
            surface: 0x181825,
            text: 0xcdd6f4,
            subtext: 0xa6adc8,
            accent: 0x89b4fa,
            border: 0x313244,
            on_accent: 0x11111b,
        }
    }

    pub fn light() -> Self {
        Theme {
            mode: ThemeMode::Light,
            background: 0xeff1f5,
            surface: 0xe6e9ef,
            text: 0x4c4f69,
            subtext: 0x6c6f85,
            accent: 0x1e66f5,
            border: 0xccd0da,
            on_accent: 0xffffff,
        }
    }

    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Theme::dark(),
            ThemeMode::Light => Theme::light(),
        }
    }

    pub fn for_name(name: &str) -> Self {
        Theme::for_mode(ThemeMode::from_name(name))
    }
}

impl Theme {
    pub fn rgb(&self, color: u32) -> Rgba {
        gpui::rgb(color)
    }
}
