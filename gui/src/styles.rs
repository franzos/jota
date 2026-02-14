use iced::font::Weight;
use iced::widget::{button, container, Space};
use iced::{Background, Border, Color, Element, Fill, Font, Shadow, Vector};

use crate::{ACTIVE, BORDER, MUTED, PRIMARY, SURFACE};

// -- Additional palette --

pub const ACCENT: Color = Color::from_rgb(0.059, 0.757, 0.718);
pub const DANGER: Color = Color::from_rgb(0.906, 0.192, 0.192);
pub const WARNING: Color = Color::from_rgb(1.0, 0.757, 0.027);

// -- Fonts --

pub const BOLD: Font = Font {
    weight: Weight::Bold,
    ..Font::DEFAULT
};

// -- Container styles --

pub fn card(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.15),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..Default::default()
    }
}

pub fn card_flat(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    }
}

pub fn pill(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ACTIVE)),
        border: Border {
            radius: 20.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

// -- Button styles --

pub fn btn_primary(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        text_color: Color::WHITE,
        border: Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    };

    match status {
        button::Status::Active => button::Style {
            background: Some(Background::Color(PRIMARY)),
            shadow: Shadow {
                color: Color::from_rgba(0.145, 0.349, 0.961, 0.25),
                offset: Vector::new(0.0, 2.0),
                blur_radius: 6.0,
            },
            ..base
        },
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.19, 0.40, 1.0))),
            shadow: Shadow {
                color: Color::from_rgba(0.145, 0.349, 0.961, 0.4),
                offset: Vector::new(0.0, 3.0),
                blur_radius: 10.0,
            },
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.11, 0.30, 0.88))),
            ..base
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.15, 0.19, 0.25))),
            text_color: Color::from_rgba(1.0, 1.0, 1.0, 0.35),
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        },
    }
}

pub fn btn_secondary(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let base_border = Border {
        color: BORDER,
        width: 1.0,
        radius: 8.0.into(),
    };

    match status {
        button::Status::Active => button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: Color::from_rgb(0.85, 0.87, 0.90),
            border: base_border,
            ..Default::default()
        },
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(ACTIVE)),
            text_color: Color::WHITE,
            border: base_border,
            ..Default::default()
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(SURFACE)),
            text_color: Color::WHITE,
            border: base_border,
            ..Default::default()
        },
        button::Status::Disabled => button::Style {
            text_color: Color::from_rgba(1.0, 1.0, 1.0, 0.3),
            border: Border {
                color: Color::from_rgba(0.204, 0.259, 0.337, 0.5),
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        },
    }
}

pub fn btn_danger(_theme: &iced::Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Active => button::Style {
            background: Some(Background::Color(Color::from_rgba(
                0.906, 0.192, 0.192, 0.12,
            ))),
            text_color: DANGER,
            border: Border {
                color: Color::from_rgba(0.906, 0.192, 0.192, 0.25),
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        },
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(DANGER)),
            text_color: Color::WHITE,
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.75, 0.15, 0.15))),
            text_color: Color::WHITE,
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.15, 0.19, 0.25))),
            text_color: Color::from_rgba(1.0, 1.0, 1.0, 0.35),
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        },
    }
}

pub fn btn_ghost(_theme: &iced::Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Active => button::Style {
            background: None,
            text_color: Color::from_rgb(0.85, 0.87, 0.90),
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        },
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.05))),
            text_color: Color::WHITE,
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        },
        _ => button::Style {
            text_color: MUTED,
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        },
    }
}

pub fn nav_btn(active: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_theme, status| {
        if active {
            button::Style {
                background: Some(Background::Color(ACTIVE)),
                text_color: Color::WHITE,
                border: Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        } else {
            match status {
                button::Status::Hovered => button::Style {
                    background: Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.04))),
                    text_color: Color::WHITE,
                    border: Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                _ => button::Style {
                    background: None,
                    text_color: MUTED,
                    border: Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            }
        }
    }
}

pub fn toggle_btn(active: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_theme, status| {
        if active {
            button::Style {
                background: Some(Background::Color(PRIMARY)),
                text_color: Color::WHITE,
                border: Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        } else {
            match status {
                button::Status::Hovered => button::Style {
                    background: Some(Background::Color(ACTIVE)),
                    text_color: Color::WHITE,
                    border: Border {
                        color: BORDER,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                },
                _ => button::Style {
                    background: Some(Background::Color(Color::TRANSPARENT)),
                    text_color: MUTED,
                    border: Border {
                        color: BORDER,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                },
            }
        }
    }
}

// -- Helpers --

pub fn separator<'a, M: 'a>() -> Element<'a, M> {
    container(Space::new())
        .width(Fill)
        .height(1)
        .style(|_theme| container::Style {
            background: Some(Background::Color(Color::from_rgba(
                0.204, 0.259, 0.337, 0.5,
            ))),
            ..Default::default()
        })
        .into()
}
