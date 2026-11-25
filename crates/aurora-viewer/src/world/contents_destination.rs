//! "Accept in" of the object contents window: the folder the copy of an
//! object's contents goes into. Port of the choice of
//! LLPanelInventoryOfferFolder::getSelectedFolder and
//! LLFloaterOpenObject::moveToInventory (Catznip, indra/newview/
//! llpanelinventoryoffer.cpp and llfloateropenobject.cpp, by Kitty Barnett,
//! originally LGPL 2.1). The new folder, named after the object, is created
//! inside the chosen folder; "[Default]" keeps the usual place.

use crate::world::inventory::{Inventory, actions};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The system folder type of Received Items (FT_INBOX).
const FT_INBOX: i32 = 50;

/// Where the copy goes, as the window's drop-down offers it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Destination {
    /// The usual place: the inventory root, or Clothing for a copy to wear.
    #[default]
    Default,
    /// The folder the object was rezzed from ("[Object: <folder>]").
    ObjectFolder,
    /// The Received Items folder.
    ReceivedItems,
    /// A destination saved for the account, by the entry's id (`Saved::id`):
    /// removing or reordering entries never points it at another one, and
    /// another account's list simply has no such entry.
    Saved(Uuid),
}

/// The window's "Accept in" choice, kept in the preferences.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcceptIn {
    /// The checkbox: off, the usual place is used whatever the choice.
    pub on: bool,
    #[serde(deserialize_with = "lenient_destination")]
    pub choice: Destination,
}

/// A choice the preferences cannot read (an older or newer form of it) is the
/// usual place, rather than an error that would lose the other preferences.
fn lenient_destination<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Destination, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Lenient {
        Known(Destination),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Lenient::deserialize(deserializer)? {
        Lenient::Known(destination) => destination,
        Lenient::Other(_) => Destination::Default,
    })
}

/// The choice an offer's card starts on, until the user picks one on it: the
/// giving object's folder when auto-select is on and that folder is known
/// (the editor's "automatically select" box), else the choice last made on a
/// card. The object's folder belongs to its own offer, so with auto-select off
/// a remembered object's folder starts on the usual place.
pub fn offer_start(remembered: Destination, auto_select: bool, object_known: bool) -> Destination {
    match remembered {
        _ if auto_select && object_known => Destination::ObjectFolder,
        Destination::ObjectFolder => Destination::Default,
        other => other,
    }
}

impl AcceptIn {
    /// The destination a copy asks for: the choice when the box is ticked.
    pub fn requested(self) -> Destination {
        if self.on { self.choice } else { Destination::Default }
    }
}

/// The folder the copy goes into, or None for the usual place. A choice
/// that is not a usable folder now (missing, in Trash, in the Library, or
/// the object's folder unknown) falls back to the usual place, as Catznip
/// does: the copy never stops for it.
pub fn parent(choice: Destination, source: Option<Uuid>, inv: &Inventory) -> Option<Uuid> {
    let folder = match choice {
        Destination::Default | Destination::Saved(_) => return None,
        Destination::ObjectFolder => source?,
        Destination::ReceivedItems => inv.system_folder(FT_INBOX)?,
    };
    usable(inv, folder).then_some(folder)
}

/// The folder a choice files in, saved destinations included: `saved` is the
/// account's list. None for the usual place, or when the folder is not usable now.
pub fn destination_folder(choice: Destination, source: Option<Uuid>, inv: &Inventory, saved: &[Saved]) -> Option<Uuid> {
    match choice {
        Destination::Saved(entry) => find_saved(saved, entry)
            .map(|entry| entry.folder)
            .filter(|folder| usable(inv, *folder)),
        other => parent(other, source, inv),
    }
}

/// The saved entry with this id, if the account's list still has it.
pub fn find_saved(saved: &[Saved], id: Uuid) -> Option<&Saved> {
    saved.iter().find(|entry| entry.id == id)
}

/// The folder an object's Open window names as "[Object: …]": its source
/// folder, when it is a usable destination.
pub fn object_folder(source: Option<Uuid>, inv: &Inventory) -> Option<Uuid> {
    let folder = source?;
    usable(inv, folder).then_some(folder)
}

/// A folder a new folder may be created in: the inventory's own destination
/// rule (`actions::destination`: not in the Library, the Trash, the Current
/// Outfit or My Outfits), and Received Items with its folders, which that rule
/// leaves out but Catznip allows (the Legacy Viewer files received items
/// there). Stricter than Catznip on the outfit trees: a folder of the copy, or
/// an offer, inside an outfit would be read as part of the outfit.
pub fn usable(inv: &Inventory, folder: Uuid) -> bool {
    actions::destination(inv, folder)
        || (actions::in_type(inv, folder, FT_INBOX)
            && inv.folders.contains_key(&folder)
            && !actions::library(inv, folder)
            && !actions::in_type(inv, folder, 14))
}

/// A destination saved in the "Accept in" list (Catznip's
/// InventoryOfferAcceptInOptions): what the drop-down names, the folder it
/// files in, and the subfolder path made under that folder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    /// The entry's own id, which the choices name (`Destination::Saved`). Nil
    /// in a file written before ids existed: `accept_in_saved::load` gives it one.
    pub id: Uuid,
    pub name: String,
    pub folder: Uuid,
    /// The subfolder path, with tokens (see `expand`). Empty: the folder itself.
    pub subfolder: String,
}

/// The deepest subfolder path a destination may name (MAX_SUBFOLDERS_DEPTH).
pub const MAX_SUBFOLDER_DEPTH: usize = 3;

/// Expand the tokens of a subfolder path, following Catznip's tokens: `%yyyy`
/// year, `%yy` last two digits of the year, `%mmm` the month's name (in
/// English, whatever the interface language), `%mm` month, `%dd` day, `%date`
/// as `yyyy-mm-dd`, `%region` the region. The longer tokens first, so that
/// `%yyyy` is not read as `%yy`; the region last, so that a region's name is
/// never read as tokens.
pub fn expand(path: &str, date: chrono::NaiveDate, region: &str) -> String {
    use chrono::Datelike;
    let month = date.format("%B").to_string();
    path.replace("%date", &date.format("%Y-%m-%d").to_string())
        .replace("%yyyy", &format!("{:04}", date.year()))
        .replace("%yy", &format!("{:02}", date.year().rem_euclid(100)))
        .replace("%mmm", &month)
        .replace("%mm", &format!("{:02}", date.month()))
        .replace("%dd", &format!("{:02}", date.day()))
        .replace("%region", region)
}

/// The folder names of a subfolder path: split on `/`, blank parts left out.
/// None when the path is deeper than `MAX_SUBFOLDER_DEPTH` levels.
pub fn subfolder_parts(path: &str) -> Option<Vec<String>> {
    let parts: Vec<String> = path
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect();
    (parts.len() <= MAX_SUBFOLDER_DEPTH).then_some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// root (1), My Item (2), Received Items (3, type 50), Trash (4, type 14),
    /// a Library folder (5).
    fn inventory() -> Inventory {
        let mut inv = Inventory {
            root: id(1),
            ..Default::default()
        };
        inv.folders.insert(id(1), folder(1, Uuid::nil(), -1, false));
        for (n, parent, kind, library) in [
            (2, id(1), -1, false),
            (3, id(1), 50, false),
            (4, id(1), 14, false),
            (5, id(1), -1, true),
        ] {
            inv.folders.insert(id(n), folder(n, parent, kind, library));
        }
        inv.folders.get_mut(&id(1)).expect("root").children = vec![id(2), id(3), id(4), id(5)];
        inv
    }

    fn folder(n: u128, parent: Uuid, kind: i32, library: bool) -> crate::world::inventory::Folder {
        crate::world::inventory::Folder {
            info: aurora_net::inventory::InvFolder {
                id: id(n),
                parent,
                name: format!("folder {n}"),
                type_default: kind,
                version: 1,
                favorite: false,
                ..Default::default()
            },
            children: Vec::new(),
            items: Vec::new(),
            state: crate::world::inventory::FetchState::Fetched,
            library,
        }
    }

    /// The inventory's destination rule, plus Received Items: an ordinary
    /// folder and Received Items (with its folders) are usable; My Outfits, an
    /// outfit in it, the Current Outfit, the Trash and the Library are not.
    #[test]
    fn a_usable_folder_follows_the_inventory_rule_and_allows_received_items() {
        let mut inv = inventory();
        // My Outfits (6, type 48) holding an outfit (7), the Current Outfit
        // (8, type 46), a folder in Received Items (9), a folder in My Item (10)
        for (n, parent, kind) in [(6, id(1), 48), (7, id(6), -1), (8, id(1), 46), (9, id(3), -1), (10, id(2), -1)] {
            inv.folders.insert(id(n), folder(n, parent, kind, false));
        }
        inv.folders.get_mut(&id(1)).expect("root").children.extend([id(6), id(8)]);
        inv.folders.get_mut(&id(6)).expect("My Outfits").children.push(id(7));
        inv.folders.get_mut(&id(3)).expect("Received Items").children.push(id(9));
        inv.folders.get_mut(&id(2)).expect("My Item").children.push(id(10));
        for (n, expected, what) in [
            (2, true, "an ordinary folder"),
            (10, true, "a folder inside it"),
            (3, true, "Received Items"),
            (9, true, "a folder in Received Items"),
            (6, false, "My Outfits"),
            (7, false, "an outfit in My Outfits"),
            (8, false, "the Current Outfit"),
            (4, false, "the Trash"),
            (5, false, "the Library"),
            (99, false, "a folder that does not exist"),
        ] {
            assert_eq!(usable(&inv, id(n)), expected, "{what}");
        }
    }

    #[test]
    fn default_keeps_the_usual_place_whatever_else_is_known() {
        let inv = inventory();
        assert_eq!(parent(Destination::Default, Some(id(2)), &inv), None);
        assert_eq!(parent(Destination::Default, None, &inv), None);
    }

    #[test]
    fn the_object_folder_is_the_destination_when_it_is_usable() {
        let inv = inventory();
        assert_eq!(parent(Destination::ObjectFolder, Some(id(2)), &inv), Some(id(2)));
        assert_eq!(object_folder(Some(id(2)), &inv), Some(id(2)));
    }

    #[test]
    fn an_unknown_trashed_or_library_object_folder_falls_back() {
        let inv = inventory();
        for source in [None, Some(id(9)), Some(id(4)), Some(id(5))] {
            assert_eq!(parent(Destination::ObjectFolder, source, &inv), None, "{source:?}");
            assert_eq!(object_folder(source, &inv), None, "{source:?}");
        }
    }

    #[test]
    fn received_items_is_the_inbox_and_falls_back_when_missing() {
        let mut inv = inventory();
        assert_eq!(parent(Destination::ReceivedItems, Some(id(2)), &inv), Some(id(3)));
        inv.folders.remove(&id(3));
        assert_eq!(parent(Destination::ReceivedItems, Some(id(2)), &inv), None);
    }

    #[test]
    fn the_checkbox_off_means_the_usual_place() {
        let on = AcceptIn {
            on: true,
            choice: Destination::ReceivedItems,
        };
        assert_eq!(on.requested(), Destination::ReceivedItems);
        let off = AcceptIn { on: false, ..on };
        assert_eq!(off.requested(), Destination::Default);
        assert_eq!(AcceptIn::default().requested(), Destination::Default);
    }

    #[test]
    fn subfolder_tokens_expand_from_the_date_and_the_region() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).expect("date");
        assert_eq!(expand("%yyyy/%mm/%dd", date, "Ahern"), "2026/01/05");
        assert_eq!(expand("%yy-%mmm", date, "Ahern"), "26-January");
        assert_eq!(expand("%date", date, "Ahern"), "2026-01-05");
        assert_eq!(expand("Trips %region", date, "Ahern"), "Trips Ahern");
        assert_eq!(expand("plain", date, "Ahern"), "plain");
    }

    #[test]
    fn a_saved_destination_round_trips_and_missing_fields_default() {
        let saved = Saved {
            id: id(70),
            name: "Work".into(),
            folder: Uuid::from_u128(7),
            subfolder: "%yyyy/%mm".into(),
        };
        let text = serde_json::to_string(&saved).expect("json");
        assert_eq!(serde_json::from_str::<Saved>(&text).expect("back"), saved);
        assert_eq!(serde_json::from_str::<Saved>("{}").expect("empty"), Saved::default());
    }

    fn entry(n: u128, name: &str, folder: Uuid) -> Saved {
        Saved {
            id: id(n),
            name: name.into(),
            folder,
            subfolder: String::new(),
        }
    }

    #[test]
    fn a_saved_destination_files_in_its_folder_or_falls_back() {
        let mut inv = inventory();
        let saved = vec![entry(20, "Usable", id(2)), entry(21, "Trashed", id(4))];
        assert_eq!(destination_folder(Destination::Saved(id(20)), None, &inv, &saved), Some(id(2)));
        assert_eq!(destination_folder(Destination::Saved(id(21)), None, &inv, &saved), None, "in Trash");
        assert_eq!(
            destination_folder(Destination::Saved(id(25)), None, &inv, &saved),
            None,
            "no such entry"
        );
        assert_eq!(destination_folder(Destination::Default, None, &inv, &saved), None);
        inv.folders.remove(&id(2));
        assert_eq!(destination_folder(Destination::Saved(id(20)), None, &inv, &saved), None, "gone");
    }

    /// A choice names its entry, not a place in the list: removing the entry
    /// before it changes nothing, and removing the entry itself falls back.
    #[test]
    fn a_saved_choice_follows_its_entry_when_the_list_changes() {
        let inv = inventory();
        let mut saved = vec![entry(20, "First", id(3)), entry(21, "Second", id(2))];
        let choice = Destination::Saved(id(21));
        assert_eq!(destination_folder(choice, None, &inv, &saved), Some(id(2)));
        saved.remove(0);
        assert_eq!(
            destination_folder(choice, None, &inv, &saved),
            Some(id(2)),
            "still the second entry"
        );
        saved.clear();
        assert_eq!(destination_folder(choice, None, &inv, &saved), None, "removed: the usual place");
    }

    /// A choice the preferences cannot read (the older index form, a newer
    /// variant) is the usual place, and the rest of the choice is kept.
    #[test]
    fn an_unreadable_choice_in_the_preferences_is_the_usual_place() {
        let old: AcceptIn = serde_json::from_str(r#"{"on":true,"choice":{"saved":0}}"#).expect("old form");
        assert_eq!(
            old,
            AcceptIn {
                on: true,
                choice: Destination::Default
            }
        );
        let unknown: AcceptIn = serde_json::from_str(r#"{"on":true,"choice":"somewhere_new"}"#).expect("unknown");
        assert_eq!(unknown.choice, Destination::Default);
        let saved = AcceptIn {
            on: true,
            choice: Destination::Saved(id(20)),
        };
        let text = serde_json::to_string(&saved).expect("json");
        assert_eq!(serde_json::from_str::<AcceptIn>(&text).expect("back"), saved);
        let plain: AcceptIn = serde_json::from_str(r#"{"on":false,"choice":"received_items"}"#).expect("plain");
        assert_eq!(plain.choice, Destination::ReceivedItems);
    }

    #[test]
    fn an_offer_starts_on_the_object_folder_only_with_auto_select() {
        let received = Destination::ReceivedItems;
        assert_eq!(offer_start(received, true, true), Destination::ObjectFolder);
        assert_eq!(offer_start(received, true, false), received, "object folder not known yet");
        assert_eq!(offer_start(received, false, true), received, "auto-select off: the last choice");
        assert_eq!(offer_start(Destination::ObjectFolder, false, true), Destination::Default);
        assert_eq!(offer_start(Destination::Saved(id(20)), false, false), Destination::Saved(id(20)));
    }

    #[test]
    fn a_subfolder_path_is_split_and_limited_to_three_levels() {
        assert_eq!(subfolder_parts(" a / b //c/ "), Some(vec!["a".into(), "b".into(), "c".into()]));
        assert_eq!(subfolder_parts(""), Some(Vec::new()));
        assert_eq!(subfolder_parts("a/b/c/d"), None);
    }
}
