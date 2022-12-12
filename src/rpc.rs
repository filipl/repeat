use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::error::Error;
use std::path::Path;
use std::sync::{Arc, Mutex};

use breadx::display::DisplayConnection;
use futures::channel::mpsc::Sender;
use futures::{future, prelude::*};
use log::{debug, error};
use tarpc::serde_transport::unix::listen;
use tarpc::server::Channel;
use tarpc::tokio_serde::formats::Json;
use tarpc::{client, context, server};
use tokio::io::unix::AsyncFd;
use tokio::sync::Mutex as AsyncMutex;

use crate::db::Database;
use crate::options::Options;
use crate::ui::Window;

#[tarpc::service]
pub trait Manager {
    async fn show();
}

#[derive(Clone)]
struct Server {
    sender: Arc<AsyncMutex<Sender<Message>>>,
}

#[derive(Debug)]
pub enum Message {
    Show,
}

#[tarpc::server]
impl Manager for Server {
    async fn show(self, _: context::Context) {
        debug!("trying to show window");

        {
            debug!("showing window");

            let _ = self.sender.lock().await.send(Message::Show).await;

            debug!("showed window");
        }
    }
}

pub async fn start_server<P: AsRef<Path>>(
    path: P,
    sender: Sender<Message>,
) -> Result<(), Box<dyn Error>> {
    if path.as_ref().exists() {
        std::fs::remove_file(&path)?;
    }

    let asender = Arc::new(AsyncMutex::new(sender));
    let listener = listen(path, Json::default).await?;
    tokio::spawn(
        listener
            .filter_map(|r| future::ready(r.ok()))
            .map(server::BaseChannel::with_defaults)
            .map(move |channel| {
                let server = Server {
                    sender: asender.clone(),
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
