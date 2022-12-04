use futures::{future, prelude::*};
use log::{debug, info};
use std::error::Error;
use std::path::Path;
use std::sync::{Arc, Mutex};
use breadx::display::{AsyncDisplay, DisplayConnection};
use tarpc::serde_transport::unix::listen;
use tarpc::server::Channel;
use tarpc::tokio_serde::formats::Json;
use tarpc::{client, context, server};
use tokio::io::unix::AsyncFd;
use crate::ui::Window;

#[tarpc::service]
pub trait Manager {
    async fn show();
}

#[derive(Clone)]
struct Server {
    dpy: Box<dyn AsyncDisplay>,
    window: Arc<Mutex<Option<Window>>>,
}

#[tarpc::server]
impl Manager for Server {
    async fn show(self, _: context::Context) {
        debug!("showing window");

        //let dpy = self.dpy.lock().unwrap();
        let window = self.window.lock().unwrap();
        let new_window = match *window {
            None => {}
            Some(_) => {
                info!("it's already showing - won't do anything");
            }
        };

        debug!("showed window");
    }
}

pub async fn start_server<P: AsRef<Path>>(path: P, dpy: Box<dyn AsyncDisplay>, window: Arc<Mutex<Option<Window>>>) -> Result<(), Box<dyn Error>> {
    if path.as_ref().exists() {
        std::fs::remove_file(&path)?;
    }

    let listener = listen(path, Json::default).await?;
    tokio::spawn(
        listener
            .filter_map(|r| future::ready(r.ok()))
            .map(server::BaseChannel::with_defaults)
            .map(move |channel| {
                let server = Server {
                    dpy,
                    window: window.clone()
                };
                channel.execute(server.serve())
            })
            .buffer_unordered(10)
            .for_each(|_| async {}),
    );

    Ok(())
}

pub async fn create_client<P: AsRef<Path>>(path: P) -> Result<ManagerClient, Box<dyn Error>> {
    let transport = tarpc::serde_transport::unix::connect(path, Json::default);
    let client = ManagerClient::new(client::Config::default(), transport.await?).spawn();

    Ok(client)
}