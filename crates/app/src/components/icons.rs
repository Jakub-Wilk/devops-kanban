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
icon!(CHECK, "22", "2.2", "<path d=\"M5 12.5l4.5 4.5L19 7.5\"/>");
icon!(
    CHECK_14,
    "14",
    "2.4",
    "<path d=\"M5 12.5l4.5 4.5L19 7.5\"/>"
);
icon!(PLAY, "14", "2.2", "<path d=\"M7 4.5v15l12.5-7.5z\"/>");
icon!(
    STOP,
    "14",
    "2.2",
    "<rect x=\"6\" y=\"6\" width=\"12\" height=\"12\" rx=\"2\"/>"
);
icon!(
    CLOCK_PLUS,
    "14",
    "2",
    "<circle cx=\"11\" cy=\"13\" r=\"8\"/><path d=\"M11 9v4l2.5 2.5M19 2v5M16.5 4.5h5\"/>"
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
icon!(
    CLOCK,
    "14",
    "2",
    "<circle cx=\"12\" cy=\"12\" r=\"9\"/><path d=\"M12 7v5l3 2\"/>"
);
icon!(ARROW_UP, "12", "2.4", "<path d=\"M12 19V5M6 11l6-6 6 6\"/>");

// Work item type icons, keyed by the icon id Azure DevOps reports for each type
// (`wit/workitemtypes` → `icon.id`). Never by type name: processes rename and add types.
icon!(
    TY_CROWN,
    "12",
    "2.2",
    "<path d=\"M3 18h18M4 8l4 4 4-7 4 7 4-4-2 10H6z\"/>"
);
icon!(
    TY_TROPHY,
    "12",
    "2.2",
    "<path d=\"M8 4h8v5a4 4 0 0 1-8 0zM8 6H5v1a3 3 0 0 0 3 3M16 6h3v1a3 3 0 0 1-3 3M12 13v4M8 20h8\"/>"
);
icon!(
    TY_BOOK,
    "12",
    "2.2",
    "<path d=\"M5 5a2 2 0 0 1 2-2h11v16H7a2 2 0 0 0-2 2zM5 21a2 2 0 0 1 2-2h11\"/>"
);
icon!(
    TY_CLIPBOARD,
    "12",
    "2.2",
    "<rect x=\"5\" y=\"4\" width=\"14\" height=\"17\" rx=\"2\"/><path d=\"M9 4V3h6v1M9 11h6M9 15h4\"/>"
);
icon!(
    TY_INSECT,
    "12",
    "2.2",
    "<path d=\"M9 8a3 3 0 0 1 6 0M8 8h8v6a4 4 0 0 1-8 0zM4 7l3 2M20 7l-3 2M3 14h5M16 14h5M5 20l3-2M19 20l-3-2\"/>"
);
icon!(
    TY_LIST,
    "12",
    "2.2",
    "<path d=\"M9 6h11M9 12h11M9 18h11M4 6h.01M4 12h.01M4 18h.01\"/>"
);
icon!(
    TY_CHECK_BOX,
    "12",
    "2.2",
    "<rect x=\"4\" y=\"4\" width=\"16\" height=\"16\" rx=\"3\"/><path d=\"M8.5 12l2.5 2.5 4.5-5\"/>"
);
icon!(
    TY_TRAFFIC_CONE,
    "12",
    "2.2",
    "<path d=\"M10 3h4l5 16H5zM7.5 11h9M4 21h16\"/>"
);
icon!(TY_DIAMOND, "12", "2.2", "<path d=\"M12 3l9 9-9 9-9-9z\"/>");
icon!(
    TY_STAR,
    "12",
    "2.2",
    "<path d=\"M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1-4.4-4.3 6.1-.9z\"/>"
);

/// The SVG for an ADO work item type icon id, if we have one.
pub fn type_icon(ado_icon_id: &str) -> Option<&'static str> {
    Some(match ado_icon_id {
        "icon_crown" => TY_CROWN,
        "icon_trophy" => TY_TROPHY,
        "icon_book" => TY_BOOK,
        "icon_clipboard" => TY_CLIPBOARD,
        "icon_insect" => TY_INSECT,
        "icon_list" => TY_LIST,
        "icon_check_box" => TY_CHECK_BOX,
        "icon_traffic_cone" => TY_TRAFFIC_CONE,
        "icon_diamond" => TY_DIAMOND,
        "icon_star" => TY_STAR,
        _ => return None,
    })
}
