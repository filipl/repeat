pub struct Options {
    pub font_size: f32,
    pub font_name: Option<String>,
    //pub theme: Theme,
}

pub struct Color {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

impl Color {
    pub fn red() -> Color {
        Color {
            red: 255f32,
            green: 0f32,
            blue: 0f32,
        }
    }

    pub fn white() -> Color {
        Color {
            red: 255f32,
            green: 255f32,
            blue: 255f32,
        }
    }
}

pub struct Theme {
    text: Color,
    highlight: Color,
    background: Color,
}
