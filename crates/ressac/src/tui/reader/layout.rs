//! The reader's columns for the pane's size (spec §6, Columns): three when wide, two in the
//! middle, one when narrow. Pure: a size in, rectangles out.

use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

/// The two looks of the reader (spec §2, Looks). Stored as `"native"` / `"antiope"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Look {
    #[default]
    Native,
    Antiope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Sources,
    Entries,
    Reader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutClass {
    Three,
    Two,
    One,
    TooSmall,
}

pub const MIN_WIDTH: u16 = 40;
pub const MIN_HEIGHT: u16 = 10;
pub const TWO_COLUMNS_FROM: u16 = 80;
pub const THREE_COLUMNS_FROM: u16 = 120;

pub fn layout_class(width: u16, height: u16) -> LayoutClass {
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        LayoutClass::TooSmall
    } else if width >= THREE_COLUMNS_FROM {
        LayoutClass::Three
    } else if width >= TWO_COLUMNS_FROM {
        LayoutClass::Two
    } else {
        LayoutClass::One
    }
}

/// Where each visible column goes; `top` and `bottom` are the status lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnRects {
    pub sources: Option<Rect>,
    pub entries: Option<Rect>,
    pub reader: Option<Rect>,
    pub top: Rect,
    pub bottom: Rect,
}

/// Column widths in cells: Native (spec §6) and Antiope (the app's cell widths, spec §8), with
/// the Antiope `▐` separator cell between columns.
struct Widths {
    sources: u16,
    entries: u16,
    gap: u16,
}

fn widths(look: Look) -> Widths {
    match look {
        Look::Native => Widths {
            sources: 26,
            entries: 40,
            gap: 0,
        },
        Look::Antiope => Widths {
            sources: 22,
            entries: 28,
            gap: 1,
        },
    }
}

pub fn columns(area: Rect, class: LayoutClass, focus: Column, look: Look) -> ColumnRects {
    let top = Rect {
        height: area.height.min(1),
        ..area
    };
    let bottom = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: area.height.min(1),
        ..area
    };
    let body = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(2),
        ..area
    };
    let w = widths(look);
    let (sources, entries, reader) = match class {
        LayoutClass::TooSmall => (None, None, None),
        LayoutClass::Three => (
            Some(slice(body, 0, w.sources)),
            Some(slice(body, w.sources + w.gap, w.entries)),
            Some(rest(body, w.sources + w.gap + w.entries + w.gap)),
        ),
        LayoutClass::Two => (
            (focus == Column::Sources).then(|| slice(body, 0, w.sources)),
            Some(slice(body, 0, w.entries)),
            Some(rest(body, w.entries + w.gap)),
        ),
        LayoutClass::One => match focus {
            Column::Sources => (Some(body), None, None),
            Column::Entries => (None, Some(body), None),
            Column::Reader => (None, None, Some(body)),
        },
    };
    ColumnRects {
        sources,
        entries,
        reader,
        top,
        bottom,
    }
}

fn slice(body: Rect, offset: u16, width: u16) -> Rect {
    let offset = offset.min(body.width);
    Rect {
        x: body.x + offset,
        width: width.min(body.width - offset),
        ..body
    }
}

fn rest(body: Rect, offset: u16) -> Rect {
    let offset = offset.min(body.width);
    Rect {
        x: body.x + offset,
        width: body.width - offset,
        ..body
    }
}

/// The text area inside a column: Native draws a one-cell border, Antiope none.
pub fn text_width(rect: Rect, look: Look) -> u16 {
    match look {
        Look::Native => rect.width.saturating_sub(2),
        Look::Antiope => rect.width,
    }
}

pub fn text_height(rect: Rect, look: Look) -> u16 {
    match look {
        Look::Native => rect.height.saturating_sub(2),
        Look::Antiope => rect.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u16, y: u16, width: u16, height: u16) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn the_class_follows_the_pane_size() {
        assert_eq!(layout_class(39, 30), LayoutClass::TooSmall);
        assert_eq!(layout_class(140, 9), LayoutClass::TooSmall);
        assert_eq!(layout_class(40, 10), LayoutClass::One);
        assert_eq!(layout_class(79, 24), LayoutClass::One);
        assert_eq!(layout_class(80, 24), LayoutClass::Two);
        assert_eq!(layout_class(119, 24), LayoutClass::Two);
        assert_eq!(layout_class(120, 24), LayoutClass::Three);
    }

    #[test]
    fn three_native_columns_and_the_status_lines() {
        let r = columns(
            rect(0, 0, 140, 30),
            LayoutClass::Three,
            Column::Entries,
            Look::Native,
        );
        assert_eq!(r.top, rect(0, 0, 140, 1));
        assert_eq!(r.bottom, rect(0, 29, 140, 1));
        assert_eq!(r.sources, Some(rect(0, 1, 26, 28)));
        assert_eq!(r.entries, Some(rect(26, 1, 40, 28)));
        assert_eq!(r.reader, Some(rect(66, 1, 74, 28)));
    }

    #[test]
    fn three_antiope_columns_leave_a_separator_cell() {
        let r = columns(
            rect(0, 0, 140, 30),
            LayoutClass::Three,
            Column::Reader,
            Look::Antiope,
        );
        assert_eq!(r.sources, Some(rect(0, 1, 22, 28)));
        assert_eq!(r.entries, Some(rect(23, 1, 28, 28)));
        assert_eq!(r.reader, Some(rect(52, 1, 88, 28)));
    }

    #[test]
    fn two_columns_show_the_sources_over_the_list_when_focused() {
        let r = columns(
            rect(0, 0, 100, 30),
            LayoutClass::Two,
            Column::Entries,
            Look::Native,
        );
        assert_eq!(r.sources, None);
        assert_eq!(r.entries, Some(rect(0, 1, 40, 28)));
        assert_eq!(r.reader, Some(rect(40, 1, 60, 28)));
        let r = columns(
            rect(0, 0, 100, 30),
            LayoutClass::Two,
            Column::Sources,
            Look::Native,
        );
        assert_eq!(r.sources, Some(rect(0, 1, 26, 28)));
        assert_eq!(r.entries, Some(rect(0, 1, 40, 28)));
    }

    #[test]
    fn one_column_is_the_focused_one() {
        let area = rect(0, 0, 70, 24);
        let body = Some(rect(0, 1, 70, 22));
        let r = columns(area, LayoutClass::One, Column::Reader, Look::Native);
        assert_eq!((r.sources, r.entries, r.reader), (None, None, body));
        let r = columns(area, LayoutClass::One, Column::Sources, Look::Native);
        assert_eq!((r.sources, r.entries, r.reader), (body, None, None));
    }

    #[test]
    fn too_small_shows_no_column() {
        let r = columns(
            rect(0, 0, 30, 8),
            LayoutClass::TooSmall,
            Column::Entries,
            Look::Native,
        );
        assert_eq!((r.sources, r.entries, r.reader), (None, None, None));
    }

    #[test]
    fn a_narrow_pane_never_overflows() {
        let r = columns(
            rect(0, 0, 120, 30),
            LayoutClass::Three,
            Column::Entries,
            Look::Native,
        );
        let reader = r.reader.unwrap();
        assert_eq!(reader.x + reader.width, 120);
    }

    #[test]
    fn the_text_area_is_inside_the_native_border() {
        let column = rect(66, 1, 74, 28);
        assert_eq!(text_width(column, Look::Native), 72);
        assert_eq!(text_height(column, Look::Native), 26);
        assert_eq!(text_width(column, Look::Antiope), 74);
        assert_eq!(text_height(column, Look::Antiope), 28);
    }

    #[test]
    fn a_resize_down_and_back_up_recomputes_the_same_columns() {
        // Review Focus 3: the layout is a pure function of (size, focus, look), so 140 -> 70
        // -> 140 with the reader focused ends exactly where it started.
        let wide = rect(0, 0, 140, 30);
        let narrow = rect(0, 0, 70, 30);
        let at = |a: Rect| {
            let class = layout_class(a.width, a.height);
            (class, columns(a, class, Column::Reader, Look::Native))
        };
        let before = at(wide);
        let (class, one) = at(narrow);
        assert_eq!(class, LayoutClass::One);
        assert_eq!(one.reader, Some(rect(0, 1, 70, 28)));
        assert_eq!((one.sources, one.entries), (None, None));
        assert_eq!(at(wide), before);
    }
}
