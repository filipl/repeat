use crate::clipboard::GetState::GetText;
use breadx::prelude::*;
use breadx::protocol::xfixes::SelectionEventMask;
use breadx::protocol::xproto::EventMask;
use breadx::protocol::{xproto, Event};
use log::{debug, info, trace, warn};
use std::collections::HashMap;
use std::error::Error;

const SELECTIONS: &'static [&'static str] = &["PRIMARY", "SECONDARY", "CLIPBOARD"];
const TARGETS: &'static str = "TARGETS";

pub struct Clipboard {
    getter: xproto::Window,
    setter: xproto::Window,
    get_states: HashMap<xproto::Atom, GetState>,
    atoms: HashMap<String, xproto::Atom>,
}

#[derive(Debug)]
enum GetState {
    GetTargets { property: xproto::Atom },
    GetText(xproto::Atom),
}

impl Clipboard {
    pub async fn new<D: AsyncDisplay>(dpy: &mut D) -> Result<Clipboard, Box<dyn Error>> {
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
            num = num + 1;
            if self.atoms.contains_key(&name) {
                continue;
            } else {
                return self.get_atom(dpy, &name, false).await;
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
        self.get_states
            .insert(property, GetState::GetTargets { property });
        Ok(())
    }

    pub async fn handle_event<D: AsyncDisplay>(
        &mut self,
        dpy: &mut D,
        event: &Event,
    ) -> Result<(), Box<dyn Error>> {
        match event {
            Event::XfixesSelectionNotify(sn) => {
                self.get_targets(dpy, sn.selection).await?;
            }
            Event::SelectionNotify(sn) => {
                match self.get_states.get(&sn.property) {
                    None => {
                        warn!("some other unhandled property changed: {}", sn.property);
                    }
                    Some(&GetState::GetTargets { property }) => {
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
                        debug!(
                            "property notify (atom:{}) value: {:?}",
                            pn.atom, target_reply.value
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
