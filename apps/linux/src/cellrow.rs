//! `CellRow`: a painted item drawing one teletext grid row (or several, for a two-line entry)
//! from the runs `ressac_ui::teletext` encodes. Backgrounds go first, then the text, clipped to
//! the row, in Bedstead at an integer pixel size without antialiasing, so every glyph lands on
//! whole pixels.

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{
    ClipOperation, QColor, QFont, QFontHintingPreference, QFontStyleStrategy, QPen, QPoint, QRect,
    QRectF, QString,
};
use ressac_ui::teletext::{self, Color, PaintRun};

use crate::shim;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtQuick/QQuickPaintedItem>);
        type QQuickPaintedItem;
        include!("cxx-qt-lib/qpainter.h");
        type QPainter = cxx_qt_lib::QPainter;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickPaintedItem]
        #[qproperty(QString, line, READ, WRITE = set_line, NOTIFY)]
        #[qproperty(f64, size, READ, WRITE = set_size, NOTIFY)]
        #[qproperty(bool, double_height, READ, WRITE = set_double_height, NOTIFY)]
        #[qproperty(bool, reversed, READ, WRITE = set_reversed, NOTIFY)]
        #[qproperty(i32, reverse_foreground, READ, WRITE = set_reverse_foreground, NOTIFY)]
        #[qproperty(i32, reverse_background, READ, WRITE = set_reverse_background, NOTIFY)]
        type CellRow = super::CellRowRust;

        fn set_line(self: Pin<&mut CellRow>, line: QString);
        fn set_size(self: Pin<&mut CellRow>, size: f64);
        fn set_double_height(self: Pin<&mut CellRow>, on: bool);
        fn set_reversed(self: Pin<&mut CellRow>, on: bool);
        fn set_reverse_foreground(self: Pin<&mut CellRow>, index: i32);
        fn set_reverse_background(self: Pin<&mut CellRow>, index: i32);

        #[cxx_override]
        unsafe fn paint(self: Pin<&mut CellRow>, painter: *mut QPainter);
    }

    unsafe extern "RustQt" {
        #[inherit]
        fn update(self: Pin<&mut CellRow>);
        #[inherit]
        fn width(self: &CellRow) -> f64;
        #[inherit]
        #[cxx_name = "setImplicitWidth"]
        fn set_implicit_width(self: Pin<&mut CellRow>, width: f64);
        #[inherit]
        #[cxx_name = "setImplicitHeight"]
        fn set_implicit_height(self: Pin<&mut CellRow>, height: f64);
        #[inherit]
        #[cxx_name = "setAntialiasing"]
        fn set_antialiasing(self: Pin<&mut CellRow>, on: bool);
        #[inherit]
        #[cxx_name = "setFillColor"]
        fn set_fill_color(self: Pin<&mut CellRow>, color: &QColor);
        #[inherit]
        #[cxx_name = "setOpaquePainting"]
        fn set_opaque_painting(self: Pin<&mut CellRow>, on: bool);
    }

    // BaseArguments = (): QQuickPaintedItem's own default constructor.
    impl cxx_qt::Constructor<()> for CellRow {}
}

/// The painted row's state. `reverse_foreground` and `reverse_background` are `Color::index`
/// values, used when `reversed`.
pub struct CellRowRust {
    line: QString,
    size: f64,
    double_height: bool,
    reversed: bool,
    reverse_foreground: i32,
    reverse_background: i32,
    /// `line` decoded on `set_line`.
    rows: Vec<Vec<PaintRun>>,
}

impl Default for CellRowRust {
    fn default() -> Self {
        CellRowRust {
            line: QString::default(),
            size: 20.0,
            double_height: false,
            reversed: false,
            reverse_foreground: i32::from(Color::Black.index()),
            reverse_background: i32::from(Color::White.index()),
            rows: Vec::new(),
        }
    }
}

impl cxx_qt::Constructor<()> for qobject::CellRow {
    type BaseArguments = ();
    type InitializeArguments = ();
    type NewArguments = ();

    fn route_arguments(_: ()) -> ((), (), ()) {
        ((), (), ())
    }

    fn new(_: ()) -> CellRowRust {
        CellRowRust::default()
    }

    fn initialize(mut self: Pin<&mut Self>, _: ()) {
        self.as_mut().set_antialiasing(false);
        self.as_mut().set_opaque_painting(true);
        self.as_mut().set_fill_color(&QColor::from_rgb(0, 0, 0));
    }
}

fn color(color: Color) -> QColor {
    let [r, g, b] = color.rgb();
    QColor::from_rgb(i32::from(r), i32::from(g), i32::from(b))
}

fn indexed(index: i32) -> Color {
    u8::try_from(index)
        .ok()
        .and_then(Color::from_index)
        .unwrap_or(Color::White)
}

/// Cells of the widest decoded row.
fn widest(rows: &[Vec<PaintRun>]) -> usize {
    rows.iter()
        .map(|row| row.iter().map(|run| run.text.chars().count()).sum())
        .max()
        .unwrap_or(0)
}

impl qobject::CellRow {
    /// Implicit size from the cells of the widest row and the number of rows.
    fn resize(mut self: Pin<&mut Self>) {
        let (rows, size, double) = {
            let rust = self.rust();
            (rust.rows.len(), rust.size, rust.double_height)
        };
        let cells = widest(&self.rust().rows);
        let (cell_width, cell_height) = teletext::cell_size(size);
        let factor = if double { 2.0 } else { 1.0 };
        self.as_mut().set_implicit_width(cells as f64 * cell_width);
        self.as_mut()
            .set_implicit_height(rows as f64 * cell_height * factor);
        self.update();
    }

    pub fn set_line(mut self: Pin<&mut Self>, line: QString) {
        if self.rust().line == line {
            return;
        }
        self.as_mut().rust_mut().get_mut().rows = teletext::decode_lines(&line.to_string());
        self.as_mut().rust_mut().get_mut().line = line;
        self.as_mut().line_changed();
        self.resize();
    }

    pub fn set_size(mut self: Pin<&mut Self>, size: f64) {
        if self.rust().size == size {
            return;
        }
        self.as_mut().rust_mut().get_mut().size = size;
        self.as_mut().size_changed();
        self.resize();
    }

    pub fn set_double_height(mut self: Pin<&mut Self>, on: bool) {
        if self.rust().double_height == on {
            return;
        }
        self.as_mut().rust_mut().get_mut().double_height = on;
        self.as_mut().double_height_changed();
        self.resize();
    }

    pub fn set_reversed(mut self: Pin<&mut Self>, on: bool) {
        if self.rust().reversed == on {
            return;
        }
        self.as_mut().rust_mut().get_mut().reversed = on;
        self.as_mut().reversed_changed();
        self.resize();
    }

    pub fn set_reverse_foreground(mut self: Pin<&mut Self>, index: i32) {
        if self.rust().reverse_foreground == index {
            return;
        }
        self.as_mut().rust_mut().get_mut().reverse_foreground = index;
        self.as_mut().reverse_foreground_changed();
        self.resize();
    }

    pub fn set_reverse_background(mut self: Pin<&mut Self>, index: i32) {
        if self.rust().reverse_background == index {
            return;
        }
        self.as_mut().rust_mut().get_mut().reverse_background = index;
        self.as_mut().reverse_background_changed();
        self.resize();
    }

    /// # Safety
    /// `painter` is the live painter Qt passes to `QQuickPaintedItem::paint`.
    pub unsafe fn paint(self: Pin<&mut Self>, painter: *mut qobject::QPainter) {
        // SAFETY: Qt hands `paint` a valid, exclusive painter for the duration of the call.
        let Some(mut painter) =
            (unsafe { painter.as_mut() }).map(|p| unsafe { Pin::new_unchecked(p) })
        else {
            return;
        };
        let rust = self.rust();
        let size = rust.size;
        if size < 1.0 {
            return;
        }
        let width = self.width().ceil() as i32;
        let pixel = size as i32;
        let (cell_width, _) = teletext::cell_size(size);

        // Double height: a row is `size` tall in logical units and drawn twice as tall.
        if rust.double_height {
            painter.as_mut().set_window(&QRect::new(0, 0, width, pixel));
            painter
                .as_mut()
                .set_viewport(&QRect::new(0, 0, width, 2 * pixel));
        }

        let mut font = QFont::default();
        let family = shim::ressac_bedstead_family();
        font.set_family(&QString::from(if family.is_empty() {
            teletext::FAMILY
        } else {
            family.as_str()
        }));
        font.set_pixel_size(pixel);
        font.set_style_strategy(QFontStyleStrategy::NoAntialias);
        font.set_hinting_preference(QFontHintingPreference::PreferFullHinting);
        font.set_kerning(false);
        painter.as_mut().set_font(&font);

        let reverse_background = color(indexed(rust.reverse_background));
        let reverse_foreground = color(indexed(rust.reverse_foreground));
        let magenta = color(Color::Magenta);

        // Pass 1: every run's background.
        for (r, row) in rust.rows.iter().enumerate() {
            let top = r as f64 * size;
            let mut x = 0.0;
            for run in row {
                let run_width = run.text.chars().count() as f64 * cell_width;
                let background = if rust.reversed {
                    reverse_background.clone()
                } else {
                    color(run.bg)
                };
                painter
                    .as_mut()
                    .fill_rect(&QRectF::new(x, top, run_width, size), &background);
                x += run_width;
            }
        }

        // Pass 2: the text, clipped to its row.
        for (r, row) in rust.rows.iter().enumerate() {
            let top = (r as f64 * size) as i32;
            painter.as_mut().set_clip_rect(
                &QRect::new(0, top, width, pixel),
                ClipOperation::ReplaceClip,
            );
            let mut x = 0.0_f64;
            for run in row {
                let foreground = match run.fg {
                    None => magenta.clone(),
                    Some(_) if rust.reversed => reverse_foreground.clone(),
                    Some(fg) => color(fg),
                };
                painter.as_mut().set_pen(&QPen::from(&foreground));
                painter.as_mut().draw_text(
                    &QPoint::new(x.round() as i32, top + pixel * 8 / 10),
                    &QString::from(run.text.as_str()),
                );
                x += run.text.chars().count() as f64 * cell_width;
            }
        }
    }
}
