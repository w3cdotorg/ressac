//! The article body both looks draw: each article line is one block, an image's placeholder line
//! grows into the rows of the picture (at most half the area), so scrolling is in screen rows.
//! The looks differ only in how a line is painted and in the image variant.

use std::collections::HashMap;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui_image::StatefulImage;
use ratatui_image::protocol::StatefulProtocol;

use super::html::{ImageRef, RenderedArticle};
use super::images::{ImageState, ImageStore, Variant};

/// What one article line becomes on screen.
enum Piece<'a> {
    /// The line as `render_html` made it (also `[image: alt text]` for a failed image or with
    /// images off).
    Text(&'a Line<'static>),
    /// An image that is on its way: one placeholder row.
    Loading,
    /// A decoded image, drawn in the rows reserved for it.
    Image { url: &'a str },
}

/// How a look paints the body.
pub struct BodyLook<'a> {
    pub variant: Variant,
    /// Width of the text and placeholder rows.
    pub text_width: u16,
    /// Width of a picture.
    pub image_width: u16,
    /// The look's version of an article line.
    pub paint_line: &'a dyn Fn(&Line<'static>) -> Line<'static>,
    /// The row shown while an image loads.
    pub loading: Line<'static>,
    /// Called with the area of every picture just drawn.
    pub after_image: &'a dyn Fn(&mut Frame, Rect),
}

/// What `draw_body` measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyDrawn {
    /// The article's height in rows.
    pub total: usize,
    /// The first row drawn: the requested scroll, unless it fell inside a picture.
    pub scroll: usize,
}

/// A scroll inside a picture shows none of it (a picture cannot be cut) and leaves a blank band:
/// it moves to the picture's first row, or past its last one on the last page, where going back
/// would hide the article's end.
fn off_the_pictures(
    scroll: usize,
    blocks: &[(usize, usize)],
    total: usize,
    viewport: usize,
) -> usize {
    let Some(&(top, rows)) = blocks
        .iter()
        .find(|&&(top, rows)| rows > 1 && scroll > top && scroll < top + rows)
    else {
        return scroll;
    };
    if scroll >= total.saturating_sub(viewport) {
        top + rows
    } else {
        top
    }
}

/// Draws `article` in `area` from line `scroll`, fetching what is near the viewport. Returns the
/// article's height in rows and the scroll actually drawn.
pub fn draw_body(
    frame: &mut Frame,
    area: Rect,
    article: &RenderedArticle,
    show_images: bool,
    scroll: usize,
    images: &mut ImageStore,
    look: &BodyLook<'_>,
) -> BodyDrawn {
    let viewport = usize::from(area.height);
    let max_image_rows = (area.height / 2).max(1);
    let show_images = show_images && images.enabled();
    let at_line: HashMap<usize, &ImageRef> = article.images.iter().map(|i| (i.line, i)).collect();
    // First the blocks and their rows, then the drawing from the scroll they allow.
    let mut blocks: Vec<(Piece<'_>, usize, usize)> = Vec::with_capacity(article.lines.len());
    let mut top = 0usize;
    for (index, line) in article.lines.iter().enumerate() {
        let (piece, rows) = match at_line.get(&index) {
            Some(image) if show_images => match images.state_of(&image.url) {
                ImageState::Ready => {
                    let rows =
                        images.rows_for(&image.url, look.variant, look.image_width, max_image_rows);
                    (Piece::Image { url: &image.url }, usize::from(rows))
                }
                ImageState::Failed => (Piece::Text(line), 1),
                ImageState::Absent | ImageState::Loading => {
                    // Lazy loading: from one viewport above to two below the visible rows.
                    if top + viewport >= scroll && top <= scroll + 2 * viewport {
                        images.want(&image.url);
                    }
                    (Piece::Loading, 1)
                }
            },
            _ => (Piece::Text(line), 1),
        };
        blocks.push((piece, top, rows));
        top += rows;
    }
    let spans: Vec<(usize, usize)> = blocks.iter().map(|&(_, top, rows)| (top, rows)).collect();
    let scroll = off_the_pictures(scroll, &spans, top, viewport);
    for (piece, top, rows) in blocks {
        // A picture cannot be cut: only blocks wholly inside the viewport are drawn.
        if top >= scroll && top + rows <= scroll + viewport {
            let place = Rect {
                x: area.x,
                y: area.y + u16::try_from(top - scroll).unwrap_or(u16::MAX),
                width: look.text_width,
                height: u16::try_from(rows).unwrap_or(u16::MAX),
            };
            match piece {
                Piece::Text(line) => {
                    frame.render_widget(Paragraph::new((look.paint_line)(line)), place);
                }
                Piece::Loading => {
                    frame.render_widget(Paragraph::new(look.loading.clone()), place);
                }
                Piece::Image { url } => {
                    let place = Rect {
                        width: look.image_width,
                        ..place
                    };
                    if let Some(protocol) = images.protocol(url, look.variant) {
                        frame.render_stateful_widget(
                            StatefulImage::<StatefulProtocol>::default(),
                            place,
                            protocol,
                        );
                        (look.after_image)(frame, place);
                    }
                }
            }
        }
    }
    BodyDrawn { total: top, scroll }
}
