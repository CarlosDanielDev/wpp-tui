//! Colour theme. Defaults to the retro phosphor-green CRT palette; an `amber`
//! preset shifts the primary text to amber. Never blue/cyan — project rule.

use ratatui::style::Color;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub primary: Color,
    pub dim: Color,
    pub accent: Color,
    pub accent_dk: Color,
    pub panel_bg: Color,
}

impl Theme {
    pub fn retro() -> Self {
        Self {
            bg: Color::Rgb(0, 0, 0),
            primary: Color::Rgb(0, 255, 65),
            dim: Color::Rgb(0, 180, 45),
            accent: Color::Rgb(255, 175, 0),
            accent_dk: Color::Rgb(255, 140, 0),
            panel_bg: Color::Rgb(0, 40, 10),
        }
    }

    pub fn amber() -> Self {
        Self {
            bg: Color::Rgb(0, 0, 0),
            primary: Color::Rgb(255, 175, 0),
            dim: Color::Rgb(180, 120, 0),
            accent: Color::Rgb(0, 255, 65),
            accent_dk: Color::Rgb(0, 180, 45),
            panel_bg: Color::Rgb(40, 25, 0),
        }
    }

    /// Resolve from `WPP_THEME` (`retro` default, `amber` alternative).
    pub fn from_env() -> Self {
        match std::env::var("WPP_THEME").as_deref() {
            Ok("amber") => Self::amber(),
            _ => Self::retro(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retro_primary_is_phosphor_green() {
        assert_eq!(Theme::retro().primary, Color::Rgb(0, 255, 65));
    }

    #[test]
    fn amber_primary_is_amber_never_blue() {
        let t = Theme::amber();
        assert_eq!(t.primary, Color::Rgb(255, 175, 0));
        // Sanity: no channel config produces a blue-dominant primary.
        if let Color::Rgb(r, g, b) = t.primary {
            assert!(b < r && b < g, "primary must not be blue-dominant");
        }
    }
}
