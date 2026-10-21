//! The icons of the tool bars.
//!
//! Prism's tool buttons show an icon next to the text, so the launcher needs
//! a small icon set as well. The icons are simple strokes on a 16x16 canvas,
//! drawn in the colour of the text so they fit both themes.

use gpui::Image;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The paths of the icons, drawn on a 16x16 canvas.
fn paths(name: &str) -> Option<&'static str> {
    Some(match name {
        "add" => "M8 2.5v11M2.5 8h11",
        "folder" => "M1.5 12.5v-8h4l1.5 2h7.5v6.5z",
        "settings" => {
            "M8 4.6a3.4 3.4 0 1 0 0 6.8 3.4 3.4 0 0 0 0-6.8M8 1.2v1.6M8 13.2v1.6M1.2 8h1.6M13.2 8h1.6M3.2 3.2l1.2 1.2M11.6 11.6l1.2 1.2M12.8 3.2l-1.2 1.2M4.4 11.6l-1.2 1.2"
        }
        "help" => {
            "M8 1.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13M6.1 6.1a1.9 1.9 0 1 1 2.5 1.6c-.6.4-.9.8-.9 1.6M8 11.2v.1"
        }
        "update" => "M8 1.8v8.4M4.4 6.6L8 10.2l3.6-3.6M2.4 13.2h11.2",
        "account" => {
            "M8 2.4a2.8 2.8 0 1 0 0 5.6 2.8 2.8 0 0 0 0-5.6M2.6 14c0-3 2.4-4.8 5.4-4.8s5.4 1.8 5.4 4.8"
        }
        "meow" => {
            "M3.2 6.2l.5 2.4M12.8 6.2l-.5 2.4M8 4.2a4.6 4.6 0 1 0 0 9.2 4.6 4.6 0 0 0 0-9.2M6.2 8.2v.1M9.8 8.2v.1M6.2 10.4c.5.6 1.1.9 1.8.9s1.3-.3 1.8-.9"
        }
        "news" => "M2 3.6h12M2 6.6h12M2 9.6h9M2 12.6h9",
        "rename" => "M2.4 13.6l.8-2.9L11 2.9l2.1 2.1-7.8 7.8zM9.6 4.3l2.1 2.1",
        "launch" => "M4.4 2.6l8.4 5.4-8.4 5.4z",
        "kill" => "M4 4h8v8H4z",
        "edit" => {
            "M2 5h4.6M9.4 5H14M2 11h4.6M9.4 11H14M8.3 5a1.3 1.3 0 1 0 0 2.6 1.3 1.3 0 0 0 0-2.6M8.3 9.4a1.3 1.3 0 1 0 0 2.6 1.3 1.3 0 0 0 0-2.6"
        }
        "group" => "M2 2.2h5.2l6.6 6.6-5.2 5.2L2 7.4zM5 5.2v.1",
        "export" => "M2.4 4.6h11.2v8H2.4zM8 12V3.6M5.4 6.2L8 3.6l2.6 2.6",
        "copy" => "M5.6 1.6h7.2v7.2H5.6zM3.2 5.4v7.2h7.2",
        "delete" => {
            "M2.6 4.4h10.8M6.4 4.4V2.8h3.2v1.6M3.8 4.4l.7 9.2h7l.7-9.2M6.4 6.6v4.6M9.6 6.6v4.6"
        }
        "shortcut" => "M8.6 2.8h4.6v4.6M13.2 2.8L6.6 9.4M12.4 9.6V13.2H3.2V4h3.6",
        "instance" => "M8 1.4l6.2 3.2v6.8L8 14.6l-6.2-3.2V4.6zM1.8 4.6L8 7.8l6.2-3.2M8 7.8v6.8",
        "console" => "M1.4 2.8h13.2v10.4H1.4zM4 6.4l2 1.6-2 1.6M7.6 10.4h4",
        "mod" => "M8 2.2l5 2.9v5.8l-5 2.9-5-2.9V5.1zM3 5.1l5 2.9 5-2.9M8 8v5.8",
        "world" => {
            "M8 1.6a6.4 6.4 0 1 0 0 12.8 6.4 6.4 0 0 0 0-12.8M1.6 8h12.8M8 1.6c1.8 1.8 2.6 4 2.6 6.4S9.8 12.6 8 14.4C6.2 12.6 5.4 10.4 5.4 8S6.2 3.4 8 1.6"
        }
        "screenshot" => {
            "M1.4 3.6h13.2v8.8H1.4zM5.2 3.6l1-1.6h3.6l1 1.6M8 6a2 2 0 1 0 0 4 2 2 0 0 0 0-4"
        }
        "notes" => "M3.4 1.6h9.2v12.8H3.4zM5.6 4.4h4.8M5.6 7h4.8M5.6 9.6h3.2",
        "servers" => "M1.8 2.6h12.4v4.2H1.8zM1.8 9.2h12.4v4.2H1.8M4.2 4.7v.1M4.2 11.3v.1",
        "settings2" => "M8 1.6a6.4 6.4 0 1 0 0 12.8 6.4 6.4 0 0 0 0-12.8",
        "search" => "M7 1.8a5.2 5.2 0 1 0 0 10.4 5.2 5.2 0 0 0 0-10.4M10.6 10.6l3.6 3.6",
        "check" => "M3.2 8.4l3.2 3.2 6.4-7.2",
        "picture" => "M1.6 3.2h12.8v9.6H1.6zM1.6 10.2l3.4-3.2 2.6 2.4 2.4-2.8 4.4 4.2M4.6 6.2v.1",
        "wand" => {
            "M2.6 13.4l8-8M10.6 2.2l.7 1.7 1.7.7-1.7.7-.7 1.7-.7-1.7-1.7-.7 1.7-.7zM4.2 2.6l.5 1.1 1.1.5-1.1.5-.5 1.1-.5-1.1-1.1-.5 1.1-.5z"
        }
        _ => return None,
    })
}

/// The SVG of an icon in the given colour.
pub fn svg(name: &str, colour: u32) -> Option<Vec<u8>> {
    let paths = paths(name)?;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"#{colour:06x}\" stroke-width=\"1.3\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"{paths}\"/></svg>"
    );
    Some(svg.into_bytes())
}

/// The rendered icon, cached per name and colour.
pub fn icon(name: &str, colour: u32) -> Option<std::sync::Arc<Image>> {
    type Cache = HashMap<(String, u32), Option<std::sync::Arc<Image>>>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (name.to_string(), colour);
    if let Some(found) = cache.lock().unwrap().get(&key) {
        return found.clone();
    }
    let image = svg(name, colour)
        .map(|bytes| std::sync::Arc::new(Image::from_bytes(gpui::ImageFormat::Svg, bytes)));
    cache.lock().unwrap().insert(key, image.clone());
    image
}
