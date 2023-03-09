use std::borrow::Cow;
use std::error::Error;
use std::sync::Arc;

use crate::db::{Clip, ClipContents, Database};
use crate::options::{Color, Options};
use crate::ui;
use breadx::protocol::xproto::{ModMask, SendEventRequest};
use breadx::protocol::{self, xproto::EventMask, Event};
use breadx::{prelude::*, protocol::xproto};
use breadx_keysyms::{keysyms, KeyboardState};
use log::{debug, error};
use crate::clipboard::Clipboard;
use crate::ui::window::WindowAction::{CloseWindow, StayOpen};

pub struct Window {
    keyboard_state: KeyboardState,
    window: xproto::Window,
    focused_window: xproto::Window,
    root: xproto::Window,
    database: Arc<Database>,
    canvas: ui::canvas::Canvas,
    input: String,
    modes: Modes,
    searches: Vec<Clip>,
    current_choice: usize,
}

struct Modes {
    shift: bool,
    ctrl: bool,
}

pub enum WindowAction {
    CloseWindow,
    StayOpen,
}

impl Window {
    pub async fn create<D: AsyncDisplay>(
        display: &mut D,
        database: Arc<Database>,
        options: &Options,
    ) -> Result<Window, Box<dyn Error>> {
        let focused_window = get_focused_window(display).await?;
        let geom = get_active_screen_geom(display).await?;
        debug!("active screen geom {:?}", geom);

        let wid = display.generate_xid().await?;
        let def_screen = display.default_screen();
        let root = def_screen.root;
        let width = 800u16;
        let height = 600u16;
        display.create_window_checked(
            0,
            wid,
            root,
            (geom.x + geom.width as i16 / 2i16 - width as i16 / 2i16).into(),
            (geom.y + geom.height as i16 / 2i16 - height as i16 / 2i16).into(),
            width,
            height,
            2,
            xproto::WindowClass::COPY_FROM_PARENT,
            0,
            xproto::CreateWindowAux::new()
                .background_pixel(display.default_screen().white_pixel)
                .override_redirect(1)
                .event_mask(
                    EventMask::EXPOSURE
                        | EventMask::KEY_PRESS
                        | EventMask::KEY_RELEASE
                        | EventMask::VISIBILITY_CHANGE
                        | EventMask::FOCUS_CHANGE,
                ),
        ).await?;

        let canvas = ui::canvas::Canvas::new(display, wid, width, height, &options).await?;
        let keyboard_state = KeyboardState::new_async(display).await?;

        let mut w = Window {
            keyboard_state,
            window: wid,
            focused_window,
            root,
            database,
            canvas,
            input: String::new(),
            modes: Modes {
                shift: false,
                ctrl: false,
            },
            searches: Vec::new(),
            current_choice: 0,
        };

        w.redraw();
        w.canvas.draw(display).await?;
        w.show(display).await?;

        Ok(w)
    }

    pub async fn hide<D: AsyncDisplay>(&self, display: &mut D) -> breadx::Result<()> {
        display.unmap_window_checked(self.window).await
    }

    pub async fn destroy<D: AsyncDisplay>(&self, display: &mut D) -> breadx::Result<()> {
        display.destroy_window_checked(self.window).await
    }

    pub async fn show<D: AsyncDisplay>(&mut self, display: &mut D) -> breadx::Result<()> {
        let focused_window = get_focused_window(display).await?;
        self.focused_window = focused_window;
        self.research();

        display.map_window_checked(self.window).await?;
        let cookie = display.send_void_request(
            xproto::SetInputFocusRequest {
                focus: self.window,
                revert_to: xproto::InputFocus::PARENT,
                ..Default::default()
            },
            true,
        ).await?;
        self.redraw();
        display.wait_for_reply(cookie).await
    }

    fn research(&mut self) {
        self.current_choice = 0;
        if self.input.is_empty() {
            self.searches = self.database.clips().iter().rev().take(100).map(|c| c.clone()).collect();
        } else {
            self.searches = self.database.search(&self.input, 100);
        }
    }

    fn redraw(&mut self) {
        self.canvas.clear();
        self.canvas.draw_text(&self.input, &Color::red(), 0, 0);
        let max_rows = self.canvas.text_rows();
        let mut row_offset = 1;
        for (i, clip) in self.searches.iter().enumerate() {
            if i > max_rows {
                break;
            }
            match &clip.contents.as_ref() {
                &ClipContents::Text(text) => {
                    let color = if self.current_choice == i { Color::green() } else { Color::white() };
                    let mut r = 0;
                    for row in text.lines() {
                        if r == 5 {
                            // TODO: Configurable size
                            let extra_rows = text.lines().count() - 5;
                            self.canvas.draw_text(&format!(" ... + {} rows", extra_rows), &color, row_offset, 0);
                            row_offset += 1;
                            break;
                        } else {
                            if r == 0 {
                                self.canvas.draw_text(&format!("{}", i), &color, row_offset as u16, 0);
                            }
                            // TODO: Calculate the size of three numbers and use as offset
                            self.canvas
                                .draw_text(row, &color, row_offset as u16, 20);
                            row_offset += 1;
                        }
                        r += 1;
                    }
                }
            }
        }
    }

    fn selection_down(&mut self) -> bool {
        if self.current_choice < self.searches.len() - 1 {
            self.current_choice += 1;
            self.redraw();
        }
        true
    }

    fn selection_up(&mut self) -> bool {
        if self.current_choice > 0 {
            self.current_choice -= 1;
            self.redraw();
        }
        true
    }

    pub async fn handle_event<D: AsyncDisplay>(
        &mut self,
        display: &mut D,
        event: &Event,
        clipboard: &mut Clipboard,
    ) -> Result<WindowAction, Box<dyn Error>> {
        match event {
            Event::KeyRelease(kp) => {
                let sym = self.keyboard_state.symbol_async(display, kp.detail, 0).await?;
                match sym {
                    keysyms::KEY_Control_L | keysyms::KEY_Control_R =>
                        self.modes.ctrl = false,
                    keysyms::KEY_Shift_L | keysyms::KEY_Shift_R =>
                        self.modes.shift = false,
                    _ => {}
                }
            }
            Event::KeyPress(kp) => {
                let column = if self.modes.shift { 1 } else if self.modes.ctrl { 2 } else { 0 };
                let sym = self.keyboard_state.symbol_async(display, kp.detail, column).await?;
                let redraw = match sym {
                    keysyms::KEY_Control_L | keysyms::KEY_Control_R => {
                        self.modes.ctrl = true;
                        false
                    }
                    keysyms::KEY_Shift_L | keysyms::KEY_Shift_R => {
                        self.modes.shift = true;
                        false
                    }

                    keysyms::KEY_Escape => {
                        self.hide(display).await?;
                        focus_window(display, self.focused_window).await?;
                        return Ok(CloseWindow);
                    }

                    keysyms::KEY_K | keysyms::KEY_k if self.modes.ctrl =>
                        self.selection_up(),
                    keysyms::KEY_Up =>
                        self.selection_up(),
                    keysyms::KEY_J | keysyms::KEY_j if self.modes.ctrl =>
                        self.selection_down(),
                    keysyms::KEY_Down =>
                        self.selection_down(),

                    keysyms::KEY_u | keysyms::KEY_U if self.modes.ctrl => {
                        self.input.clear();
                        self.research();
                        true
                    }
                    keysyms::KEY_BackSpace => {
                        self.input.pop();
                        self.research();
                        true
                    }

                    keysyms::KEY_Return => {
                        self.hide(display).await?;
                        focus_window(display, self.focused_window).await?;
                        return if !self.searches.is_empty() {
                            // Send Shift + Insert
                            let choice = match self.searches.get(self.current_choice) {
                                None => CloseWindow,
                                Some(clip) => {
                                    self.database.select_clip(clip.clone());
                                    clipboard.take_ownership(display).await?;
                                    if !self.modes.ctrl {
                                        send_key(display, self.focused_window, self.root, 118, ModMask::SHIFT).await?;
                                    }
                                    CloseWindow
                                }
                            };
                            Ok(choice)
                        } else {
                            Ok(CloseWindow)
                        };
                    }
                    _ if self.modes.ctrl => {
                        true
                    }
                    key => {
                        if let Some(char) = char::from_u32(key) {
                            self.input.push(char);
                            self.research();
                        }
                        true
                    }
                };
                if redraw {
                    self.redraw();
                    self.canvas.draw(display).await?;
                }
            }
            Event::Expose(ee) if ee.window == self.window => {
                self.canvas.draw(display).await?;
            }
            Event::FocusOut(_fe) => {
                focus_window(display, self.window).await?;
            }
            _ => {}
        }
        Ok(StayOpen)
    }
}

#[derive(Debug)]
struct Geometry {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
}

// TODO: Take a keysym instead and look up the keycode
async fn send_key<D: AsyncDisplay>(
    dpy: &mut D,
    window: xproto::Window,
    root: xproto::Window,
    key: xproto::Keycode,
    modmask: ModMask,
) -> breadx::Result<()> {
    let mut event = xproto::KeyPressEvent {
        response_type: xproto::KEY_PRESS_EVENT,
        detail: key,
        sequence: 0,
        time: 0, // TODO: Need to set this?
        root,
        event: window,
        child: 0,
        root_x: 1,
        root_y: 1,
        event_x: 1,
        event_y: 1,
        state: modmask.into(),
        same_screen: true,
    };
    let press_request = SendEventRequest {
        propagate: true,
        destination: window,
        event_mask: EventMask::KEY_PRESS.into(),
        event: Cow::Owned(event.into()),
    };
    let press_cookie = dpy.send_void_request(press_request, false).await?;
    dpy.wait_for_reply(press_cookie).await?;

    event.response_type = xproto::KEY_RELEASE_EVENT;
    let release_request = SendEventRequest {
        propagate: true,
        destination: window,
        event_mask: EventMask::KEY_RELEASE.into(),
        event: Cow::Owned(event.into()),
    };
    let release_cookie = dpy.send_void_request(release_request, false).await?;
    dpy.wait_for_reply(release_cookie).await
}

async fn focus_window<D: AsyncDisplay>(dpy: &mut D, window: xproto::Window) -> breadx::Result<()> {
    let cookie = dpy.send_void_request(
        xproto::SetInputFocusRequest {
            focus: window,
            revert_to: xproto::InputFocus::PARENT,
            ..Default::default()
        },
        false,
    ).await?;
    dpy.wait_for_reply(cookie).await
}

async fn get_focused_window<D: AsyncDisplay>(connection: &mut D) -> breadx::Result<xproto::Window> {
    // TODO: grab and ungrab with drop
    //connection.grab_server_checked()?;
    let focus = connection.get_input_focus().await?;
    connection.wait_for_reply(focus).await.map(|r| r.focus)
    //connection.ungrab_server_checked()?
}

async fn get_active_screen_geom<D: AsyncDisplay>(connection: &mut D) -> breadx::Result<Geometry> {
    let focus = get_focused_window(connection).await?;
    let resources = {
        let request = protocol::randr::GetScreenResourcesRequest { window: focus };
        let cookie = connection.send_reply_request(request).await?;
        connection.wait_for_reply(cookie).await?
    };

    let geom = connection.get_geometry_immediate(focus).await?;
    let absolute = connection.translate_coordinates_immediate(focus, geom.root, geom.x, geom.y).await?;

    // TODO: Perhaps only read until we've found what we're looking for
    let mut crtcs = Vec::new();
    for crtc in resources.crtcs.iter() {
        let request = protocol::randr::GetCrtcInfoRequest {
            crtc: crtc.clone(),
            config_timestamp: 0,
        };
        let cookie = connection.send_reply_request(request).await?;
        let reply = connection.wait_for_reply(cookie).await?;
        if !reply.outputs.is_empty() {
            debug!("crtc {:?}", reply);
            crtcs.push(reply);
        }
    }

    let active_crtc = crtcs
        .iter()
        .find(|crtc| {
            crtc.y <= absolute.dst_y
                && crtc.x <= absolute.dst_x
                && (crtc.y + crtc.height as i16) >= absolute.dst_y
                && (crtc.x + crtc.width as i16) >= absolute.dst_x
        })
        .unwrap_or_else(|| {
            error!("unable to find active screen - taking first");
            crtcs.first().unwrap()
        });

    Ok(Geometry {
        x: active_crtc.x,
        y: active_crtc.y,
        width: active_crtc.width,
        height: active_crtc.height,
    })
}
