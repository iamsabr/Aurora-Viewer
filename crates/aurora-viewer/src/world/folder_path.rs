//! The subfolder path of an "Accept in" destination (Catznip's destination
//! subfolders): each level is looked for under the one before and made only
//! when it is missing, one request at a time, so that a dated path such as
//! `%yyyy/%mm` keeps filing into the same folders. The last level is where the
//! item goes. The caller sends each level's change and gives the folder the
//! reply names back to the walk.

use crate::world::inventory::{Inventory, actions};
use aurora_net::inventory::operations::Mutation;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// How long one level may wait for the server's reply before the walk gives up
/// (the same wait as the copy's own folder, `COPY_TIMEOUT`).
pub const LEVEL_TIMEOUT: Duration = Duration::from_secs(60);

/// The walk down a subfolder path.
#[derive(Debug)]
pub struct PathWalk {
    levels: VecDeque<String>,
    /// The folder the next level goes into: the destination at first, then
    /// each level found or made.
    parent: Uuid,
    /// The id asked for the level in progress, the reply names the folder made.
    asked: Option<Uuid>,
    /// When the level in progress was asked for.
    sent: Option<Instant>,
}

impl PathWalk {
    /// A walk from `parent` down the folder names `levels`.
    pub fn new(parent: Uuid, levels: Vec<String>) -> Self {
        Self {
            levels: levels.into(),
            parent,
            asked: None,
            sent: None,
        }
    }

    /// The change that makes the next missing level, or None when every level
    /// is there. A level that already exists under its parent (same name, as
    /// `create_folder` would write it) is walked into without a request. A
    /// name the viewer cannot use is refused.
    pub fn next_level(&mut self, inv: &Inventory) -> Result<Option<Mutation>, String> {
        while let Some(name) = self.levels.pop_front() {
            let wanted = actions::new_folder_name(&name)?;
            if let Some(existing) = existing_child(inv, self.parent, &wanted) {
                self.parent = existing;
                continue;
            }
            let change = actions::create_folder(self.parent, &name)?;
            self.asked = match change.operations.first() {
                Some(aurora_net::inventory::operations::Operation::Folder { id, .. }) => Some(*id),
                _ => return Err("the folder change has no folder".into()),
            };
            self.sent = Some(Instant::now());
            return Ok(Some(change));
        }
        Ok(None)
    }

    /// The reply for the level in progress named `folder` (None when it was not
    /// made): the walk goes on from it, and a level that failed ends the walk.
    pub fn made(&mut self, folder: Option<Uuid>) -> Result<(), String> {
        self.asked = None;
        self.sent = None;
        match folder {
            Some(folder) => {
                self.parent = folder;
                Ok(())
            }
            None => Err("the folder was not made".into()),
        }
    }

    /// The id asked for the level in progress: the reply's `created` map names
    /// the folder made under it.
    pub fn asked(&self) -> Option<Uuid> {
        self.asked
    }

    /// The folder the walk has reached: the destination of the item once every
    /// level is there.
    pub fn folder(&self) -> Uuid {
        self.parent
    }

    /// Whether the level in progress has waited longer than `LEVEL_TIMEOUT`.
    pub fn timed_out(&self, now: Instant) -> bool {
        self.sent.is_some_and(|sent| now.saturating_duration_since(sent) >= LEVEL_TIMEOUT)
    }
}

/// A folder named `name` directly under `parent`, the first one when there are
/// several (the inventory allows two folders of the same name).
fn existing_child(inv: &Inventory, parent: Uuid, name: &str) -> Option<Uuid> {
    inv.folders
        .get(&parent)?
        .children
        .iter()
        .filter_map(|id| inv.folders.get(id))
        .find(|folder| !folder.library && folder.info.name == name)
        .map(|folder| folder.info.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::inventory::{FetchState, Folder};
    use aurora_net::inventory::InvFolder;
    use aurora_net::inventory::operations::Operation;

    fn parent_of(change: &Mutation) -> Uuid {
        match change.operations.first() {
            Some(Operation::Folder { parent, .. }) => *parent,
            other => panic!("not a folder: {other:?}"),
        }
    }

    fn folder(id: Uuid, parent: Uuid, name: &str) -> Folder {
        Folder {
            info: InvFolder {
                id,
                parent,
                name: name.into(),
                type_default: -1,
                ..Default::default()
            },
            children: Vec::new(),
            items: Vec::new(),
            state: FetchState::Fetched,
            library: false,
        }
    }

    /// An inventory with `dest` and, under it, the folders `children`.
    fn inventory(dest: Uuid, children: &[(Uuid, &str)]) -> Inventory {
        let mut inv = Inventory {
            root: dest,
            ..Default::default()
        };
        let mut top = folder(dest, Uuid::nil(), "dest");
        for (id, name) in children {
            top.children.push(*id);
            inv.folders.insert(*id, folder(*id, dest, name));
        }
        inv.folders.insert(dest, top);
        inv
    }

    #[test]
    fn each_level_is_made_under_the_one_before() {
        let (dest, made_a, made_b) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let inv = inventory(dest, &[]);
        let mut walk = PathWalk::new(dest, vec!["2026".into(), "January".into()]);
        let first = walk.next_level(&inv).expect("first").expect("a level");
        assert_eq!(parent_of(&first), dest);
        walk.made(Some(made_a)).expect("made");
        let second = walk.next_level(&inv).expect("second").expect("a level");
        assert_eq!(parent_of(&second), made_a);
        walk.made(Some(made_b)).expect("made");
        assert!(walk.next_level(&inv).expect("done").is_none());
        assert_eq!(walk.folder(), made_b);
    }

    /// A dated path files into the folders an earlier accept made: an existing
    /// level is walked into, and only the missing one is asked for.
    #[test]
    fn an_existing_level_is_reused_not_made_again() {
        let (dest, year, made_month) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let inv = inventory(dest, &[(year, "2026")]);
        let mut walk = PathWalk::new(dest, vec!["2026".into(), "January".into()]);
        let change = walk.next_level(&inv).expect("level").expect("the month is missing");
        assert_eq!(parent_of(&change), year, "made inside the existing year");
        walk.made(Some(made_month)).expect("made");
        assert!(walk.next_level(&inv).expect("done").is_none());
        assert_eq!(walk.folder(), made_month);
    }

    #[test]
    fn a_path_that_exists_already_needs_no_request() {
        let (dest, year) = (Uuid::new_v4(), Uuid::new_v4());
        let inv = inventory(dest, &[(year, "2026")]);
        let mut walk = PathWalk::new(dest, vec!["2026".into()]);
        assert!(walk.next_level(&inv).expect("none").is_none());
        assert_eq!(walk.folder(), year);
    }

    #[test]
    fn a_folder_of_another_name_is_not_reused() {
        let dest = Uuid::new_v4();
        let inv = inventory(dest, &[(Uuid::new_v4(), "2025")]);
        let mut walk = PathWalk::new(dest, vec!["2026".into()]);
        let change = walk.next_level(&inv).expect("level").expect("made");
        assert_eq!(parent_of(&change), dest);
    }

    #[test]
    fn no_levels_means_the_destination_itself() {
        let dest = Uuid::new_v4();
        let mut walk = PathWalk::new(dest, Vec::new());
        assert!(walk.next_level(&inventory(dest, &[])).expect("none").is_none());
        assert_eq!(walk.folder(), dest);
    }

    #[test]
    fn a_level_that_is_not_made_ends_the_walk() {
        let dest = Uuid::new_v4();
        let mut walk = PathWalk::new(dest, vec!["a".into(), "b".into()]);
        walk.next_level(&inventory(dest, &[])).expect("first");
        assert!(walk.made(None).is_err());
    }

    #[test]
    fn a_level_waits_for_its_reply_up_to_the_timeout() {
        let dest = Uuid::new_v4();
        let mut walk = PathWalk::new(dest, vec!["a".into()]);
        assert!(!walk.timed_out(Instant::now() + LEVEL_TIMEOUT), "nothing asked yet");
        walk.next_level(&inventory(dest, &[])).expect("asked");
        let now = Instant::now();
        assert!(!walk.timed_out(now));
        assert!(walk.timed_out(now + LEVEL_TIMEOUT));
    }
}
