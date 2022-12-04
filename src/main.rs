#![allow(dead_code)]

mod clipboard;
mod db;
mod options;
mod ui;
mod rpc;

use std::borrow::BorrowMut;
use std::env;
use std::sync::{Arc, Mutex};
use x11_clipboard::Clipboard;
use log::{debug, info, trace};

use crate::ui::Window;
use breadx::{display::DisplayConnection, prelude::*};
use breadx::rt_support::tokio_support;

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

    //let t1 = thread::spawn(move || { monitor(true) });
    //let t2 = thread::spawn(move || { monitor(false) });

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
    database.add_clip(db::Clip {
        source: db::Source::Clipboard,
        contents: db::ClipContents::Text("clipboard".to_owned()),
    });
    database.add_clip(db::Clip {
        source: db::Source::Primary,
        contents: db::ClipContents::Text("primary".to_owned()),
    });
    database.add_clip(db::Clip {
        source: db::Source::Secondary,
        contents: db::ClipContents::Text("secondary".to_owned()),
    });
    for _ in 1..100 {
        database.add_clip(db::Clip {
            source: db::Source::Secondary,
            contents: db::ClipContents::Text("secondary".to_owned()),
        });
    }

    // TODO: use connection for clipboarding
    let mut connection = tokio_support::connect(None).await?;
    //let mut connection = Arc::new(Mutex::new(DisplayConnection::connect(None)?));
    //let mut connection = Arc::new(Mutex::new(tokio_support::connect(None).await?));
    let window: Arc<Mutex<Option<Window>>> = Arc::new(Mutex::new(None));
        //Some(Window::create(&mut *connection.lock().unwrap(), database.clone(), &options)?);

    let mut running = true;

    rpc::start_server("/tmp/repeat.socket", Box::new(connection), window.clone()).await?;

    while running {
        //let mut unlocked = connection.lock().unwrap();
        let event = connection.wait_for_event().await?;

        // update any open windows
        let mut locked_window = window.lock().unwrap();
        let keep_open = match locked_window.as_mut() {
            Some(w) => w.handle_event(connection, &event)?,
            _ => true,
        };
        if !keep_open {
            debug!("closing window");
            *locked_window = None;
            running = false;
        }

        trace!("event: {:?}", event)
    }

    Ok(())
}
