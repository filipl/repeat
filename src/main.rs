#![allow(dead_code)]

mod clipboard;
mod db;
mod options;
mod rpc;
mod ui;

use log::{debug, error, info, trace};
use std::env;
use std::sync::{Arc, Mutex};

use crate::ui::Window;
use breadx::prelude::*;
use breadx::rt_support::tokio_support;
use futures::StreamExt;
use tokio::sync::Mutex as AsyncMutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args: Vec<_> = env::args().collect();
    info!("args: {:?}", args);
    if args.len() > 1 {
        let client = rpc::create_client("/tmp/repeat.socket").await?;
        match args.get(1).map(|c| c.as_ref()) {
            Some("show") => client.show(tarpc::context::current()).await?,
            Some("pause") => client.pause(tarpc::context::current()).await?,
            Some("start") => client.start(tarpc::context::current()).await?,
            _ => {
                error!("not a valid argument");
            }
        }
        return Ok(());
    }

    let options = options::Options {
        font_size: 20f32,
        font_name: Some("Monospace".to_owned()),
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
                            ui::WindowAction::TakeOwnership(clip) => {
                                database.select_clip(clip);
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
                    Some(rpc::Message::Pause) => {
                        clipboard.pause();
                    }
                    Some(rpc::Message::Start) => {
                        clipboard.start();
                    }
                    None => {
                        error!("rpc server shut down?");
                    }
                }
            }
        }
    }
}
