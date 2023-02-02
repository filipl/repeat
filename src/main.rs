#![allow(dead_code)]

mod clipboard;
mod db;
mod options;
mod rpc;
mod ui;

use log::{debug, error, info, trace};
use std::env;
use std::sync::{Arc, Mutex};
use x11_clipboard::Clipboard;

use crate::ui::Window;
use breadx::prelude::*;
use breadx::rt_support::tokio_support;
use futures::StreamExt;
use tokio::sync::Mutex as AsyncMutex;

fn monitor(primary: bool) {
    let clipboard = Clipboard::new().unwrap();
    let mut last = String::new();
    let what = if primary {
        clipboard.getter.atoms.primary
    } else {
        clipboard.getter.atoms.clipboard
    };

    loop {
        if let Ok(curr) = clipboard.load_wait(
            what,
            clipboard.getter.atoms.utf8_string,
            clipboard.getter.atoms.property,
        ) {
            let curr = String::from_utf8_lossy(&curr);
            let curr = curr.trim_matches('\u{0}').trim();
            if !curr.is_empty() && last != curr {
                last = curr.to_owned();
                info!("Contents of primary selection {}: {}", what, last);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args: Vec<_> = env::args().collect();
    info!("args: {:?}", args);
    if args.len() > 1 {
        let client = rpc::create_client("/tmp/repeat.socket").await?;
        client.show(tarpc::context::current()).await?;
        return Ok(());
    }

    let options = options::Options {
        font_size: 20f32,
        font_name: Some("Arial".to_owned()),
    };

    let database = Arc::new(db::Database::new());
    let connection = Arc::new(AsyncMutex::new(tokio_support::connect(None).await?));
    let window: Arc<Mutex<Option<Window>>> = Arc::new(Mutex::new(None));
    let mut clipboard = {
        let mut dpy = connection.lock().await;
        clipboard::Clipboard::new(&mut *dpy, database.clone()).await?
    };

    let (rpc_sender, mut rpc_receiver) = futures::channel::mpsc::channel::<rpc::Message>(10);

    rpc::start_server("/tmp/repeat.socket", rpc_sender).await?;

    loop {
        tokio::select! {
            // incoming X11 events
            ev = async { connection.lock().await.wait_for_event().await } => {
                let event = ev?;

                trace!("event: {:?}", event);

                // update any open windows
                let mut locked_window = window.lock().unwrap();
                let keep_open = match locked_window.as_mut() {
                    Some(w) => {
                        let mut c = connection.lock().await;
                        match w.handle_event(&mut *c, &event).await? {
                            ui::WindowAction::TakeOwnership => {
                                clipboard.take_ownership(&mut *c).await?;
                                false
                            }
                            ui::WindowAction::JustClose => false,
                            ui::WindowAction::StayOpen => true,
                        }
                    },
                    _ => true,
                };
                if !keep_open {
                    debug!("closing window");
                    *locked_window = None;
                }

                // update clipboard
                {
                    let mut con = connection.lock().await;
                    clipboard.handle_event(&mut *con, &event).await?;
                }

            }

            // RPC messages
            command = rpc_receiver.next() => {
                trace!("got a command {:?}", command);
                match command {
                    Some(rpc::Message::Own) => {
                        clipboard.take_ownership(&mut *connection.lock().await).await?;
                    }
                    Some(rpc::Message::Show) => {
                        info!("showing window");
                        let mut locked_window = window.lock().unwrap();
                        if locked_window.is_none() {
                            *locked_window = Some(Window::create(&mut *connection.lock().await, database.clone(), &options).await?);
                        };
                    }
                    None => {
                        error!("rpc server shut down?");
                    }
                }
            }
        }
    }
}
