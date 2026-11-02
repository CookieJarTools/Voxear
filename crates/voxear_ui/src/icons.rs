//! Voxear's icons: drawn on a 24-unit square as SVG path data, stroked
//! 1.5 pixels wide with round caps and joins at whatever size they are shown,
//! or filled. They are rasterised into the glyph atlas at their exact pixel
//! size, as text is, so they stay crisp. Each is drawn in its element's text
//! colour, so a sheet colours it with `color`.

/// One shape of an icon: its path, and whether it is filled rather than stroked.
pub(crate) type Part = (&'static str, bool);

/// The square every path is drawn on.
pub(crate) const GRID: f32 = 24.0;

const STROKE: bool = false;
const FILL: bool = true;

/// An icon's name as kept, and its shapes, or `None` for a name there is no
/// icon for.
pub(crate) fn parts(name: &str) -> Option<(&'static str, &'static [Part])> {
    NAMES.iter().find(|&&known| known == name).and_then(|&known| Some((known, shapes(known)?)))
}

/// Every icon there is, for [`crate::Ui::icon`]'s users to choose from.
pub const NAMES: &[&str] = &[
    "chevron-right",
    "chevron-down",
    "check",
    "plus",
    "close",
    "more",
    "search",
    "play",
    "pause",
    "stop",
    "step",
    "logo-top",
    "logo-left",
    "logo-right",
    "cube",
    "box",
    "code",
    "audio",
    "gamepad",
    "profiler",
    "branch",
    "select",
    "move",
    "rotate",
    "scale",
    "pivot",
    "globe",
    "grid",
    "camera",
    "light",
    "obj",
    "folder",
    "file",
    "level",
    "scene",
    "material",
    "texture",
    "eye",
    "transform",
    "lod",
    "maximize",
    "chevron-left",
    "book",
    "info",
    "alert",
    "copy",
    "edit",
    "settings",
    "panels",
    "lock",
    "grip",
];

fn shapes(name: &str) -> Option<&'static [Part]> {
    Some(match name {
        "chevron-right" => &[("M9.5 6l6 6-6 6", STROKE)],
        "chevron-down" => &[("M6 9.5l6 6 6-6", STROKE)],
        "check" => &[("M5.5 12.5l4.5 4.5 8.5-9.5", STROKE)],
        "plus" => &[("M12 5v14M5 12h14", STROKE)],
        "close" => &[("M6.5 6.5l11 11M17.5 6.5l-11 11", STROKE)],
        "more" => &[(
            "M3.5 12a1.75 1.75 0 1 0 3.5 0a1.75 1.75 0 1 0-3.5 0ZM10.25 12a1.75 1.75 0 1 0 3.5 0a1.75 1.75 0 1 0-3.5 0Z\
             M17 12a1.75 1.75 0 1 0 3.5 0a1.75 1.75 0 1 0-3.5 0Z",
            FILL,
        )],
        "search" => &[("M10.5 4a6.5 6.5 0 1 0 0 13a6.5 6.5 0 1 0 0-13ZM15.5 15.5l4.5 4.5", STROKE)],
        "play" => &[("M7.5 5.6v12.8a1 1 0 0 0 1.5.87l10.6-6.4a1 1 0 0 0 0-1.74L9 4.73a1 1 0 0 0-1.5.87Z", FILL)],
        "pause" => &[("M6.5 5h3.5v14H6.5ZM14 5h3.5v14H14Z", FILL)],
        "stop" => &[("M7 6h10a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1Z", FILL)],
        "step" => {
            &[("M5 5.6v12.8a1 1 0 0 0 1.5.87l9.4-6.4a1 1 0 0 0 0-1.74L6.5 4.73A1 1 0 0 0 5 5.6ZM17 5h3v14h-3Z", FILL)]
        }
        // The Voxear mark: an isometric cube, a face per shade.
        "logo-top" => &[("M12 2.5l8.5 4.75L12 12 3.5 7.25Z", FILL)],
        "logo-left" => &[("M3.5 7.25L12 12v9.5l-8.5-4.75Z", FILL)],
        "logo-right" => &[("M20.5 7.25L12 12v9.5l8.5-4.75Z", FILL)],
        "cube" => &[("M12 2.75l8.25 4.6v9.3L12 21.25l-8.25-4.6v-9.3ZM3.75 7.35L12 12l8.25-4.65M12 12v9.25", STROKE)],
        "box" => &[("M4 8.5h16V19a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1ZM3 4.5h18v4H3ZM10 12.5h4", STROKE)],
        "code" => &[("M8.5 7l-5 5 5 5M15.5 7l5 5-5 5M13.5 4.5l-3 15", STROKE)],
        "audio" => &[("M4 9.5h3.5L12 5.5v13l-4.5-4H4ZM15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11", STROKE)],
        // A gamepad: its body, a cross on the left and two buttons on the right.
        "gamepad" => &[
            (
                "M7 6.5h10a4 4 0 0 1 3.95 3.4l.8 5.3a2.6 2.6 0 0 1-4.5 2.2L15.5 15.5h-7l-1.75 1.9a2.6 2.6 0 0 1-4.5-2.2\
                 l.8-5.3A4 4 0 0 1 7 6.5ZM8 9.5v4M6 11.5h4",
                STROKE,
            ),
            ("M15.25 10.25a1 1 0 1 0 2 0a1 1 0 1 0-2 0ZM17.25 12.75a1 1 0 1 0 2 0a1 1 0 1 0-2 0Z", FILL),
        ],
        "profiler" => &[("M3 12.5h4l3-7.5 4 14 3-6.5h4", STROKE)],
        "branch" => &[(
            "M6.5 3.5v11M17.5 8.5a2.5 2.5 0 1 0 0-5a2.5 2.5 0 1 0 0 5ZM6.5 20.5a2.5 2.5 0 1 0 0-5a2.5 2.5 0 1 0 0 5Z\
             M17.5 8.5a8 8 0 0 1-8 8h-1",
            STROKE,
        )],
        "select" => &[("M5.5 3.5l13 6.5-6 1.75-2.25 6.25Z", STROKE)],
        "move" => &[(
            "M12 3.5v17M3.5 12h17M12 3.5l-2.5 2.5M12 3.5l2.5 2.5M12 20.5l-2.5-2.5M12 20.5l2.5-2.5\
             M3.5 12l2.5-2.5M3.5 12l2.5 2.5M20.5 12l-2.5-2.5M20.5 12l-2.5 2.5",
            STROKE,
        )],
        "rotate" => &[("M19.5 12a7.5 7.5 0 1 1-2.2-5.3M19.5 4.5v5h-5", STROKE)],
        "scale" => &[("M14 4h6v6M20 4l-7.5 7.5M10 5H5.5a1 1 0 0 0-1 1v12.5a1 1 0 0 0 1 1H18a1 1 0 0 0 1-1V14", STROKE)],
        "pivot" => &[("M5 12a7 7 0 1 0 14 0a7 7 0 1 0-14 0ZM12 2.5v4M12 17.5v4M2.5 12h4M17.5 12h4", STROKE)],
        "globe" => &[(
            "M3.5 12a8.5 8.5 0 1 0 17 0a8.5 8.5 0 1 0-17 0ZM3.5 12h17M12 3.5a13 13 0 0 1 0 17M12 3.5a13 13 0 0 0 0 17",
            STROKE,
        )],
        "grid" => &[("M4.5 4.5h15v15h-15ZM4.5 9.5h15M4.5 14.5h15M9.5 4.5v15M14.5 4.5v15", STROKE)],
        "camera" => &[(
            "M3.5 8.5a2 2 0 0 1 2-2h2.25l1.5-2h5.5l1.5 2h2.25a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2Z\
             M12 9.5a3.25 3.25 0 1 0 0 6.5a3.25 3.25 0 1 0 0-6.5Z",
            STROKE,
        )],
        "light" => &[(
            "M12 8.5a3.5 3.5 0 1 0 0 7a3.5 3.5 0 1 0 0-7ZM12 2.5v2M12 19.5v2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4\
             M2.5 12h2M19.5 12h2M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4",
            STROKE,
        )],
        "obj" => &[("M12 4.5a7.5 7.5 0 1 0 0 15a7.5 7.5 0 1 0 0-15Z", STROKE)],
        "folder" => &[(
            "M3.5 7a1.5 1.5 0 0 1 1.5-1.5h4.25l2 2.5h7.75a1.5 1.5 0 0 1 1.5 1.5v8.5a1.5 1.5 0 0 1-1.5 1.5H5a1.5 1.5 0 0 1-1.5-1.5Z",
            STROKE,
        )],
        "file" => {
            &[("M6.5 3.5h7.5l4.5 4.5v11.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1v-15a1 1 0 0 1 1-1ZM14 3.5V8h4.5", STROKE)]
        }
        "level" => &[("M12 3.5l8.5 4.25L12 12 3.5 7.75ZM3.5 12L12 16.25 20.5 12M3.5 16.25L12 20.5l8.5-4.25", STROKE)],
        "scene" => &[("M3.5 6.5l5.5-3 6 3 5.5-3v14l-5.5 3-6-3-5.5 3ZM9 3.5v14M15 6.5v14", STROKE)],
        "material" => &[("M12 3.5a8.5 8.5 0 1 0 0 17a8.5 8.5 0 1 0 0-17ZM7.5 10a5 5 0 0 1 4-3.5", STROKE)],
        "texture" => &[(
            "M4 5h16v14H4ZM4 15.5l4.5-4.5 4 4 2-2 5.5 5.5M14.5 9a1.25 1.25 0 1 0 2.5 0a1.25 1.25 0 1 0-2.5 0Z",
            STROKE,
        )],
        "eye" => &[(
            "M2.5 12s3.5-6.5 9.5-6.5 9.5 6.5 9.5 6.5-3.5 6.5-9.5 6.5S2.5 12 2.5 12ZM12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6Z",
            STROKE,
        )],
        // Three of one shape, each smaller: levels of detail.
        "lod" => &[("M2.5 7.5h9v9h-9ZM14 9.5h5v5h-5ZM21 11h1.5v2H21Z", STROKE)],
        "transform" => &[("M5.5 4v14.5H20M5.5 18.5l10-10M15.5 8.5h-4M15.5 8.5v4", STROKE)],
        "maximize" => &[("M4.5 9.5v-5h5M14.5 4.5h5v5M19.5 14.5v5h-5M9.5 19.5h-5v-5", STROKE)],
        "chevron-left" => &[("M14.5 6l-6 6 6 6", STROKE)],
        "book" => &[("M5 5.5a2 2 0 0 1 2-2h11.5v14H7a2 2 0 0 0-2 2ZM5 19.5a2 2 0 0 0 2 2h11.5v-4M9 7.5h6", STROKE)],
        "info" => &[("M12 3.5a8.5 8.5 0 1 0 0 17a8.5 8.5 0 1 0 0-17ZM12 11v5.5M12 7.75v.25", STROKE)],
        "alert" => &[("M12 4l9 15.5H3ZM12 10v4.5M12 17.25v.25", STROKE)],
        "copy" => &[("M9 9h10.5v10.5H9ZM15 9V4.5H4.5V15H9", STROKE)],
        "edit" => &[("M4.5 19.5l1-4.5L15.5 5a2.1 2.1 0 0 1 3 3l-10 10ZM13.5 7l3 3", STROKE)],
        // A window split into panels, for the Window menu.
        "panels" => &[(
            "M4 5.25h16a.75.75 0 0 1 .75.75v12a.75.75 0 0 1-.75.75H4a.75.75 0 0 1-.75-.75V6A.75.75 0 0 1 4 5.25ZM3.25 9.25h17.5M9.25 9.25v9.5",
            STROKE,
        )],
        "lock" => &[("M6 10.5h12v9H6ZM8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5M12 14v2", STROKE)],
        // Six dots, two by three: what drags an item of a list into a new place.
        "grip" => &[(
            "M7.5 6a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0-3 0ZM13.5 6a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0-3 0Z\
             M7.5 12a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0-3 0ZM13.5 12a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0-3 0Z\
             M7.5 18a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0-3 0ZM13.5 18a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0-3 0Z",
            FILL,
        )],
        // Two sliders, for settings.
        "settings" => &[(
            "M4 7.5h8.5M17.5 7.5H20M4 16.5h2.5M11.5 16.5H20\
             M15 5a2.5 2.5 0 1 0 0 5a2.5 2.5 0 1 0 0-5ZM9 14a2.5 2.5 0 1 0 0 5a2.5 2.5 0 1 0 0-5Z",
            STROKE,
        )],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    /// Every icon named exists and draws something of its own.
    #[test]
    fn icons_rasterise() {
        let mut atlas = crate::atlas::GlyphAtlas::new();
        let icons: Vec<_> = super::NAMES
            .iter()
            .map(|name| (name, atlas.icon(name, 14, 1.5).unwrap_or_else(|| panic!("{name} did not pack"))))
            .collect();
        let image = atlas.snapshot().expect("icons were drawn");
        for (name, icon) in icons {
            assert_eq!((icon.width, icon.height), (14, 14));
            let width = image.width;
            let ink: u32 = (icon.y..icon.y + 14)
                .flat_map(|y| (icon.x..icon.x + 14).map(move |x| (y * width + x) as usize))
                .map(|i| image.pixels[i] as u32)
                .sum();
            assert!(ink > 255 * 6, "{name} draws something, got {ink}");
        }
        assert!(super::parts("nonsense").is_none());
    }
}
