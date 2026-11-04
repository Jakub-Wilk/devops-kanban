//! Inline stroke icons (24×24 viewBox, `currentColor`), as SVG markup.

macro_rules! icon {
    ($name:ident, $size:literal, $sw:literal, $body:literal) => {
        pub const $name: &str = concat!(
            "<svg width=\"",
            $size,
            "\" height=\"",
            $size,
            "\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"",
            $sw,
            "\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\">",
            $body,
            "</svg>"
        );
    };
}

icon!(CHEVRON_DOWN, "14", "2", "<path d=\"M6 9l6 6 6-6\"/>");
icon!(CHEVRON_RIGHT, "16", "2.4", "<path d=\"M9 6l6 6-6 6\"/>");
icon!(
    CALENDAR,
    "14",
    "2",
    "<rect x=\"3\" y=\"5\" width=\"18\" height=\"16\" rx=\"2\"/><path d=\"M3 10h18M8 3v4M16 3v4\"/>"
);
icon!(
    CALENDAR_16,
    "16",
    "2",
    "<rect x=\"3\" y=\"5\" width=\"18\" height=\"16\" rx=\"2\"/><path d=\"M3 10h18M8 3v4M16 3v4\"/>"
);
icon!(
    LIST,
    "16",
    "2",
    "<path d=\"M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01\"/>"
);
icon!(
    KANBAN,
    "16",
    "2",
    "<rect x=\"3\" y=\"4\" width=\"5\" height=\"16\" rx=\"1.5\"/><rect x=\"10\" y=\"4\" width=\"5\" height=\"10\" rx=\"1.5\"/><rect x=\"17\" y=\"4\" width=\"4\" height=\"13\" rx=\"1.5\"/>"
);
icon!(TIMELINE, "16", "2.6", "<path d=\"M4 6h8M8 12h12M6 18h9\"/>");
icon!(
    SEARCH,
    "16",
    "2",
    "<circle cx=\"11\" cy=\"11\" r=\"7\"/><path d=\"M20 20l-3.5-3.5\"/>"
);
icon!(PLUS, "16", "2.4", "<path d=\"M12 5v14M5 12h14\"/>");
icon!(PLUS_14, "14", "2.4", "<path d=\"M12 5v14M5 12h14\"/>");
icon!(
    FOLDER,
    "14",
    "2",
    "<path d=\"M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z\"/>"
);
icon!(
    ASSIGN,
    "16",
    "2.2",
    "<circle cx=\"10\" cy=\"8\" r=\"4\"/><path d=\"M2 21c0-4 3.5-7 8-7M18 14v6M15 17h6\"/>"
);
icon!(CLOSE, "18", "2.2", "<path d=\"M6 6l12 12M18 6L6 18\"/>");
icon!(
    GEAR,
    "18",
    "2",
    "<circle cx=\"12\" cy=\"12\" r=\"3\"/><path d=\"M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z\"/>"
);
icon!(
    COLUMNS,
    "16",
    "2",
    "<rect x=\"3\" y=\"4\" width=\"18\" height=\"16\" rx=\"2\"/><path d=\"M9 4v16M15 4v16\"/>"
);
icon!(
    REFRESH,
    "16",
    "2",
    "<path d=\"M21 12a9 9 0 1 1-2.6-6.4L21 8\"/><path d=\"M21 3v5h-5\"/>"
);
icon!(
    GRIP,
    "14",
    "2.4",
    "<path d=\"M9 6h.01M15 6h.01M9 12h.01M15 12h.01M9 18h.01M15 18h.01\"/>"
);
icon!(
    INFO,
    "16",
    "2",
    "<circle cx=\"12\" cy=\"12\" r=\"9\"/><path d=\"M12 11v5M12 8h.01\"/>"
);
icon!(
    ALERT,
    "16",
    "2",
    "<path d=\"M10.3 3.9L1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z\"/><path d=\"M12 9v4M12 17h.01\"/>"
);
icon!(BACK, "16", "2", "<path d=\"M15 18l-6-6 6-6\"/>");

/// The three-bar logo, in the accent colour.
pub const LOGO: &str = "<svg width=\"28\" height=\"28\" viewBox=\"0 0 28 28\" aria-hidden=\"true\"><rect x=\"2\" y=\"3\" width=\"6\" height=\"22\" rx=\"2\" fill=\"#7C9BFF\"/><rect x=\"11\" y=\"3\" width=\"6\" height=\"14\" rx=\"2\" fill=\"#7C9BFF\" opacity=\"0.7\"/><rect x=\"20\" y=\"3\" width=\"6\" height=\"9\" rx=\"2\" fill=\"#7C9BFF\" opacity=\"0.45\"/></svg>";
