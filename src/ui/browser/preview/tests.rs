// SPDX-License-Identifier: MIT

//! Regression: docking the preview shrinks the column viewport, and the focused
//! column must still be revealed as the available width changes.

use super::*;
use crate::ui::browser::BrowserView;
use crate::ui::browser_modes::BrowserMode;
use crate::ui::preferences::PreferenceManager;
use std::time::Instant;

fn pump_until(condition: impl Fn() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn pump_for(duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn populated_columns_view(
    depths: usize,
    window_width: i32,
) -> (BrowserView, gtk::Window, tempfile::TempDir) {
    PreferenceManager::seed_saved_preferences_for_test();
    crate::ui::motion::set_reduce_motion(true);
    let root = tempfile::tempdir().expect("preview fixture");
    let mut path = root.path().to_path_buf();
    let mut locations = Vec::new();
    for depth in 0..depths {
        path = path.join(format!("level-{depth}"));
        std::fs::create_dir_all(&path).expect("column directory");
        std::fs::write(path.join("note.txt"), b"note").expect("column entry");
        locations.push(Location::local(&path));
    }
    let view = BrowserView::new(
        Rc::new(crate::adapters::LocalFileSource),
        crate::ui::browser::PeekBehavior::default(),
    );
    view.set_view_mode(BrowserMode::Columns);
    let window = gtk::Window::builder()
        .default_width(window_width)
        .default_height(650)
        .child(&view.widget())
        .build();
    window.present();
    view.browser().navigate(Location::local(root.path()));
    pump_until(
        || {
            view.browser()
                .column_snapshot(0)
                .is_some_and(|column| !column.loading)
        },
        "root column load",
    );
    for (depth, location) in locations.iter().take(depths.saturating_sub(1)).enumerate() {
        view.browser().descend(depth, location.clone());
        pump_until(
            || {
                view.browser()
                    .column_snapshot(depth + 1)
                    .is_some_and(|column| !column.loading)
            },
            "descended column load",
        );
    }
    pump_for(Duration::from_millis(40));
    (view, window, root)
}

#[test]
fn focused_column_is_revealed_when_the_viewport_shrinks() {
    crate::test_support::gtk_test(
        "ui::browser::preview::tests::focused_column_is_revealed_when_the_viewport_shrinks",
        || {
            let (view, window, _root) = populated_columns_view(6, 700);
            view.bind_preview_scrolling();
            let adjustment = view.state.scroller.hadjustment();
            pump_until(|| adjustment.page_size() > 0.0, "viewport sizing");
            view.browser().set_active_column(0);
            adjustment.set_value(adjustment.upper() - adjustment.page_size());
            pump_for(Duration::from_millis(40));
            let scrolled = adjustment.value();
            assert!(
                scrolled > 0.0,
                "fixture starts scrolled away from the first column"
            );

            // A shrinking viewport (as when the preview docks) must scroll the
            // focused first column back into view.
            adjustment.set_page_size((adjustment.page_size() - 120.0).max(60.0));
            adjustment.emit_by_name::<()>("changed", &[]);
            pump_until(
                || adjustment.value() < scrolled,
                "focused column revealed as the viewport shrinks",
            );

            PreferenceManager::shared().release_bindings_within(&view.widget());
            view.browser().clear_observer();
            window.destroy();
        },
    );
}
