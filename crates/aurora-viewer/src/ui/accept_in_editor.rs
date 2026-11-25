//! The editor of the "Accept in" destination folders (the wrench's window, as
//! Catznip's "ACCEPT IN DESTINATION FOLDERS"): the account's saved destinations
//! in a table, Add and Remove, the auto-select checkbox, the editor of one entry
//! (friendly name, folder with Browse, subfolder with its tokens), and Save and
//! Cancel. It works on a copy of the list, which Save keeps and Cancel drops.

use crate::theme::Palette;
use crate::ui::accept_in_picker::{self, Outcome as PickOutcome, Picker};
use crate::ui::widgets::{self, Floater};
use crate::world::contents_destination::{self, Saved};
use crate::world::inventory::Inventory;
use uuid::Uuid;

/// The editor's state while it is open.
#[derive(Default)]
pub struct Editor {
    pub open: bool,
    /// The working copy of the account's destinations.
    pub list: Vec<Saved>,
    /// The entry in the table being edited, None for a new one.
    pub selected: Option<usize>,
    pub name: String,
    pub folder: Option<Uuid>,
    pub subfolder: String,
    pub auto_select: bool,
    /// The folder picker, while Browse is open.
    pub picker: Option<Picker>,
    /// Each column's share of the table's width (name, folder, subfolder),
    /// which the heading's handles change: the table always spans the window,
    /// and its columns grow and shrink with it. Zero until the first frame.
    pub shares: [f32; 3],
    /// The height of the table's rows area, corrected each frame by what the
    /// window's content missed its height by (`next_list_height`). Zero until
    /// the first frame.
    pub list_height: f32,
}

/// The columns' shares to begin with: name, folder, then subfolder.
const DEFAULT_SHARES: [f32; 3] = [0.32, 0.38, 0.30];
/// What the table keeps for the rows under its heading, at the least.
const MIN_ROWS: f32 = 3.0;

/// The rows area's height for the next frame: this frame's height, corrected
/// by the room the content left (or took over) in the window. It never
/// overshoots: the content then fills the window exactly, so the window never
/// grows to fit it, and a window made shorter makes the list shorter, down to
/// `min`. The content is measured to the cursor, past the last widget's
/// spacing, so the correction errs on the short side.
fn next_list_height(current: f32, available: f32, used: f32, min: f32) -> f32 {
    (current + available - used).max(min)
}
/// The smallest the window may be made: room for the table's heading and a
/// few rows, and for the whole entry form under it.
const MIN_WINDOW: egui::Vec2 = egui::Vec2::new(520.0, 470.0);

/// The columns' widths for a table `total` wide.
fn column_widths(shares: &[f32; 3], total: f32) -> [f32; 3] {
    shares.map(|share| share * total)
}

/// The columns' shares of the table, from their widths (after a drag).
fn column_shares(widths: &[f32; 3]) -> [f32; 3] {
    let total = widths.iter().sum::<f32>();
    if total > 0.0 {
        widths.map(|width| width / total)
    } else {
        DEFAULT_SHARES
    }
}
/// The narrowest a column may be dragged.
const MIN_WIDTH: f32 = 60.0;
/// The height of the heading and of each row.
const ROW_HEIGHT: f32 = 22.0;
/// The space between a column's left edge and its text, in the heading and in
/// the rows alike, so that each row's text starts under its column's name.
const TEXT_PAD: f32 = 6.0;
/// The space kept clear at a column's right edge: the gap between two heading
/// cells, and where a row's text is clipped.
const COLUMN_GAP: f32 = 4.0;

/// The left and right edges of the three columns of a table whose left edge
/// is `left`. The heading and the rows both place their columns with it, so
/// that they always line up, whatever the widths.
fn column_edges(left: f32, widths: &[f32; 3]) -> [(f32, f32); 3] {
    let mut x = left;
    widths.map(|width| {
        let edges = (x, x + width);
        x += width;
        edges
    })
}

/// The table's heading: each column's name, with a handle between two columns
/// that is dragged to resize them (the total width stays the same). The whole
/// heading is one rectangle with no spacing between its cells, as the rows are.
fn table_header(ui: &mut egui::Ui, p: &Palette, widths: &mut [f32; 3], labels: &[String; 3]) {
    let total = widths.iter().sum::<f32>();
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(total, ROW_HEIGHT), egui::Sense::hover());
    let font = egui::TextStyle::Body.resolve(ui.style());
    for (index, (left, right)) in column_edges(rect.left(), widths).into_iter().enumerate() {
        let cell = egui::Rect::from_min_max(egui::pos2(left, rect.top()), egui::pos2(right - COLUMN_GAP, rect.bottom()));
        ui.painter().rect_filled(cell, 0.0, p.field);
        ui.painter().with_clip_rect(cell).text(
            egui::pos2(left + TEXT_PAD, rect.center().y),
            egui::Align2::LEFT_CENTER,
            labels[index].as_str(),
            font.clone(),
            p.muted,
        );
    }
    // the handles sit on the edges between columns, read after the widths
    // above so that a drag moves the heading and the rows in the same frame
    for index in 0..2 {
        let edge = column_edges(rect.left(), widths)[index].1;
        let handle = egui::Rect::from_min_max(egui::pos2(edge - 3.0, rect.top()), egui::pos2(edge + 3.0, rect.bottom()));
        let response = ui.interact(handle, ui.scope_id().with(("accept_in_column", index)), egui::Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if response.dragged() {
            resize(widths, index, response.drag_delta().x);
        }
    }
}

/// Move the edge after column `index` by `delta`: that column grows and the
/// next one shrinks by the same amount, neither below `MIN_WIDTH`.
fn resize(widths: &mut [f32; 3], index: usize, delta: f32) {
    let low = (MIN_WIDTH - widths[index]).min(0.0);
    let high = (widths[index + 1] - MIN_WIDTH).max(0.0);
    let grow = delta.clamp(low, high);
    widths[index] += grow;
    widths[index + 1] -= grow;
}

/// Whether a subfolder path stays within the levels a destination may have,
/// once its tokens are expanded (a date and a region add no level).
fn subfolder_fits(subfolder: &str) -> bool {
    let today = chrono::Local::now().date_naive();
    contents_destination::subfolder_parts(&contents_destination::expand(subfolder, today, "Region")).is_some()
}

/// What the editor asks of the app this frame.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// The new list and its auto-select choice, to keep for the account.
    Save(Vec<Saved>, bool),
    Close,
}

impl Editor {
    /// Open on a copy of the account's destinations.
    pub fn open(&mut self, list: &[Saved], auto_select: bool) {
        self.open = true;
        self.list = list.to_vec();
        self.auto_select = auto_select;
        self.clear_draft();
    }

    fn clear_draft(&mut self) {
        self.selected = None;
        self.name.clear();
        self.folder = None;
        self.subfolder.clear();
    }

    fn load_selected(&mut self, index: usize) {
        if let Some(entry) = self.list.get(index) {
            self.selected = Some(index);
            self.name = entry.name.clone();
            self.folder = Some(entry.folder);
            self.subfolder = entry.subfolder.clone();
        }
    }

    /// The entry in the editor, when it can be kept: a name, a folder, and a
    /// subfolder path within the level limit. It keeps the id of the entry it
    /// edits; a new entry's id is nil until it is kept.
    fn draft(&self) -> Option<Saved> {
        let name = self.name.trim();
        let folder = self.folder?;
        let subfolder = self.subfolder.trim();
        (!name.is_empty() && subfolder_fits(subfolder)).then(|| Saved {
            id: self.selected_entry().map(|entry| entry.id).unwrap_or_default(),
            name: name.to_owned(),
            folder,
            subfolder: subfolder.to_owned(),
        })
    }

    fn selected_entry(&self) -> Option<&Saved> {
        self.selected.and_then(|index| self.list.get(index))
    }

    /// Keep the entry in the editor: replace the one selected, or add it with
    /// an id of its own.
    fn keep_draft(&mut self) {
        let Some(mut entry) = self.draft() else {
            return;
        };
        match self.selected.filter(|index| *index < self.list.len()) {
            Some(index) => self.list[index] = entry,
            None => {
                entry.id = Uuid::new_v4();
                self.list.push(entry);
            }
        }
        self.clear_draft();
    }

    /// The list Save keeps: an entry filled in but not yet kept with "Save
    /// entry" is kept too, rather than lost.
    fn saved_list(&mut self) -> Vec<Saved> {
        self.keep_draft();
        self.list.clone()
    }
}

/// One row of the entry form: its label in a column `label_width` wide, then
/// the row's controls, on a row no taller than a text box.
fn form_line(ui: &mut egui::Ui, label_width: f32, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    let height = ui.spacing().interact_size.y + 4.0;
    let size = egui::Vec2::new(ui.available_width(), height);
    ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.allocate_ui_with_layout(
            egui::Vec2::new(label_width, height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(label_width);
                ui.label(label);
            },
        );
        add(ui);
    });
}

/// The name of a folder, or nothing when it is gone.
fn folder_name(inv: &Inventory, folder: Uuid) -> String {
    inv.folders.get(&folder).map(|f| f.info.name.clone()).unwrap_or_default()
}

/// Draw the editor, and say what the app should do with it.
pub fn show(ctx: &egui::Context, p: &Palette, inv: &Inventory, editor: &mut Editor) -> Action {
    let mut open = true;
    let mut action = Action::None;
    let center = ctx.content_rect().center();
    let mut floater = Floater::new(
        "accept_in_editor",
        aurora_i18n::tr!("accept-in-editor-title"),
        egui::pos2(center.x - 290.0, center.y - 280.0),
        egui::Vec2::new(580.0, 560.0),
    );
    floater.min_size = MIN_WINDOW;
    floater.show(ctx, p, &mut open, |ui| {
        // Everything below takes its width from the window as it is now, never
        // from the frame before (no Grid, no fixed widths), and no row claims
        // more height than it needs: the window shrinks as freely as it grows.
        // The table's rows take the height the rest leaves (as Catznip's list
        // does): the window's content is measured against its height at the
        // end, and the rows area corrected by the difference for the next frame.
        let content_top = ui.cursor().top();
        let window_height = ui.available_height();
        let min_list = ROW_HEIGHT * MIN_ROWS;
        if editor.shares[0] == 0.0 {
            editor.shares = DEFAULT_SHARES;
        }
        let list_height = editor.list_height.max(min_list);
        ui.label(aurora_i18n::tr!("accept-in-configured"));
        let labels = [
            aurora_i18n::tr!("accept-in-column-name").to_owned(),
            aurora_i18n::tr!("accept-in-column-path").to_owned(),
            aurora_i18n::tr!("accept-in-column-subfolder").to_owned(),
        ];
        let font = egui::TextStyle::Body.resolve(ui.style());
        widgets::surface(ui, p, |ui| {
            let mut widths = column_widths(&editor.shares, ui.available_width());
            table_header(ui, p, &mut widths, &labels);
            editor.shares = column_shares(&widths);
            egui::ScrollArea::vertical()
                .min_scrolled_height(list_height)
                .max_height(list_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut pick = None;
                    for (index, entry) in editor.list.iter().enumerate() {
                        // each text starts at its column's edge plus the
                        // heading's padding, clipped to its column
                        // (`column_edges`); the row never asks for more width
                        // than the list has, so it never widens the window
                        let folder = folder_name(inv, entry.folder);
                        let texts = [entry.name.as_str(), folder.as_str(), entry.subfolder.as_str()];
                        let row_width = widths.iter().sum::<f32>().min(ui.available_width());
                        let (row, response) = ui.allocate_exact_size(egui::Vec2::new(row_width, ROW_HEIGHT), egui::Sense::click());
                        if response.clicked() {
                            pick = Some(index);
                        }
                        if editor.selected == Some(index) {
                            ui.painter().rect_filled(row, 0.0, p.violet.gamma_multiply(0.35));
                        }
                        for ((left, right), text) in column_edges(row.left(), &widths).into_iter().zip(texts) {
                            let cell = egui::Rect::from_min_max(
                                egui::pos2(left + TEXT_PAD, row.top()),
                                egui::pos2((right - COLUMN_GAP).min(row.right()), row.bottom()),
                            );
                            ui.painter().with_clip_rect(cell).text(
                                egui::pos2(cell.left(), row.center().y),
                                egui::Align2::LEFT_CENTER,
                                text,
                                font.clone(),
                                p.ink,
                            );
                        }
                    }
                    if let Some(index) = pick {
                        editor.load_selected(index);
                    }
                });
        });
        ui.horizontal(|ui| {
            if widgets::flat_button(ui, p, aurora_i18n::tr!("accept-in-add")).clicked() {
                editor.clear_draft();
            }
            let remove = editor.selected.is_some();
            ui.add_enabled_ui(remove, |ui| {
                if widgets::flat_button(ui, p, aurora_i18n::tr!("accept-in-remove")).clicked()
                    && let Some(index) = editor.selected
                {
                    editor.list.remove(index);
                    editor.clear_draft();
                }
            });
        });
        widgets::divider(ui, p);
        widgets::checkbox(ui, p, &mut editor.auto_select, aurora_i18n::tr!("accept-in-auto-select"));
        widgets::divider(ui, p);

        // the editor of one entry: a label column as wide as its longest label,
        // and boxes that fill the rest of the row as it is now
        let mut path = editor.folder.map(|folder| folder_name(inv, folder)).unwrap_or_default();
        let form_labels = [
            aurora_i18n::tr!("accept-in-name"),
            aurora_i18n::tr!("accept-in-path"),
            aurora_i18n::tr!("accept-in-subfolder"),
        ];
        let label_width = form_labels
            .iter()
            .map(|label| ui.painter().layout_no_wrap((*label).to_owned(), font.clone(), p.ink).size().x)
            .fold(0.0, f32::max)
            + 10.0;
        form_line(ui, label_width, form_labels[0], |ui| {
            ui.add(egui::TextEdit::singleline(&mut editor.name).desired_width(ui.available_width()));
        });
        form_line(ui, label_width, form_labels[1], |ui| {
            // Browse at the row's right end, the path box filling what is left
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::flat_button(ui, p, aurora_i18n::tr!("accept-in-browse")).clicked() {
                    editor.picker = Some(Picker::default());
                }
                ui.add(
                    egui::TextEdit::singleline(&mut path)
                        .desired_width(ui.available_width())
                        .interactive(false),
                );
            });
        });
        form_line(ui, label_width, form_labels[2], |ui| {
            ui.add(egui::TextEdit::singleline(&mut editor.subfolder).desired_width(ui.available_width()));
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new(aurora_i18n::tr!("accept-in-tokens")).size(13.5));
        // a path deeper than the limit is refused here, before any accept
        let fits = subfolder_fits(editor.subfolder.trim());
        if !fits {
            ui.label(egui::RichText::new(aurora_i18n::tr!("accept-in-too-deep")).color(p.danger));
        }
        ui.horizontal(|ui| {
            let keep = editor.draft().is_some();
            ui.add_enabled_ui(keep, |ui| {
                if widgets::flat_button(ui, p, aurora_i18n::tr!("accept-in-save-entry")).clicked() {
                    editor.keep_draft();
                }
            });
        });
        widgets::divider(ui, p);
        // Save and Cancel at the bottom right, as in Catznip: a row of its own
        // height (a bare right-to-left layout would claim all the height left,
        // and the window could then never be made shorter)
        let row = egui::Vec2::new(ui.available_width(), 30.0);
        ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::flat_button(ui, p, aurora_i18n::tr!("accept-in-cancel")).clicked() {
                action = Action::Close;
            }
            // a form holding a path that is too deep is fixed before saving
            ui.add_enabled_ui(fits, |ui| {
                if widgets::primary_button_sized(ui, p, aurora_i18n::tr!("accept-in-save"), egui::Vec2::new(96.0, 26.0)).clicked() {
                    action = Action::Save(editor.saved_list(), editor.auto_select);
                }
            });
        });
        // the rows area for the next frame: corrected by what the content
        // missed the window's height by, so it fills the window without ever
        // making it grow
        let used = ui.cursor().top() - content_top;
        editor.list_height = next_list_height(list_height, window_height, used, min_list);
    });
    // the picker takes the folder for the entry
    if let Some(picker) = editor.picker.as_mut() {
        match accept_in_picker::show(ctx, p, inv, picker) {
            PickOutcome::Open => {}
            PickOutcome::Select(folder) => {
                editor.folder = Some(folder);
                editor.picker = None;
            }
            PickOutcome::Cancel => editor.picker = None,
        }
    }
    if !open && action == Action::None {
        action = Action::Close;
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(name: &str, n: u128) -> Saved {
        Saved {
            id: Uuid::from_u128(100 + n),
            name: name.into(),
            folder: Uuid::from_u128(n),
            subfolder: String::new(),
        }
    }

    #[test]
    fn an_entry_needs_a_name_and_a_folder_to_be_kept() {
        let mut editor = Editor::default();
        editor.open(&[], true);
        editor.name = "  ".into();
        editor.folder = Some(Uuid::from_u128(1));
        assert!(editor.draft().is_none(), "blank name");
        editor.name = "Work".into();
        editor.folder = None;
        assert!(editor.draft().is_none(), "no folder");
        editor.folder = Some(Uuid::from_u128(1));
        editor.subfolder = " %yyyy/%mm ".into();
        editor.keep_draft();
        assert_eq!(editor.list.len(), 1);
        let kept = &editor.list[0];
        assert_eq!(
            (kept.name.as_str(), kept.folder, kept.subfolder.as_str()),
            ("Work", Uuid::from_u128(1), "%yyyy/%mm")
        );
        assert!(!kept.id.is_nil(), "a new entry gets an id of its own");
        assert!(editor.name.is_empty(), "the draft is cleared once kept");
    }

    #[test]
    fn a_selected_entry_is_replaced_not_added_and_keeps_its_id() {
        let mut editor = Editor::default();
        editor.open(&[saved("Work", 1), saved("Home", 2)], false);
        editor.load_selected(1);
        editor.name = "House".into();
        editor.keep_draft();
        let names: Vec<&str> = editor.list.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["Work", "House"]);
        assert_eq!(editor.list[1].id, Uuid::from_u128(102), "the choices that name it still do");
        assert!(!editor.auto_select);
    }

    #[test]
    fn a_subfolder_deeper_than_three_levels_is_refused() {
        let mut editor = Editor::default();
        editor.open(&[], true);
        editor.name = "Deep".into();
        editor.folder = Some(Uuid::from_u128(1));
        editor.subfolder = "a/b/%yyyy/%mm".into();
        assert!(editor.draft().is_none());
        editor.subfolder = "a/%yyyy/%mm".into();
        assert!(editor.draft().is_some());
        assert!(subfolder_fits("%date %region"), "tokens add no level");
    }

    /// Save keeps an entry that was filled in but not kept with "Save entry".
    #[test]
    fn save_keeps_an_entry_left_in_the_form() {
        let mut editor = Editor::default();
        editor.open(&[saved("Work", 1)], true);
        editor.name = "Trips".into();
        editor.folder = Some(Uuid::from_u128(3));
        let list = editor.saved_list();
        let names: Vec<&str> = list.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["Work", "Trips"]);
        // an empty form adds nothing
        assert_eq!(editor.saved_list().len(), 2);
    }

    /// The heading and the rows share their columns' edges: each column starts
    /// where the one before ends, with no spacing between them.
    #[test]
    fn the_columns_follow_each_other_without_gaps() {
        let edges = column_edges(10.0, &[150.0, 170.0, 140.0]);
        assert_eq!(edges, [(10.0, 160.0), (160.0, 330.0), (330.0, 470.0)]);
    }

    /// The table spans the window: its columns keep their shares as the window
    /// grows or shrinks, and a drag changes the shares, not the total.
    #[test]
    fn the_columns_grow_and_shrink_with_the_window() {
        let shares = [0.25, 0.5, 0.25];
        assert_eq!(column_widths(&shares, 400.0), [100.0, 200.0, 100.0]);
        assert_eq!(column_widths(&shares, 800.0), [200.0, 400.0, 200.0]);
        let mut widths = column_widths(&shares, 400.0);
        resize(&mut widths, 0, 20.0);
        assert_eq!(column_shares(&widths), [0.3, 0.45, 0.25]);
        assert_eq!(column_shares(&[0.0; 3]), DEFAULT_SHARES, "nothing to share: the defaults");
    }

    /// The rows area settles at the height that makes the content fill the
    /// window, follows the window when it is made taller or shorter, and never
    /// asks for more than the window has (which would make it grow).
    #[test]
    fn the_rows_area_settles_on_the_window_height_without_growing_it() {
        let rest: f32 = 420.0; // everything else in the window
        let mut list: f32 = 0.0;
        for window in [600.0, 600.0, 900.0, 900.0, 500.0, 500.0, 300.0] {
            let used = rest + list.max(66.0);
            list = next_list_height(list.max(66.0), window, used, 66.0);
            assert!(rest + list <= window || list == 66.0, "{window}: {list}");
        }
        assert_eq!(list, 66.0, "a window shorter than the rest: the rows' minimum");
        assert_eq!(next_list_height(180.0, 600.0, 600.0, 66.0), 180.0, "filled exactly: no change");
        assert_eq!(next_list_height(180.0, 600.0, 610.0, 66.0), 170.0, "over by 10: 10 shorter");
    }

    #[test]
    fn a_column_edge_moves_both_columns_within_the_minimum() {
        let mut widths = [150.0, 170.0, 140.0];
        resize(&mut widths, 0, 20.0);
        assert_eq!(widths, [170.0, 150.0, 140.0]);
        resize(&mut widths, 1, 500.0);
        assert_eq!(widths, [170.0, 230.0, MIN_WIDTH], "the next column stops at the minimum");
        resize(&mut widths, 0, -500.0);
        assert_eq!(widths, [MIN_WIDTH, 340.0, MIN_WIDTH]);
        assert_eq!(widths.iter().sum::<f32>(), 460.0, "the total stays the same");
    }
}
