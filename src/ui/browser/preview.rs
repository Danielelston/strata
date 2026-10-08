// SPDX-License-Identifier: MIT

use super::columns::ColumnSpan;
use super::*;

#[cfg(test)]
mod tests;

impl ViewState {
    pub(super) fn column_span(&self, depth: usize) -> Option<ColumnSpan> {
        let columns = self.columns.borrow();
        let column = columns.get(depth)?;
        let left = columns[..depth]
            .iter()
            .map(column_width)
            .fold(0, i32::saturating_add);
        Some(ColumnSpan {
            left: f64::from(left),
            right: f64::from(left.saturating_add(column_width(column))),
            trailing: f64::from(self.columns_width(depth.saturating_add(1)..)),
        })
    }

    fn focused_column_span(&self) -> Option<ColumnSpan> {
        let count = self.columns.borrow().len();
        let depth = self
            .browser
            .active_depth()
            .filter(|depth| *depth < count)
            .or_else(|| count.checked_sub(1))?;
        self.column_span(depth)
    }

    fn columns_width(
        &self,
        range: impl std::slice::SliceIndex<[ColumnView], Output = [ColumnView]>,
    ) -> i32 {
        self.columns.borrow().get(range).map_or(0, |columns| {
            columns
                .iter()
                .map(column_width)
                .fold(0, i32::saturating_add)
        })
    }
}

fn column_width(column: &ColumnView) -> i32 {
    column
        .shell
        .width()
        .max(column.shell.width_request())
        .max(COLUMN_WIDTH)
}

impl BrowserView {
    pub(in crate::ui) fn is_resizing_columns(&self) -> bool {
        self.state.column_resizing.get()
    }

    /// Keeps the focused column on screen as the preview docks and shrinks the
    /// viewport, so the reduced strip still shows the column being navigated.
    pub(in crate::ui) fn bind_preview_scrolling(&self) {
        let weak = self.downgrade();
        let last_page_size = Cell::new(self.state.scroller.hadjustment().page_size());
        self.state
            .scroller
            .hadjustment()
            .connect_changed(move |adjustment| {
                let page_changed =
                    last_page_size.replace(adjustment.page_size()) != adjustment.page_size();
                if !page_changed {
                    return;
                }
                let weak = weak.clone();
                // GtkViewport must finish allocating before its scroll value is changed.
                glib::idle_add_local_once(move || {
                    let Some(view) = weak.upgrade() else { return };
                    if view.view_mode() != BrowserMode::Columns {
                        return;
                    }
                    let adjustment = view.state.scroller.hadjustment();
                    let Some(span) = view.state.focused_column_span() else {
                        return;
                    };
                    let target = span.reveal_target(
                        adjustment.value(),
                        adjustment.page_size(),
                        adjustment.lower(),
                        adjustment.upper(),
                    );
                    if target != adjustment.value() {
                        adjustment.set_value(target);
                    }
                });
            });
    }
}
