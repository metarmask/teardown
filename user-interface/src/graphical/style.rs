use iced::{Background, Color, button, container, scrollable::{self, Scroller}};

pub struct Theme;
pub struct SelectedListButton;
pub struct ListButton;

impl button::StyleSheet for SelectedListButton {
    fn active(&self) -> button::Style {
        button::Style {
            background: Some(Background::Color([1., 1., 0.].into())),
            text_color: Color::from_rgb(0., 0., 0.),
            .. Default::default()
        }
    }
}

impl button::StyleSheet for ListButton {
    fn active(&self) -> button::Style {
        button::Style {
            background: Some(Background::Color([0., 0., 0.].into())),
            text_color: Color::from_rgb(1., 1., 1.),
            .. Default::default()
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
            background: Some(Background::Color(Color::from_rgb(1., 1., 1.))),
            border_color: Default::default(),
            border_radius: Default::default(),
            border_width: Default::default(),
            scroller: Scroller {
                border_color: Default::default(),
                border_radius: Default::default(),
                border_width: Default::default(),
                color: Default::default(),
            }
        }
    }

    fn hovered(&self) -> scrollable::Scrollbar {
        scrollable::Scrollbar {
            background: Some(Background::Color(Color::from_rgb(1., 0.5, 1.))),
            border_color: Default::default(),
            border_radius: Default::default(),
            border_width: Default::default(),
            scroller: Scroller {
                border_color: Default::default(),
                border_radius: Default::default(),
                border_width: Default::default(),
                color: Default::default(),
            }
        }
    }
}