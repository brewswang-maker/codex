//! Hand-drawn line icons rendered on the iced canvas.
//!
//! Every glyph is drawn on a 24x24 design grid with 2px round-capped
//! strokes, scaled to the requested pixel size; no font glyphs or external
//! assets are involved.

use crate::message::Message;
use iced::Point;
use iced::Rectangle;
use iced::Size;
use iced::widget::canvas;
use iced::widget::canvas::Geometry;
use iced::widget::canvas::Path;
use iced::widget::canvas::Stroke;
use iced::widget::canvas::stroke::LineCap;

/// The glyph set the shell needs. Some variants land with later
/// milestones (composer send, task rows, command palette, git status).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    /// Conversation / task list.
    Chat,
    /// Folder / file explorer.
    Folder,
    /// Sliders-style settings.
    Gear,
    /// Plus cross.
    Plus,
    /// History clock.
    Clock,
    /// Search lens.
    Search,
    /// Upward send arrow.
    SendUp,
    /// Filled square for the interrupt action.
    Stop,
    /// Git branch.
    GitBranch,
    /// Copy (two stacked rectangles).
    Copy,
    /// Thumbs-up feedback.
    ThumbUp,
    /// Thumbs-down feedback.
    ThumbDown,
    /// Paperclip for the composer attach action.
    Paperclip,
    /// Source-control rail entry (branch with change dot).
    SourceControl,
    /// Repo Wiki rail entry (closed book).
    Wiki,
    /// Run-and-debug rail entry (bug).
    Debug,
    /// Remote-explorer rail entry (monitor).
    Remote,
    /// Extensions rail entry (grid of squares).
    Extensions,
    /// Skills rail entry (a large sparkle with a small companion).
    Sparkle,
}

/// A single drawn icon; drop it into a `canvas` widget.
pub struct Icon {
    kind: IconKind,
    color: iced::Color,
    size: f32,
}

impl Icon {
    pub fn new(kind: IconKind, color: iced::Color, size: f32) -> Self {
        Self { kind, color, size }
    }

    /// The icon as a sized canvas widget.
    pub fn widget(self) -> canvas::Canvas<Icon, Message> {
        let size = self.size;
        canvas(self).width(size).height(size)
    }
}

impl<Message> canvas::Program<Message> for Icon {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        _bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry<iced::Renderer>> {
        let mut frame = canvas::Frame::new(renderer, Size::new(self.size, self.size));
        let k = self.size / 24.0;
        let mut stroke = Stroke::default().with_color(self.color).with_width(2.0 * k);
        stroke.line_cap = LineCap::Round;

        match self.kind {
            IconKind::Chat => {
                frame.stroke(
                    &Path::rounded_rectangle(
                        point(4.0, 5.0, k),
                        Size::new(16.0 * k, 12.0 * k),
                        (3.0 * k).into(),
                    ),
                    stroke,
                );
                frame.stroke(
                    &polyline(&[(8.0, 17.5), (8.0, 20.5), (11.5, 17.5)], k),
                    stroke,
                );
            }
            IconKind::Folder => {
                frame.stroke(
                    &Path::new(|p| {
                        p.move_to(point(3.0, 18.0, k));
                        p.line_to(point(3.0, 6.0, k));
                        p.line_to(point(9.0, 6.0, k));
                        p.line_to(point(11.0, 8.5, k));
                        p.line_to(point(21.0, 8.5, k));
                        p.line_to(point(21.0, 18.0, k));
                        p.close();
                    }),
                    stroke,
                );
            }
            IconKind::Gear => {
                for (y, knob) in [(7.0, 15.0), (12.0, 8.0), (17.0, 13.0)] {
                    frame.stroke(&line(3.0, y, 21.0, y, k), stroke);
                    frame.stroke(&Path::circle(point(knob, y, k), 2.2 * k), stroke);
                }
            }
            IconKind::Plus => {
                frame.stroke(&line(12.0, 6.0, 12.0, 18.0, k), stroke);
                frame.stroke(&line(6.0, 12.0, 18.0, 12.0, k), stroke);
            }
            IconKind::Clock => {
                frame.stroke(&Path::circle(point(12.0, 12.0, k), 8.0 * k), stroke);
                frame.stroke(
                    &polyline(&[(12.0, 7.5), (12.0, 12.0), (15.5, 13.5)], k),
                    stroke,
                );
            }
            IconKind::Search => {
                frame.stroke(&Path::circle(point(11.0, 11.0, k), 6.0 * k), stroke);
                frame.stroke(&line(15.5, 15.5, 20.0, 20.0, k), stroke);
            }
            IconKind::SendUp => {
                frame.stroke(
                    &polyline(&[(6.5, 11.5), (12.0, 6.0), (17.5, 11.5)], k),
                    stroke,
                );
                frame.stroke(&line(12.0, 6.0, 12.0, 19.0, k), stroke);
            }
            IconKind::Stop => {
                frame.fill(
                    &Path::rounded_rectangle(
                        point(6.5, 6.5, k),
                        Size::new(11.0 * k, 11.0 * k),
                        (2.0 * k).into(),
                    ),
                    self.color,
                );
            }
            IconKind::GitBranch => {
                frame.stroke(&Path::circle(point(7.0, 6.0, k), 2.2 * k), stroke);
                frame.stroke(&Path::circle(point(7.0, 18.0, k), 2.2 * k), stroke);
                frame.stroke(&Path::circle(point(17.0, 6.0, k), 2.2 * k), stroke);
                frame.stroke(
                    &Path::new(|p| {
                        p.move_to(point(7.0, 8.2, k));
                        p.line_to(point(7.0, 15.8, k));
                        p.move_to(point(17.0, 8.2, k));
                        p.line_to(point(17.0, 11.0, k));
                        p.quadratic_curve_to(point(17.0, 13.5, k), point(13.0, 13.5, k));
                        p.line_to(point(9.2, 13.5, k));
                    }),
                    stroke,
                );
            }
            IconKind::SourceControl => {
                // Branch line with a change dot at the tip.
                frame.stroke(&Path::circle(point(7.0, 6.0, k), 2.2 * k), stroke);
                frame.stroke(&Path::circle(point(7.0, 18.0, k), 2.2 * k), stroke);
                frame.stroke(&Path::circle(point(17.0, 16.0, k), 2.6 * k), stroke);
                frame.stroke(&line(7.0, 8.2, 7.0, 15.8, k), stroke);
                frame.stroke(
                    &Path::new(|p| {
                        p.move_to(point(9.2, 6.0, k));
                        p.line_to(point(13.0, 6.0, k));
                        p.quadratic_curve_to(point(17.0, 8.0, k), point(17.0, 13.4, k));
                    }),
                    stroke,
                );
            }
            IconKind::Wiki => {
                // Closed book: cover, spine, and two text lines.
                frame.stroke(
                    &Path::rounded_rectangle(
                        point(4.0, 4.0, k),
                        Size::new(16.0 * k, 16.0 * k),
                        (1.5 * k).into(),
                    ),
                    stroke,
                );
                frame.stroke(&line(8.5, 4.5, 8.5, 19.5, k), stroke);
                frame.stroke(&line(11.5, 9.0, 16.5, 9.0, k), stroke);
                frame.stroke(&line(11.5, 13.0, 16.5, 13.0, k), stroke);
            }
            IconKind::Debug => {
                // Bug: body, antennae, and three leg pairs.
                frame.stroke(
                    &Path::rounded_rectangle(
                        point(8.0, 9.0, k),
                        Size::new(8.0 * k, 10.0 * k),
                        (4.0 * k).into(),
                    ),
                    stroke,
                );
                frame.stroke(&line(10.0, 9.5, 8.5, 6.0, k), stroke);
                frame.stroke(&line(14.0, 9.5, 15.5, 6.0, k), stroke);
                frame.stroke(&line(8.0, 12.0, 4.5, 11.0, k), stroke);
                frame.stroke(&line(16.0, 12.0, 19.5, 11.0, k), stroke);
                frame.stroke(&line(8.0, 16.0, 4.5, 17.0, k), stroke);
                frame.stroke(&line(16.0, 16.0, 19.5, 17.0, k), stroke);
            }
            IconKind::Remote => {
                // Monitor on a stand.
                frame.stroke(
                    &Path::rounded_rectangle(
                        point(4.0, 5.0, k),
                        Size::new(16.0 * k, 11.0 * k),
                        (2.0 * k).into(),
                    ),
                    stroke,
                );
                frame.stroke(&line(12.0, 16.0, 12.0, 19.5, k), stroke);
                frame.stroke(&line(8.5, 19.5, 15.5, 19.5, k), stroke);
            }
            IconKind::Extensions => {
                // Four squares, VS Code plugin-grid style.
                for (x, y) in [(4.0, 4.0), (13.0, 4.0), (4.0, 13.0), (13.0, 13.0)] {
                    frame.stroke(
                        &Path::rounded_rectangle(
                            point(x, y, k),
                            Size::new(7.0 * k, 7.0 * k),
                            (1.5 * k).into(),
                        ),
                        stroke,
                    );
                }
            }
            IconKind::Sparkle => {
                // A four-point sparkle plus a small companion star.
                frame.stroke(
                    &Path::new(|p| {
                        p.move_to(point(10.5, 7.0, k));
                        p.quadratic_curve_to(point(11.2, 12.8, k), point(17.0, 13.5, k));
                        p.quadratic_curve_to(point(11.2, 14.2, k), point(10.5, 20.0, k));
                        p.quadratic_curve_to(point(9.8, 14.2, k), point(4.0, 13.5, k));
                        p.quadratic_curve_to(point(9.8, 12.8, k), point(10.5, 7.0, k));
                        p.close();
                        p.move_to(point(18.5, 2.5, k));
                        p.quadratic_curve_to(point(18.8, 5.2, k), point(21.5, 5.5, k));
                        p.quadratic_curve_to(point(18.8, 5.8, k), point(18.5, 8.5, k));
                        p.quadratic_curve_to(point(18.2, 5.8, k), point(15.5, 5.5, k));
                        p.quadratic_curve_to(point(18.2, 5.2, k), point(18.5, 2.5, k));
                        p.close();
                    }),
                    stroke,
                );
            }
            IconKind::Copy => {
                frame.stroke(
                    &Path::rounded_rectangle(
                        point(9.0, 9.0, k),
                        Size::new(11.0 * k, 11.0 * k),
                        (2.0 * k).into(),
                    ),
                    stroke,
                );
                frame.stroke(
                    &Path::new(|p| {
                        p.move_to(point(15.0, 5.5, k));
                        p.line_to(point(6.0, 5.5, k));
                        p.line_to(point(6.0, 15.0, k));
                    }),
                    stroke,
                );
            }
            IconKind::Paperclip => {
                // Classic paperclip: one long leg, a bottom U-turn, and the
                // inner leg folding back with a small hook.
                frame.stroke(
                    &Path::new(|p| {
                        p.move_to(point(20.5, 11.0, k));
                        p.line_to(point(12.3, 19.2, k));
                        p.quadratic_curve_to(point(6.0, 20.5, k), point(5.2, 14.5, k));
                        p.line_to(point(13.5, 6.2, k));
                        p.quadratic_curve_to(point(14.4, 2.8, k), point(17.0, 5.4, k));
                        p.line_to(point(9.0, 13.4, k));
                        p.quadratic_curve_to(point(8.4, 15.4, k), point(10.2, 15.6, k));
                        p.line_to(point(16.8, 9.0, k));
                    }),
                    stroke,
                );
            }
            IconKind::ThumbUp | IconKind::ThumbDown => {
                let up = self.kind == IconKind::ThumbUp;
                // One path, flipped vertically for the down variant: the
                // hand outline (palm block plus the raised thumb) on the
                // 24-grid, y-flipped around the center when drawing down.
                let y = |v: f32| if up { v } else { 24.0 - v };
                frame.stroke(
                    &Path::new(|p| {
                        p.move_to(point(7.0, y(11.0), k));
                        p.line_to(point(7.0, y(19.5), k));
                        p.move_to(point(10.0, y(10.5), k));
                        p.line_to(point(13.5, y(3.5), k));
                        p.quadratic_curve_to(point(15.5, y(4.5), k), point(14.5, y(7.0), k));
                        p.line_to(point(14.0, y(10.5), k));
                        p.line_to(point(18.5, y(10.5), k));
                        p.quadratic_curve_to(point(20.5, y(11.0), k), point(20.0, y(13.0), k));
                        p.line_to(point(18.5, y(18.5), k));
                        p.quadratic_curve_to(point(18.0, y(19.5), k), point(16.5, y(19.5), k));
                        p.line_to(point(10.0, y(19.5), k));
                        p.close();
                    }),
                    stroke,
                );
                frame.stroke(&line(7.0, y(11.0), 3.5, y(11.0), k), stroke);
                frame.stroke(&line(3.5, y(11.0), 3.5, y(19.5), k), stroke);
                frame.stroke(&line(3.5, y(19.5), 7.0, y(19.5), k), stroke);
            }
        }

        vec![frame.into_geometry()]
    }
}

/// A point on the 24-grid scaled to pixels.
fn point(x: f32, y: f32, k: f32) -> Point {
    Point::new(x * k, y * k)
}

/// A straight segment between two grid points.
fn line(x1: f32, y1: f32, x2: f32, y2: f32, k: f32) -> Path {
    polyline(&[(x1, y1), (x2, y2)], k)
}

/// A connected run of grid points.
fn polyline(points: &[(f32, f32)], k: f32) -> Path {
    Path::new(|p| {
        let (first_x, first_y) = points[0];
        p.move_to(point(first_x, first_y, k));
        for (x, y) in &points[1..] {
            p.line_to(point(*x, *y, k));
        }
    })
}
