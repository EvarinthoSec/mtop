use ratatui::style::Color;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    pub background: Color,
    pub panel: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub good: Color,
    pub warn: Color,
    pub bad: Color,
    pub graph: Color,
    pub violet: Color,
}

impl Theme {
    pub fn from_name(name: &str) -> Self {
        match name {
            "amber" => Self::amber(),
            "mono" => Self::mono(),
            _ => Self::neon(),
        }
    }

    fn neon() -> Self {
        Self {
            background: Color::Rgb(4, 6, 8),
            panel: Color::Rgb(5, 7, 9),
            border: Color::Rgb(48, 88, 64),
            text: Color::Rgb(232, 240, 248),
            muted: Color::Rgb(128, 150, 168),
            accent: Color::Rgb(0, 255, 255),
            good: Color::Rgb(64, 224, 128),
            warn: Color::Rgb(255, 210, 64),
            bad: Color::Rgb(255, 80, 96),
            graph: Color::Rgb(96, 160, 255),
            violet: Color::Rgb(224, 64, 255),
        }
    }

    fn amber() -> Self {
        Self {
            background: Color::Rgb(24, 18, 10),
            panel: Color::Rgb(42, 30, 16),
            border: Color::Rgb(112, 74, 28),
            text: Color::Rgb(255, 239, 204),
            muted: Color::Rgb(176, 145, 96),
            accent: Color::Rgb(255, 176, 48),
            good: Color::Rgb(128, 208, 96),
            warn: Color::Rgb(255, 208, 64),
            bad: Color::Rgb(240, 96, 64),
            graph: Color::Rgb(224, 144, 48),
            violet: Color::Rgb(192, 112, 64),
        }
    }

    fn mono() -> Self {
        Self {
            background: Color::Black,
            panel: Color::Rgb(24, 24, 24),
            border: Color::Rgb(96, 96, 96),
            text: Color::White,
            muted: Color::Rgb(160, 160, 160),
            accent: Color::Gray,
            good: Color::White,
            warn: Color::Gray,
            bad: Color::DarkGray,
            graph: Color::Gray,
            violet: Color::Gray,
        }
    }
}
