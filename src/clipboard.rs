use std::borrow::Cow;
use crate::clipboard::GetState::{GetTargets, GetText};
use crate::db;
use crate::db::{Clip, ClipContents, Database};
use breadx::prelude::*;
use breadx::protocol::xfixes::SelectionEventMask;
use breadx::protocol::xproto::{AtomEnum, EventMask};
use breadx::protocol::{xproto, Event};
use log::{debug, info, trace, warn};
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

const SELECTIONS: &'static [&'static str] = &["PRIMARY", "SECONDARY", "CLIPBOARD"];
const TARGETS: &'static str = "TARGETS";

pub struct Clipboard {
    getter: xproto::Window,
    setter: xproto::Window,
    get_states: HashMap<xproto::Atom, GetState>,
    atoms: HashMap<String, xproto::Atom>,
    database: Arc<Database>,
}

#[derive(Debug)]
enum GetState {
    GetTargets(xproto::Atom),
    GetText(xproto::Atom),
}

// Note: To get around Void not being implemented for &[u8]
struct WrappedU8 {
    data: Vec<u8>
}

impl breadx::Void for WrappedU8 {
    fn bytes(&self) -> &[u8] {
        &self.data
    }
}

impl Clipboard {
    pub async fn new<D: AsyncDisplay>(
        dpy: &mut D,
        database: Arc<Database>,
    ) -> Result<Clipboard, Box<dyn Error>> {
        // create window
        dpy.xfixes_query_version_immediate(5, 0).await?;

        let def_screen = dpy.default_screen();
        let root = def_screen.root;
        let visual = def_screen.root_visual;

        for name in SELECTIONS {
            let selection = dpy.intern_atom_immediate(false, name).await?;
            dpy.xfixes_select_selection_input(
                root,
                selection.atom,
                SelectionEventMask::SET_SELECTION_OWNER
                    | SelectionEventMask::SELECTION_CLIENT_CLOSE
                    | SelectionEventMask::SELECTION_WINDOW_DESTROY,
            )
            .await?;
        }

        let mask = xproto::CreateWindowAux::new()
            .event_mask(EventMask::STRUCTURE_NOTIFY | EventMask::PROPERTY_CHANGE);
        let getter = dpy.generate_xid().await?;
        let setter = dpy.generate_xid().await?;
        dpy.create_window_checked(
            0,
            getter,
            root,
            0,
            0,
            1,
            1,
            0,
            xproto::WindowClass::INPUT_OUTPUT,
            visual,
            mask,
        )
        .await?;
        dpy.create_window_checked(
            0,
            setter,
            root,
            0,
            0,
            1,
            1,
            0,
            xproto::WindowClass::INPUT_OUTPUT,
            visual,
            mask,
        )
        .await?;

        let mut c = Clipboard {
            getter,
            setter,
            get_states: HashMap::new(),
            atoms: HashMap::new(),
            database,
        };
        c.fetch_initial(dpy).await?;
        Ok(c)
    }

    async fn fetch_initial<D: AsyncDisplay>(&mut self, dpy: &mut D) -> Result<(), Box<dyn Error>> {
        for sel in SELECTIONS {
            let atom = self.get_atom(dpy, sel, false).await?;
            self.get_targets(dpy, atom).await?;
        }
        Ok(())
    }

    async fn get_atom_name<D: AsyncDisplay>(
        &mut self,
        dpy: &mut D,
        atom: xproto::Atom,
    ) -> Result<String, Box<dyn Error>> {
        for (name, a) in self.atoms.iter() {
            if *a != atom {
                continue;
            } else {
                return Ok(name.to_owned());
            }
        }
        let reply = dpy.get_atom_name_immediate(atom).await?;
        let name = String::from_utf8_lossy(&reply.name).to_string();
        self.atoms.insert(name.clone(), atom);
        Ok(name)
    }

    async fn get_atom<D: AsyncDisplay>(
        &mut self,
        dpy: &mut D,
        name: &str,
        only_if_exists: bool,
    ) -> Result<xproto::Atom, Box<dyn Error>> {
        match self.atoms.get(name) {
            None => {
                let reply = dpy.intern_atom_immediate(only_if_exists, name).await?;
                self.atoms.insert(name.to_owned(), reply.atom);
                Ok(reply.atom)
            }
            Some(a) => Ok(a.clone()),
        }
    }

    async fn get_free_getter_property<D: AsyncDisplay>(
        &mut self,
        dpy: &mut D,
    ) -> Result<xproto::Atom, Box<dyn Error>> {
        let mut num = 0;
        loop {
            let name = format!("REPEAT_{}", num);
            let atom = self.get_atom(dpy, &name, false).await?;
            num = num + 1;
            if self.get_states.contains_key(&atom) {
                continue;
            } else {
                return Ok(atom);
            }
        }
    }

    async fn fetch_string<D: AsyncDisplay>(
        &mut self,
        dpy: &mut D,
        selection: xproto::Atom,
        target: xproto::Atom,
    ) -> Result<(), Box<dyn Error>> {
        let property = self.get_selection_property(dpy, selection, target).await?;
        debug!("fetching string to property {}", property);
        self.get_states.insert(property, GetText(property));
        Ok(())
    }

    async fn fetch_image<D: AsyncDisplay>(
        &mut self,
        _dpy: &mut D,
        _selection: xproto::Atom,
        _property: xproto::Atom,
    ) -> Result<(), Box<dyn Error>> {
        warn!("image not supported yet");
        Ok(())
    }

    async fn get_selection_property<D: AsyncDisplay>(
        &mut self,
        dpy: &mut D,
        selection: xproto::Atom,
        target: xproto::Atom,
    ) -> Result<xproto::Atom, Box<dyn Error>> {
        let property = self.get_free_getter_property(dpy).await?;
        trace!("queued getter {}", property);
        dpy.delete_property_checked(self.getter, property).await?;
        dpy.convert_selection_checked(
            self.getter,
            selection,
            target,
            property,
            0, // TODO: Use something else than 0
        )
        .await?;
        Ok(property)
    }

    async fn get_targets<D: AsyncDisplay>(
        &mut self,
        dpy: &mut D,
        selection: xproto::Atom,
    ) -> Result<(), Box<dyn Error>> {
        let targets = self.get_atom(dpy, TARGETS, true).await?;
        let property = self.get_selection_property(dpy, selection, targets).await?;
        self.get_states.insert(property, GetTargets(property));
        Ok(())
    }

    pub async fn take_ownership<D: AsyncDisplay>(&mut self, dpy: &mut D) -> Result<(), Box<dyn Error>> {
        info!("taking ownership");
        let primary = self.get_atom(dpy, "PRIMARY", true).await?;
        dpy.set_selection_owner_checked(self.setter, primary, 0).await?;
        Ok(())
    }

    pub async fn handle_event<D: AsyncDisplay>(
        &mut self,
        dpy: &mut D,
        event: &Event,
    ) -> Result<(), Box<dyn Error>> {
        match event {
            Event::SelectionRequest(sr) => {
                let targets_atom = self.get_atom(dpy, TARGETS, true).await?;
                let string_atom = self.get_atom(dpy, "UTF8_STRING", false).await?;
                if sr.target == targets_atom {
                    // it wants to know what we serve
                    match self.database.selection() {
                        None => {
                            debug!("requested - but nothing available");
                            // we serve nothing
                            let d = WrappedU8 { data: Vec::new() };
                            dpy.change_property_checked(
                                xproto::PropMode::REPLACE,
                                sr.requestor,
                                0,
                                xproto::Atom::from(AtomEnum::ATOM),
                                0,
                                0,
                                &d,
                            )
                            .await?;
                        }
                        Some(clip) => {
                            let property = match clip.contents {
                                ClipContents::Text(_) => {
                                    string_atom
                                }
                            };
                            debug!("requested - sending targets");
                            // TODO: Decide what properties to actually have / clip
                            let data: &[u32] = &[targets_atom, property];
                            let mut data_u8: Vec<u8> = Vec::with_capacity(data.len() * 4);
                            for item in data {
                                data_u8.extend(&item.to_le_bytes());
                            }
                            debug!("sending data: {:?}", data_u8);
                            let d = WrappedU8 { data: data_u8 };
                            dpy.change_property_checked(
                                xproto::PropMode::REPLACE,
                                sr.requestor,
                                sr.property,
                                xproto::Atom::from(AtomEnum::ATOM),
                                32,
                                data.len().try_into().expect("too many elements"),
                                &d
                            )
                            .await?;
                        }
                    }
                } else if sr.target == string_atom {
                    let str = match self.database.selection() {
                        None => {
                            "n/a".to_owned()
                        }
                        Some(clip) => {
                            match clip.contents {
                                ClipContents::Text(txt) => txt
                            }
                        }
                    };
                    let d = WrappedU8 { data: Vec::from(str) };
                    dpy.change_property_checked(
                        xproto::PropMode::REPLACE,
                        sr.requestor,
                        sr.property,
                        string_atom,
                        8,
                        d.data.len() as u32,
                        &d
                    ).await?;
                }
                let notify_event = xproto::SelectionNotifyEvent {
                    response_type: xproto::SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: 0,
                    requestor: sr.requestor,
                    selection: sr.selection,
                    target: sr.target,
                    property: sr.property,
                };
                let event = xproto::SendEventRequest {
                    propagate: false,
                    destination: sr.requestor,
                    event_mask: 0,
                    event: Cow::Owned(notify_event.into()),
                };
                info!("sent notification: {:?}", notify_event);
                dpy.send_void_request(event, false).await?;

                //dpy.send_event_checked(false, sr.requestor, EventMask::default(), notify_event).await?;
            }
            Event::XfixesSelectionNotify(sn) => {
                if sn.owner != self.setter {
                    self.get_targets(dpy, sn.selection).await?;
                }
            }
            Event::SelectionNotify(sn) => {
                match self.get_states.get(&sn.property) {
                    None => {
                        warn!("some other unhandled property changed: {}", sn.property);
                    }
                    Some(&GetTargets(property)) => {
                        debug!("got targets for {}", property);
                        let targets = dpy
                            .get_property_immediate(false, self.getter, property, 0, 0, u32::MAX)
                            .await?;
                        let mut properties = Vec::new();
                        dpy.delete_property_checked(self.getter, sn.property)
                            .await?;
                        for prop_atom in targets.value.chunks(4) {
                            if prop_atom.len() != 4 {
                                warn!("got a non-aligned TARGETS reply");
                                continue;
                            }

                            let atom = i32::from_le_bytes(prop_atom.try_into().unwrap());
                            if atom == 0 {
                                continue;
                            }
                            let name = self.get_atom_name(dpy, atom as xproto::Atom).await?;
                            properties.push(name);
                        }
                        self.get_states.remove(&property);

                        debug!("available properties: {:?}", properties);
                        if properties.contains(&"UTF8_STRING".to_owned()) {
                            let target = self.get_atom(dpy, &"UTF8_STRING", true).await?;
                            self.fetch_string(dpy, sn.selection, target).await?;
                        } else {
                            let images: Vec<&String> = properties
                                .iter()
                                .filter(|p| p.starts_with("image/"))
                                .collect();
                            if images.len() > 0 {
                                // TODO: Chose the less lossy one
                                let target =
                                    self.get_atom(dpy, images.first().unwrap(), true).await?;
                                self.fetch_image(dpy, sn.selection, target).await?;
                            }
                        }
                    }
                    Some(&GetText(property)) => {
                        let value_reply = dpy
                            .get_property_immediate(true, sn.requestor, sn.property, 0, 0, u32::MAX)
                            .await?;
                        let value = String::from_utf8_lossy(&value_reply.value).to_string();
                        info!("property {} value ({}): {:?}", property, value.len(), value);
                        let contents = ClipContents::Text(value);
                        self.database.add_clip(Clip {
                            source: db::Source::Primary,
                            contents,
                        });
                        self.get_states.remove(&property);
                    }
                }
            }
            Event::PropertyNotify(pn) => {
                if pn.window == self.getter {
                    if pn.state == xproto::Property::NEW_VALUE {
                        let target_reply = dpy
                            .get_property_immediate(false, pn.window, pn.atom, 0, 0, u32::MAX)
                            .await?;
                        trace!(
                            "new property notify (atom:{}) value: {:?}",
                            pn.atom,
                            target_reply.value
                        );
                    }
                }
            }

            _ => {}
        }
        Ok(())
    }

    pub async fn print_owners<D: AsyncDisplay>(dpy: &mut D) -> Result<(), Box<dyn Error>> {
        for name in SELECTIONS {
            let selection = dpy.intern_atom_immediate(false, name).await?;
            let owner = dpy.get_selection_owner_immediate(selection.atom).await?;
            info!("owner of {}: {:?}", name, owner.owner);
        }

        Ok(())
    }
}
