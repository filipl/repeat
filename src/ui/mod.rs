use breadx::display::Display;

mod canvas;
mod text;
mod window;

pub trait Widget {
    fn width() -> usize;
    fn height() -> usize;

    fn draw<D: Display>(display: &mut D);
}

pub use window::Window;
pub use window::WindowAction;
pub use canvas::Canvas;