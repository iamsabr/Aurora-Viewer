//! The "Accept in" destinations saved for each account (Catznip's per-account
//! InventoryOfferAcceptInOptions): one file in the account's own folder, read
//! when the account's session starts and written when the editor saves. It
//! holds the list and the auto-select choice. It is local data, never sent to
//! the grid or to anyone else. The file is a few hundred bytes, read once per
//! login and written on the editor's Save, so it stays on the frame thread.

use crate::world::contents_destination::Saved;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

const FILE: &str = "accept_in_destinations.json";

/// What the account keeps: its destinations and the auto-select choice of the
/// editor ("automatically select the folder the offering object was rezzed from").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stored {
    pub auto_select: bool,
    pub list: Vec<Saved>,
}

impl Default for Stored {
    fn default() -> Self {
        Self {
            auto_select: true,
            list: Vec::new(),
        }
    }
}

impl Stored {
    /// Give an id to each entry that has none (a file written before entries
    /// had ids); true when one was given, for the file to be written again.
    fn give_ids(&mut self) -> bool {
        let mut given = false;
        for entry in self.list.iter_mut().filter(|entry| entry.id.is_nil()) {
            entry.id = Uuid::new_v4();
            given = true;
        }
        given
    }
}

fn path(agent: Uuid) -> PathBuf {
    crate::settings::account_dir(&agent).join(FILE)
}

/// The account's saved choices; the defaults when there is no file or it cannot be read.
pub fn load(agent: Uuid) -> Stored {
    let mut stored = match std::fs::read_to_string(path(agent)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
            log::warn!("accept-in destinations unreadable: {err}");
            Stored::default()
        }),
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                log::warn!("accept-in destinations not read: {err}");
            }
            Stored::default()
        }
    };
    // the ids the choices name must stay the same from one login to the next
    if stored.give_ids()
        && let Err(err) = save(agent, &stored)
    {
        log::warn!("accept-in destinations not saved: {err}");
    }
    stored
}

/// Keep the account's saved choices (written whole, never half).
pub fn save(agent: Uuid, stored: &Stored) -> std::io::Result<()> {
    let path = path(agent);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(stored).map_err(std::io::Error::other)?;
    crate::atomic_file::write(&path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choices_are_kept_for_their_account_only() {
        let (mine, other) = (Uuid::new_v4(), Uuid::new_v4());
        let stored = Stored {
            auto_select: false,
            list: vec![Saved {
                id: Uuid::new_v4(),
                name: "Work".into(),
                folder: Uuid::from_u128(9),
                subfolder: "%yyyy/%mm".into(),
            }],
        };
        save(mine, &stored).expect("saved");
        assert_eq!(load(mine), stored);
        assert_eq!(load(other), Stored::default());
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        // an agent id no account uses: nothing saved there
        assert_eq!(load(Uuid::from_u128(0xfeed_0001)), Stored::default());
        assert!(Stored::default().auto_select);
    }

    /// An entry saved before entries had ids gets one, written back so that
    /// the next login reads the same id.
    #[test]
    fn an_entry_without_an_id_gets_one_that_stays() {
        let agent = Uuid::new_v4();
        let old = Stored {
            auto_select: true,
            list: vec![Saved {
                id: Uuid::nil(),
                name: "Old".into(),
                folder: Uuid::from_u128(9),
                subfolder: String::new(),
            }],
        };
        save(agent, &old).expect("saved");
        let first = load(agent);
        assert!(!first.list[0].id.is_nil());
        assert_eq!(load(agent).list[0].id, first.list[0].id, "the same id at the next login");
    }
}
