//! Themes. Mirrors launcher/ui/themes (DarkTheme, BrightTheme): the palette
//! is taken from Prism's own colour schemes so the launcher looks the same.

use gpui::Rgba;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    pub fn from_name(name: &str) -> Self {
        if name.eq_ignore_ascii_case("light") || name.eq_ignore_ascii_case("bright") {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        }
    }
}

/// Layout metrics, in the same spirit as Prism's `InstanceView` constants.
pub mod metrics {
    /// Height of the menu bar row.
    pub const MENUBAR_HEIGHT: f32 = 22.0;
    /// Height of the tool bar row.
    pub const TOOLBAR_HEIGHT: f32 = 30.0;
    /// Height of the news bar above the status bar.
    pub const NEWSBAR_HEIGHT: f32 = 26.0;
    /// Height of the status bar.
    pub const STATUSBAR_HEIGHT: f32 = 22.0;
    /// Width of one instance tile.
    pub const TILE_WIDTH: f32 = 100.0;
    /// Size of the icon on an instance tile.
    pub const TILE_ICON: f32 = 48.0;
    /// Margin around the instance grid.
    pub const GRID_MARGIN: f32 = 5.0;
    /// Spacing between two instance tiles.
    pub const GRID_SPACING: f32 = 5.0;
    /// Width of the instance tool bar on the right.
    pub const INSTANCE_TOOLBAR_WIDTH: f32 = 170.0;
    /// Width of the page list in the instance window.
    pub const PAGE_LIST_WIDTH: f32 = 190.0;
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub mode: ThemeMode,
    /// `QPalette::Window` — the background of the window.
    pub background: u32,
    /// `QPalette::Base` — the background of list views.
    pub base: u32,
    /// `QPalette::AlternateBase` — the background of every other row.
    pub alternate: u32,
    /// `QPalette::Button` — tool bars and buttons.
    pub surface: u32,
    /// `QPalette::Text`.
    pub text: u32,
    /// Dimmed text, like `QPalette::PlaceholderText`.
    pub subtext: u32,
    /// `QPalette::Highlight` — the selection colour.
    pub accent: u32,
    /// `QPalette::HighlightedText`.
    pub on_accent: u32,
    /// `QPalette::Link`.
    pub link: u32,
    /// Frame colour for the panels.
    pub border: u32,
}

impl Theme {
    /// Prism's dark theme (`launcher/ui/themes/DarkTheme.cpp`).
    pub fn dark() -> Self {
        Theme {
            mode: ThemeMode::Dark,
            background: 0x313131,
            base: 0x222222,
            alternate: 0x2a2a2a,
            surface: 0x303030,
            text: 0xffffff,
            subtext: 0x9a9a9a,
            accent: 0x96db59,
            on_accent: 0x000000,
            link: 0x2fa3c6,
            border: 0x3f3f3f,
        }
    }

    /// Prism's bright theme (`launcher/ui/themes/BrightTheme.cpp`).
    pub fn light() -> Self {
        Theme {
            mode: ThemeMode::Light,
            background: 0xffffff,
            base: 0xfafafa,
            alternate: 0xf0f0f0,
            surface: 0xf9f9f9,
            text: 0x111111,
            subtext: 0x707070,
            accent: 0x89cf54,
            on_accent: 0x000000,
            link: 0x2589a4,
            border: 0xd6d6d6,
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

    /// A colour with an alpha channel.
    ///
    /// Qt draws the unselected entries of the instance grid with the window
    /// colour at 160/255 alpha, so the same is needed here.
    pub fn rgba(&self, color: u32, alpha: f32) -> Rgba {
        let [r, g, b, _] = color.to_be_bytes();
        Rgba {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: alpha,
        }
    }
}
