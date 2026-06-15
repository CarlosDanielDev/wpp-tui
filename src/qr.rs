use qrcode::types::Color as QrColor;
use qrcode::QrCode;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

// A QR must be dark modules on a light field with a 4-module quiet zone to be
// reliably scannable — phone scanners reject inverted (light-on-dark) codes.
// So the QR always renders black-on-white regardless of the app's theme.
const QR_DARK: Color = Color::Rgb(0, 0, 0);
const QR_LIGHT: Color = Color::Rgb(255, 255, 255);
const QUIET: usize = 4;

/// Render `code` as a scannable QR using half-block glyphs (two module rows per
/// text line, so the result is roughly square in a terminal cell grid).
pub fn render_qr(code: &str) -> Vec<Line<'static>> {
    let qr = match QrCode::new(code) {
        Ok(qr) => qr,
        Err(_) => {
            return vec![Line::from(Span::styled(
                "Invalid QR code data",
                Style::default().fg(QR_DARK).bg(QR_LIGHT),
            ))];
        }
    };

    let width = qr.width();
    let colors = qr.into_colors();
    let light = Style::default().fg(QR_DARK).bg(QR_LIGHT);

    // A blank (all-light) line spanning the code plus the left/right quiet zone.
    let blank = || {
        Line::from(Span::styled(
            " ".repeat(width + QUIET * 2),
            Style::default().bg(QR_LIGHT),
        ))
    };

    let mut lines = Vec::with_capacity(width.div_ceil(2) + QUIET * 2);

    // Top quiet zone.
    for _ in 0..QUIET {
        lines.push(blank());
    }

    for y in (0..width).step_by(2) {
        let mut spans = Vec::with_capacity(width + QUIET * 2);

        // Left quiet zone.
        spans.push(Span::styled(
            " ".repeat(QUIET),
            Style::default().bg(QR_LIGHT),
        ));

        for x in 0..width {
            let top_dark = colors[y * width + x] == QrColor::Dark;
            let bottom_dark = y + 1 < width && colors[(y + 1) * width + x] == QrColor::Dark;

            // fg paints the dark half, bg the light half — so every cell uses the
            // same black-on-white style and only the glyph changes.
            let ch = match (top_dark, bottom_dark) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            };
            spans.push(Span::styled(ch.to_string(), light));
        }

        // Right quiet zone.
        spans.push(Span::styled(
            " ".repeat(QUIET),
            Style::default().bg(QR_LIGHT),
        ));

        lines.push(Line::from(spans));
    }

    // Bottom quiet zone.
    for _ in 0..QUIET {
        lines.push(blank());
    }

    lines
}
