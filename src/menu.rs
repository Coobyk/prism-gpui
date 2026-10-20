//! The main window's menu bar and its drop-down menus.
//!
//! Mirrors `launcher/ui/MainWindow.ui`: the menu bar holds File, Edit, View,
//! Folders, Accounts and Help, each opening a drop-down with the same items
//! Prism registers.

use crate::theme::Theme;
use gpui::{
    App, Bounds, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Window,
    WindowBounds, WindowKind, WindowOptions, div, prelude::*, px, rgb,
};

/// An action a menu entry performs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuAction {
    AddInstance,
    Rename,
    Launch,
    Kill,
    Edit,
    ChangeGroup,
    ViewInstanceFolder,
    Export,
    Copy,
    Delete,
    CreateShortcut,
    Settings,
    CloseWindow,
    UndoTrash,
    Themes,
    Cat,
    ToggleStatusBar,
    LockToolbars,
    ViewLauncherRoot,
    ViewLogs,
    ViewIcons,
    ViewCentralMods,
    ViewJava,
    ViewSkins,
    ViewWidgetTheme,
    ViewIconTheme,
    ViewCatPack,
    Accounts,
    ClearMetadata,
    ReportBug,
    ViewLog,
    AddToPath,
    Matrix,
    Discord,
    Reddit,
    MoreNews,
    CheckUpdate,
    OpenWiki,
    About,
}

/// One entry of a drop-down menu.
#[derive(Clone, Copy)]
pub struct MenuItem {
    pub label: &'static str,
    pub action: MenuAction,
    /// Whether the entry is only usable with a selected instance.
    pub needs_instance: bool,
    /// Whether a separator line follows the entry.
    pub separator: bool,
}

const fn item(label: &'static str, action: MenuAction) -> MenuItem {
    MenuItem {
        label,
        action,
        needs_instance: false,
        separator: false,
    }
}

const fn item_sep(label: &'static str, action: MenuAction) -> MenuItem {
    MenuItem {
        label,
        action,
        needs_instance: false,
        separator: true,
    }
}

const fn inst(label: &'static str, action: MenuAction) -> MenuItem {
    MenuItem {
        label,
        action,
        needs_instance: true,
        separator: false,
    }
}

/// The entries of the "File" menu.
pub const FILE_MENU: &[MenuItem] = &[
    item("Add Instance...", MenuAction::AddInstance),
    item_sep("", MenuAction::AddInstance),
    inst("Launch", MenuAction::Launch),
    inst("Kill", MenuAction::Kill),
    item_sep("", MenuAction::Launch),
    inst("Edit...", MenuAction::Edit),
    inst("Change Group...", MenuAction::ChangeGroup),
    inst("Folder", MenuAction::ViewInstanceFolder),
    inst("Export...", MenuAction::Export),
    inst("Copy...", MenuAction::Copy),
    inst("Delete", MenuAction::Delete),
    inst("Create Shortcut", MenuAction::CreateShortcut),
    item_sep("", MenuAction::CreateShortcut),
    item("Settings...", MenuAction::Settings),
    item("Close Window", MenuAction::CloseWindow),
];

/// The entries of the "Edit" menu.
pub const EDIT_MENU: &[MenuItem] = &[item("Undo Last Instance Deletion", MenuAction::UndoTrash)];

/// The entries of the "View" menu.
pub const VIEW_MENU: &[MenuItem] = &[
    item("Themes", MenuAction::Themes),
    item_sep("", MenuAction::Themes),
    item("Meow", MenuAction::Cat),
    item("Status Bar", MenuAction::ToggleStatusBar),
    item("Lock Toolbars", MenuAction::LockToolbars),
];

/// The entries of the "Folders" menu.
pub const FOLDERS_MENU: &[MenuItem] = &[
    item("View Launcher Root Folder", MenuAction::ViewLauncherRoot),
    item("View Logs Folder", MenuAction::ViewLogs),
    item_sep("", MenuAction::ViewLogs),
    inst("View Instance Folder", MenuAction::ViewInstanceFolder),
    item("View Icons Folder", MenuAction::ViewIcons),
    item("View Central Mods Folder", MenuAction::ViewCentralMods),
    item("View Java Folder", MenuAction::ViewJava),
    item("View Skins Folder", MenuAction::ViewSkins),
    item_sep("", MenuAction::ViewSkins),
    item("View Widget Theme Folder", MenuAction::ViewWidgetTheme),
    item("View Icon Theme Folder", MenuAction::ViewIconTheme),
    item("View Cat Pack Folder", MenuAction::ViewCatPack),
];

/// The entries of the "Accounts" menu.
pub const ACCOUNTS_MENU: &[MenuItem] = &[item("Accounts", MenuAction::Accounts)];

/// The entries of the "Help" menu.
pub const HELP_MENU: &[MenuItem] = &[
    item("Clear Metadata", MenuAction::ClearMetadata),
    item("Report Bug", MenuAction::ReportBug),
    item("View Log", MenuAction::ViewLog),
    item("Add to PATH", MenuAction::AddToPath),
    item_sep("", MenuAction::AddToPath),
    item("Matrix", MenuAction::Matrix),
    item("Discord", MenuAction::Discord),
    item("Reddit", MenuAction::Reddit),
    item_sep("", MenuAction::Reddit),
    item("More News...", MenuAction::MoreNews),
    item("Update...", MenuAction::CheckUpdate),
    item("Open Wiki", MenuAction::OpenWiki),
    item("About", MenuAction::About),
];

/// A menu of the menu bar: a title and its entries.
pub struct Menu {
    pub title: &'static str,
    pub items: &'static [MenuItem],
}

/// The menus of the menu bar, in the order Prism shows them.
pub fn menus() -> Vec<Menu> {
    vec![
        Menu {
            title: "File",
            items: FILE_MENU,
        },
        Menu {
            title: "Edit",
            items: EDIT_MENU,
        },
        Menu {
            title: "View",
            items: VIEW_MENU,
        },
        Menu {
            title: "Folders",
            items: FOLDERS_MENU,
        },
        Menu {
            title: "Accounts",
            items: ACCOUNTS_MENU,
        },
        Menu {
            title: "Help",
            items: HELP_MENU,
        },
    ]
}

/// The drop-down window that shows the entries of one menu.
pub struct MenuView {
    title: SharedString,
    items: Vec<MenuItem>,
    /// The main window the actions are performed on.
    main: Entity<crate::MainWindow>,
    /// Whether an instance is selected; instance-only entries are dimmed.
    has_instance: bool,
    theme: Theme,
}

impl MenuView {
    pub fn new(
        title: SharedString,
        items: Vec<MenuItem>,
        main: Entity<crate::MainWindow>,
        has_instance: bool,
        theme: Theme,
    ) -> Self {
        Self {
            title,
            items,
            main,
            has_instance,
            theme,
        }
    }

    /// The size the drop-down needs.
    pub fn window_size(&self) -> (f32, f32) {
        let rows = self.items.len() as f32;
        // Every entry is 22 pixels tall, separators 5.
        let separators = self.items.iter().filter(|i| i.separator).count() as f32;
        let height = 8.0 + rows * 22.0 + separators * 5.0;
        (210.0, height)
    }
}

impl Render for MenuView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        for (i, entry) in self.items.iter().enumerate() {
            if entry.separator && i > 0 {
                rows.push(
                    div()
                        .h(px(5.0))
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .into_any_element(),
                );
            }
            let enabled = !entry.needs_instance || self.has_instance;
            let action = entry.action;
            let main = self.main.clone();
            rows.push(
                div()
                    .id(("menu-item", i))
                    .flex()
                    .items_center()
                    .h(px(22.0))
                    .px_3()
                    .cursor_pointer()
                    .bg(rgb(theme.surface))
                    .child(
                        div()
                            .text_sm()
                            .text_color(if enabled {
                                rgb(theme.text)
                            } else {
                                rgb(theme.subtext)
                            })
                            .child(entry.label),
                    )
                    .on_click(move |_, _, cx| {
                        main.update(cx, |main, cx| {
                            main.run_action(action, cx);
                        });
                        cx.quit();
                    })
                    .into_any_element(),
            );
        }

        div()
            .flex()
            .flex_col()
            .w(px(210.))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.border))
            .py_1()
            .children(rows)
    }
}

/// Open the drop-down of a menu under the given position.
#[allow(clippy::too_many_arguments)]
pub fn open_menu(
    title: &'static str,
    items: &'static [MenuItem],
    main: Entity<crate::MainWindow>,
    has_instance: bool,
    theme: Theme,
    position: gpui::Point<gpui::Pixels>,
    window_bounds: Bounds<gpui::Pixels>,
    cx: &mut App,
) {
    let view = MenuView::new(title.into(), items.to_vec(), main, has_instance, theme);
    let (w, h) = view.window_size();
    let x = window_bounds.origin.x + position.x;
    let y = window_bounds.origin.y + position.y;
    let bounds = Bounds::new(gpui::Point::new(x, y), gpui::size(px(w), px(h)));
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            kind: WindowKind::PopUp,
            focus: false,
            ..Default::default()
        },
        move |_, cx| cx.new(|_cx| view),
    )
    .ok();
}
