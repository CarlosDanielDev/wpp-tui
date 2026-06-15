use qrcode::types::Color as QrColor;
use qrcode::QrCode;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

pub fn render_qr(code: &str, fg: Color, bg: Color) -> Vec<Line<'static>> {
    let qr = match QrCode::new(code) {
        Ok(qr) => qr,
        Err(_) => {
            return vec![Line::from(Span::styled(
                "Invalid QR code data",
                Style::default().fg(fg).bg(bg),
            ))];
        }
    };

    let width = qr.width();
    let colors = qr.into_colors();

    let quiet = 1;
    let mut lines = Vec::with_capacity(width.div_ceil(2) + quiet * 2);

    for _ in 0..quiet {
        lines.push(Line::from(Span::styled(
            " ".repeat(width + quiet * 2),
            Style::default().bg(bg),
        )));
    }

    for y in (0..width).step_by(2) {
        let mut spans = Vec::with_capacity(width + quiet * 2);

        for _ in 0..quiet {
            spans.push(Span::styled(" ", Style::default().bg(bg)));
        }

        for x in 0..width {
            let top_dark = colors[y * width + x] == QrColor::Dark;
            let bottom_dark = if y + 1 < width {
                colors[(y + 1) * width + x] == QrColor::Dark
            } else {
                false
            };

            let ch = match (top_dark, bottom_dark) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            };

            let style = if top_dark || bottom_dark {
                Style::default().fg(fg).bg(bg)
            } else {
                Style::default().bg(bg)
            };

            spans.push(Span::styled(ch.to_string(), style));
        }

        for _ in 0..quiet {
            spans.push(Span::styled(" ", Style::default().bg(bg)));
        }

        lines.push(Line::from(spans));
    }

    for _ in 0..quiet {
        lines.push(Line::from(Span::styled(
            " ".repeat(width + quiet * 2),
            Style::default().bg(bg),
        )));
    }

    lines
}
