use iced::{Background, Color, button, container, scrollable::{self, Scroller}};

pub struct Theme;
pub struct LevelButton { pub selected: bool, pub loaded: bool }

impl button::StyleSheet for LevelButton {
    fn active(&self) -> button::Style {
        match self {
            Self { selected: false, loaded: false } => {
                button::Style {
                    background: Some(Background::Color([0., 0., 0.].into())),
                    text_color: Color::from_rgb(1., 1., 1.),
                    border_color: Color::from_rgb(0., 0., 0.),
                    .. Default::default()
                }
            }
            Self { selected: true, loaded: false } => {
                button::Style {
                    background: Some(Background::Color(Color::from_rgb(1., 1., 0.).into())),
                    text_color: Color::from_rgb(0., 0., 0.),
                    border_color: Color::from_rgb(0., 0., 0.),
                    .. Default::default()
                }

            }
            Self { selected: true, loaded: true } => {
                button::Style {
                    background: Some(Background::Color(Color::from_rgb(0.2, 0.2, 0.).into())),
                    text_color: Color::from_rgb(1., 1., 1.),
                    border_width: 2.,
                    border_color: Color::from_rgb(1., 1., 0.),
                    .. Default::default()
                }
            }
            Self { selected: false, loaded: true } => {
                button::Style {
                    background: Some(Background::Color([0.2, 0.2, 0.].into())),
                    text_color: Color::from_rgb(1., 1., 1.),
                    border_color: Color::from_rgb(0., 0., 0.),
                    .. Default::default()
                }
            }
        }
    }

    fn hovered(&self) -> button::Style {
        let active = self.active();
        button::Style {
            border_color: Color {
                r: active.border_color.r + 0.2,
                g: active.border_color.g + 0.2,
                b: active.border_color.b,
                a: active.border_color.a
            },
            border_width: 2.,
            .. active
        }
    }

    fn pressed(&self) -> button::Style {
        let active = self.active();
        button::Style {
            background: active.background.map(|background| match background {
                Background::Color(back) => Background::Color(Color {
                    r: back.r - 0.2,
                    g: back.g - 0.2,
                    b: back.b - 0.2,
                    a: back.a
                })
            }),
            border_width: 2.,
            border_color: Color {
                r: active.border_color.r + 0.4,
                g: active.border_color.g + 0.4,
                b: active.border_color.b,
                a: active.border_color.a
            },
            .. active
        }
    }
}

impl button::StyleSheet for Theme {
    fn active(&self) -> button::Style {
        button::Style {
            background: Some(Background::Color([1., 0., 0.].into())),
            text_color: Color::from_rgb(0., 1., 0.),
            .. Default::default()
        }
    }
}

impl container::StyleSheet for Theme {
    fn style(&self) -> container::Style {
        container::Style {
            text_color: Some(Color::from_rgb(0.0, 0.0, 0.0)),
            .. Default::default()
        }
    }
}

impl scrollable::StyleSheet for Theme {
    fn active(&self) -> scrollable::Scrollbar {
        scrollable::Scrollbar {
            background: None,
            border_radius: 5.0,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            scroller: Scroller {
                color: [1.0, 1.0, 1.0, 0.4].into(),
                border_radius: 5.0,
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
            },
        }
    }

    fn hovered(&self) -> scrollable::Scrollbar {
        scrollable::Scrollbar {
            .. self.active()
        }
    }
}