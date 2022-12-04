use std::{boxed::Box, error::Error};

use font_loader::system_fonts;
use rusttype::Font;

pub fn font(family: Option<&str>) -> Result<Font<'static>, Box<dyn Error>> {
    let name = match family {
        None => "monospace",
        Some(name) => name,
    };

    let property = system_fonts::FontPropertyBuilder::new()
        .monospace()
        .family(name)
        .build();
    let (font_data, _) =
        system_fonts::get(&property).ok_or("Could not get system fonts property")?;

    let font: Font<'static> = Font::try_from_vec(font_data).expect("Error constructing Font");
    Ok(font)
}
