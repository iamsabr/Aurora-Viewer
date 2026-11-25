//! The folder picker of the "Accept in" Browse button (Catznip's "PICK: FOLDER"
//! window): the inventory as a tree, a search field, and Select, Cancel and
//! Close All Folders. The rows are a pure function (`rows`), tested alone and
//! rebuilt only when the inventory, the search or the open folders change; the
//! window (`show`) draws the visible ones.

use crate::theme::Palette;
use crate::ui::widgets::{self, Floater};
use crate::world::contents_destination;
use crate::world::inventory::{Folder, Inventory};
use std::collections::HashSet;
use uuid::Uuid;

/// The picker's state while it is open.
#[derive(Default)]
pub struct Picker {
    /// The folders whose children are shown.
    pub expanded: HashSet<Uuid>,
    pub filter: String,
    pub selected: Option<Uuid>,
    /// The rows last built, with what they were built from: the inventory's
    /// generation, the search, and the open folders' revision.
    cache: Option<RowCache>,
    /// Bumped whenever a folder is opened or closed.
    expanded_revision: u64,
}

struct RowCache {
    generation: u64,
    filter: String,
    expanded_revision: u64,
    rows: Vec<Row>,
}

impl Picker {
    /// Build the rows again when what they show has changed.
    fn refresh_rows(&mut self, inv: &Inventory) {
        let fresh = self.cache.as_ref().is_some_and(|cache| {
            cache.generation == inv.generation && cache.filter == self.filter && cache.expanded_revision == self.expanded_revision
        });
        if !fresh {
            self.cache = Some(RowCache {
                generation: inv.generation,
                filter: self.filter.clone(),
                expanded_revision: self.expanded_revision,
                rows: rows(inv, &self.expanded, &self.filter),
            });
        }
    }

    /// The rows last built.
    fn cached_rows(&self) -> &[Row] {
        self.cache.as_ref().map_or(&[], |cache| cache.rows.as_slice())
    }

    fn toggle(&mut self, folder: Uuid) {
        if !self.expanded.remove(&folder) {
            self.expanded.insert(folder);
        }
        self.expanded_revision += 1;
    }

    fn close_all(&mut self) {
        self.expanded.clear();
        self.expanded_revision += 1;
    }
}

/// What the picker did this frame.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Open,
    Select(Uuid),
    Cancel,
}

/// One row of the tree: how deep it sits, the folder, its name, and whether it
/// has folders under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub depth: usize,
    pub folder: Uuid,
    pub name: String,
    pub expandable: bool,
}

/// The folders a destination may be picked from: those the "Accept in"
/// drop-down files in (`contents_destination::usable`), so that a picked
/// folder never disappears from it.
fn shown(inv: &Inventory, id: Uuid) -> bool {
    contents_destination::usable(inv, id)
}

fn by_name(a: &&Folder, b: &&Folder) -> std::cmp::Ordering {
    a.info
        .name
        .to_lowercase()
        .cmp(&b.info.name.to_lowercase())
        .then(a.info.id.cmp(&b.info.id))
}

/// The rows of the tree: the inventory's folders, alphabetical, the children of
/// the folders in `expanded` shown under them. A search lists every folder whose
/// name holds `filter`, flat.
pub fn rows(inv: &Inventory, expanded: &HashSet<Uuid>, filter: &str) -> Vec<Row> {
    let needle = filter.trim().to_lowercase();
    let mut out = Vec::new();
    if !needle.is_empty() {
        let mut found: Vec<&Folder> = inv
            .folders
            .values()
            .filter(|folder| folder.info.name.to_lowercase().contains(&needle) && shown(inv, folder.info.id))
            .collect();
        found.sort_by(by_name);
        out.extend(found.into_iter().map(|folder| Row {
            depth: 0,
            folder: folder.info.id,
            name: folder.info.name.clone(),
            expandable: false,
        }));
        return out;
    }
    push_children(inv, inv.root, 0, expanded, &mut out);
    out
}

fn push_children(inv: &Inventory, parent: Uuid, depth: usize, expanded: &HashSet<Uuid>, out: &mut Vec<Row>) {
    let Some(folder) = inv.folders.get(&parent) else {
        return;
    };
    let mut children: Vec<&Folder> = folder
        .children
        .iter()
        .filter(|id| shown(inv, **id))
        .filter_map(|id| inv.folders.get(id))
        .collect();
    children.sort_by(by_name);
    for child in children {
        let id = child.info.id;
        let expandable = child.children.iter().any(|c| shown(inv, *c));
        out.push(Row {
            depth,
            folder: id,
            name: child.info.name.clone(),
            expandable,
        });
        if expandable && expanded.contains(&id) {
            push_children(inv, id, depth + 1, expanded, out);
        }
    }
}

/// The height of one row of the tree.
const ROW_HEIGHT: f32 = 22.0;
/// The side of a row's open/close caret.
const CARET: f32 = 12.0;

/// Draw the picker. `Select` gives the picked folder; `Cancel`, or closing the
/// window, gives up.
pub fn show(ctx: &egui::Context, p: &Palette, inv: &Inventory, picker: &mut Picker) -> Outcome {
    let mut open = true;
    let mut outcome = Outcome::Open;
    let center = ctx.content_rect().center();
    Floater::new(
        "accept_in_picker",
        aurora_i18n::tr!("accept-in-picker-title"),
        egui::pos2(center.x - 180.0, center.y - 200.0),
        egui::Vec2::new(360.0, 400.0),
    )
    .show(ctx, p, &mut open, |ui| {
        let width = ui.available_width();
        widgets::search_field(ui, p, &mut picker.filter, aurora_i18n::tr!("accept-in-picker-search"), width);
        widgets::divider(ui, p);
        picker.refresh_rows(inv);
        let mut toggled = None;
        let mut picked = None;
        let list = picker.cached_rows();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(260.0)
            // only the visible rows are laid out, however large the inventory
            .show_rows(ui, ROW_HEIGHT, list.len(), |ui, range| {
                for row in &list[range] {
                    let size = egui::Vec2::new(ui.available_width(), ROW_HEIGHT);
                    ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add_space(row.depth as f32 * 14.0);
                        // the folder's caret opens and closes it, as the
                        // inventory's tree does (`disclosure_caret`)
                        let (caret, response) = ui.allocate_exact_size(egui::Vec2::splat(CARET + 6.0), egui::Sense::click());
                        if row.expandable {
                            let open = picker.expanded.contains(&row.folder);
                            let color = if response.hovered() { p.ink } else { p.muted };
                            widgets::paint_glyph(ui.painter(), widgets::disclosure_caret(open), caret.center(), CARET, color);
                            if response.clicked() {
                                toggled = Some(row.folder);
                            }
                        }
                        let selected = picker.selected == Some(row.folder);
                        if ui.selectable_label(selected, row.name.as_str()).clicked() {
                            picked = Some(row.folder);
                        }
                    });
                }
            });
        if let Some(folder) = picked {
            picker.selected = Some(folder);
        }
        if let Some(folder) = toggled {
            picker.toggle(folder);
        }
        widgets::divider(ui, p);
        ui.horizontal(|ui| {
            // Select needs a folder picked first
            ui.add_enabled_ui(picker.selected.is_some(), |ui| {
                let select = widgets::primary_button_sized(ui, p, aurora_i18n::tr!("accept-in-picker-select"), egui::Vec2::new(96.0, 26.0));
                if select.clicked()
                    && let Some(folder) = picker.selected
                {
                    outcome = Outcome::Select(folder);
                }
            });
            if widgets::flat_button(ui, p, aurora_i18n::tr!("accept-in-picker-cancel")).clicked() {
                outcome = Outcome::Cancel;
            }
            if widgets::flat_button(ui, p, aurora_i18n::tr!("accept-in-picker-close-all")).clicked() {
                picker.close_all();
            }
        });
    });
    if !open && outcome == Outcome::Open {
        outcome = Outcome::Cancel;
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::inventory::FetchState;
    use aurora_net::inventory::InvFolder;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn folder(n: u128, parent: Uuid, name: &str, kind: i32, library: bool) -> Folder {
        Folder {
            info: InvFolder {
                id: id(n),
                parent,
                name: name.into(),
                type_default: kind,
                ..Default::default()
            },
            children: Vec::new(),
            items: Vec::new(),
            state: FetchState::Fetched,
            library,
        }
    }

    /// root (1) with "beta" (2, holding "delta" 5), "Alpha" (3), Trash (4), a
    /// Library folder (6) and the Current Outfit (7).
    fn inventory() -> Inventory {
        let mut inv = Inventory {
            root: id(1),
            ..Default::default()
        };
        inv.folders.insert(id(1), folder(1, Uuid::nil(), "root", -1, false));
        inv.folders.insert(id(2), folder(2, id(1), "beta", -1, false));
        inv.folders.insert(id(3), folder(3, id(1), "Alpha", -1, false));
        inv.folders.insert(id(4), folder(4, id(1), "Trash", 14, false));
        inv.folders.insert(id(5), folder(5, id(2), "delta", -1, false));
        inv.folders.insert(id(6), folder(6, id(1), "Library", -1, true));
        inv.folders.insert(id(7), folder(7, id(1), "Current Outfit", 46, false));
        inv.folders.get_mut(&id(1)).expect("root").children = vec![id(2), id(3), id(4), id(6), id(7)];
        inv.folders.get_mut(&id(2)).expect("beta").children = vec![id(5)];
        inv
    }

    #[test]
    fn the_tree_is_alphabetical_and_opens_only_what_is_expanded() {
        let inv = inventory();
        let closed = rows(&inv, &HashSet::new(), "");
        let names: Vec<&str> = closed.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "beta"], "no Trash, no Library, no Current Outfit");
        assert!(closed[1].expandable && !closed[0].expandable);
        let open = rows(&inv, &HashSet::from([id(2)]), "");
        assert_eq!(open[1].name, "beta");
        assert_eq!((open[2].name.as_str(), open[2].depth), ("delta", 1));
    }

    #[test]
    fn a_search_lists_the_matches_flat() {
        let inv = inventory();
        let found = rows(&inv, &HashSet::new(), "ALP");
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].name.as_str(), found[0].depth), ("Alpha", 0));
        assert!(rows(&inv, &HashSet::new(), "trash").is_empty(), "Trash is not offered");
        assert!(
            rows(&inv, &HashSet::new(), "outfit").is_empty(),
            "the Current Outfit is not offered"
        );
    }

    /// The rows are built again when a folder opens, the search changes or the
    /// inventory changes, and not otherwise.
    #[test]
    fn the_rows_are_rebuilt_only_when_they_change() {
        let mut inv = inventory();
        let mut picker = Picker::default();
        let count = |picker: &mut Picker, inv: &Inventory| {
            picker.refresh_rows(inv);
            picker.cached_rows().len()
        };
        assert_eq!(count(&mut picker, &inv), 2);
        picker.toggle(id(2));
        assert_eq!(count(&mut picker, &inv), 3, "beta opened");
        picker.filter = "delta".into();
        assert_eq!(count(&mut picker, &inv), 1, "searched");
        picker.filter.clear();
        assert_eq!(count(&mut picker, &inv), 3, "search cleared");
        inv.folders.insert(id(8), folder(8, id(1), "gamma", -1, false));
        inv.folders.get_mut(&id(1)).expect("root").children.push(id(8));
        assert_eq!(count(&mut picker, &inv), 3, "same generation: the cached rows");
        inv.generation += 1;
        assert_eq!(count(&mut picker, &inv), 4, "a new generation: rebuilt");
        picker.close_all();
        assert_eq!(count(&mut picker, &inv), 3);
    }
}
