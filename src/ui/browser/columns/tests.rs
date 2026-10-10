// SPDX-License-Identifier: MIT

//! Behavioral regressions for the Miller-column nav animation. Every assertion reads an
//! adjustment value, a generation counter, a CSS class, or an explicit `width_request` —
//! never a computed layout measurement.

use super::*;
use crate::ui::browser::BrowserView;
use crate::ui::browser_modes::BrowserMode;
use crate::ui::preferences::PreferenceManager;

/// Forces motion on. Must run after the preference manager has loaded (creating a `BrowserView`
/// loads the exhaustive fixture, which seeds `reduce_motion = true`).
fn animations_on() {
    crate::ui::motion::set_reduce_motion(false);
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(true);
    }
}

fn animations_off() {
    crate::ui::motion::set_reduce_motion(true);
}

fn pump() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

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

struct Columns {
    view: BrowserView,
    window: gtk::Window,
    _root: tempfile::TempDir,
}

impl Columns {
    fn new(depths: usize, window_width: i32) -> Self {
        PreferenceManager::seed_saved_preferences_for_test();
        let root = tempfile::tempdir().expect("columns fixture");
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
        pump_until(|| column_loaded(&view, 0), "root column load");
        for (depth, location) in locations.iter().take(depths.saturating_sub(1)).enumerate() {
            view.browser().descend(depth, location.clone());
            pump_until(|| column_loaded(&view, depth + 1), "descended column load");
        }
        pump_for(Duration::from_millis(40));
        Self {
            view,
            window,
            _root: root,
        }
    }

    fn depth_count(&self) -> usize {
        self.view.state.columns.borrow().len()
    }

    fn shell(&self, depth: usize) -> gtk::Box {
        self.view
            .state
            .columns
            .borrow()
            .get(depth)
            .map(|column| column.shell.clone())
            .expect("column shell")
    }

    fn scroller(&self) -> gtk::ScrolledWindow {
        self.view.state.scroller.clone()
    }

    fn adjustment(&self) -> gtk::Adjustment {
        self.view.state.scroller.hadjustment()
    }
}

impl Drop for Columns {
    fn drop(&mut self) {
        PreferenceManager::shared().release_bindings_within(&self.view.widget());
        self.view.browser().clear_observer();
        self.window.destroy();
    }
}

fn column_loaded(view: &BrowserView, depth: usize) -> bool {
    view.browser()
        .column_snapshot(depth)
        .is_some_and(|column| !column.loading)
}

fn resize_gesture(scroller: &gtk::ScrolledWindow) -> gtk::GestureDrag {
    let controllers = scroller.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|index| {
            controllers
                .item(index)
                .and_then(|controller| controller.downcast::<gtk::GestureDrag>().ok())
        })
        .find(|gesture| gesture.name().as_deref() == Some("column-resize"))
        .expect("column-resize gesture")
}

fn shell_edge(scroller: &gtk::ScrolledWindow, shell: &gtk::Box) -> (f64, f64) {
    pump_until(
        || {
            shell
                .compute_bounds(scroller)
                .is_some_and(|bounds| bounds.width() > 0.0)
        },
        "column allocation",
    );
    let bounds = shell.compute_bounds(scroller).expect("column bounds");
    (
        f64::from(bounds.x() + bounds.width()) - 0.5,
        f64::from(bounds.y()) + 4.0,
    )
}

fn autofit_target(shell: &gtk::Box) -> i32 {
    shell
        .first_child()
        .and_downcast::<gtk::Overlay>()
        .and_then(|overlay| overlay.child())
        .map(|child| max_child_natural_width(&child))
        .unwrap_or(COLUMN_WIDTH)
        .max(COLUMN_WIDTH)
}

/// The `directory-column` pane box that the entry animation's CSS class targets.
fn column_pane(shell: &gtk::Box) -> gtk::Widget {
    shell
        .first_child()
        .and_downcast::<gtk::Overlay>()
        .and_then(|overlay| overlay.child())
        .expect("column pane")
}

#[test]
fn reveal_column_does_not_move_an_already_visible_active_column() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::reveal_column_does_not_move_an_already_visible_active_column",
        || {
            let fixture = Columns::new(6, 700);
            animations_off();
            fixture.view.browser().set_active_column(5);
            let adjustment = fixture.adjustment();
            pump_until(|| adjustment.page_size() > 0.0, "viewport sizing");
            adjustment.set_value(adjustment.upper() - adjustment.page_size());
            pump_for(Duration::from_millis(40));
            let before = adjustment.value();
            fixture.view.state.reveal_column(fixture.shell(5));
            pump_for(Duration::from_millis(200));
            assert_eq!(
                adjustment.value(),
                before,
                "an already-visible active column must not be dragged"
            );
        },
    );
}

#[test]
fn reveal_column_still_scrolls_a_genuinely_clipped_column() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::reveal_column_still_scrolls_a_genuinely_clipped_column",
        || {
            let fixture = Columns::new(6, 700);
            animations_off();
            let adjustment = fixture.adjustment();
            pump_until(|| adjustment.page_size() > 0.0, "viewport sizing");
            adjustment.set_value(adjustment.upper() - adjustment.page_size());
            pump_for(Duration::from_millis(20));
            fixture.view.browser().set_active_column(0);
            let before = adjustment.value();
            assert!(
                before > 0.0,
                "fixture starts scrolled away from the first column"
            );
            fixture.view.state.reveal_column(fixture.shell(0));
            pump_until(
                || adjustment.value() < before,
                "clipped column reveals by scrolling back",
            );
            assert!(
                adjustment.value() < before,
                "a genuinely clipped column must still be revealed"
            );
        },
    );
}

#[test]
fn mirror_focused_folder_forces_the_open_after_a_pointer_driven_removal() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::mirror_focused_folder_forces_the_open_after_a_pointer_driven_removal",
        || {
            // Just the root column — no child yet, so depth 1 only exists
            // once something (the fix under test) actually opens it.
            let fixture = Columns::new(1, 900);
            animations_off();
            fixture.view.set_columns_mirror_selection(true);
            // A directory sibling under the fixture's root column (depth 0),
            // standing in for whatever entry a deletion would land focus on.
            let root = fixture._root.path();
            let sibling = root.join("sibling-after-delete");
            std::fs::create_dir_all(&sibling).expect("sibling directory");
            fixture.view.browser().retry_column(0);
            pump_until(|| column_loaded(&fixture.view, 0), "root column reload");
            let position = fixture
                .view
                .browser()
                .with_column_entries(0, |entries| {
                    entries
                        .iter()
                        .position(|entry| entry.location == Location::local(&sibling))
                })
                .flatten()
                .expect("sibling entry present in the loaded column");

            // Mouse was the last input, which the ordinary gate would block.
            fixture
                .view
                .state
                .input_ownership
                .borrow_mut()
                .pointer_action();
            fixture.view.browser().select(0, position);
            pump_for(Duration::from_millis(20));
            assert!(
                fixture.view.state.columns.borrow().get(1).is_none(),
                "sanity: the ordinary pointer-gated path must not have opened it"
            );

            fixture
                .view
                .state
                .mirror_focused_folder(0, Some(position), true);
            pump_until(
                || column_loaded(&fixture.view, 1),
                "forced child column load",
            );
            assert!(
                fixture.view.state.columns.borrow().get(1).is_some(),
                "a removal-driven focus change must open the newly selected folder \
                 even though the last navigation input was the pointer"
            );
        },
    );
}

#[test]
fn rapid_reveals_leave_no_orphaned_tick() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::rapid_reveals_leave_no_orphaned_tick",
        || {
            let fixture = Columns::new(6, 700);
            animations_off();
            let shells: Vec<gtk::Box> = (0..fixture.depth_count())
                .map(|depth| fixture.shell(depth))
                .collect();
            for step in 0..12 {
                fixture
                    .view
                    .state
                    .reveal_column(shells[step % shells.len()].clone());
                for _ in 0..3 {
                    pump();
                }
            }
            pump_for(Duration::from_millis(120));
            pump_for(Duration::from_millis(120));
            let generation = fixture.view.state.horizontal_scroll_generation.get();
            pump_for(Duration::from_millis(50));
            assert_eq!(
                fixture.view.state.horizontal_scroll_generation.get(),
                generation,
                "no orphaned tick keeps bumping the horizontal scroll generation"
            );
        },
    );
}

#[test]
fn column_entry_animation_timing_and_class_unchanged() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::column_entry_animation_timing_and_class_unchanged",
        || {
            PreferenceManager::seed_saved_preferences_for_test();
            let root = tempfile::tempdir().expect("entry fixture");
            let view = BrowserView::new(
                Rc::new(crate::adapters::LocalFileSource),
                crate::ui::browser::PeekBehavior::default(),
            );
            view.set_view_mode(BrowserMode::Columns);
            animations_on();
            let window = gtk::Window::builder()
                .default_width(900)
                .default_height(650)
                .child(&view.widget())
                .build();
            window.present();
            view.browser().navigate(Location::local(root.path()));
            let shell = view
                .state
                .columns
                .borrow()
                .first()
                .map(|column| column.shell.clone())
                .expect("column shell");
            let pane = column_pane(&shell);
            assert!(
                pane.has_css_class("column-entering"),
                "the entry class is applied immediately"
            );
            pump_for(COLUMN_TRANSITION / 2);
            assert!(
                pane.has_css_class("column-entering"),
                "the entry class persists for the full COLUMN_TRANSITION"
            );
            let scroller = view.state.scroller.clone();
            let gesture = resize_gesture(&scroller);
            let edge = shell_edge(&scroller, &shell);
            gesture.emit_by_name::<()>("drag-begin", &[&edge.0, &edge.1]);
            gesture.emit_by_name::<()>("drag-end", &[&0.0f64, &0.0f64]);
            pump_until(
                || !pane.has_css_class("column-entering"),
                "entry class removed even though a resize began during the entry",
            );
            assert!(
                !pane.has_css_class("column-entering"),
                "the entry class is removed once COLUMN_TRANSITION elapses"
            );
            window.destroy();
            view.browser().clear_observer();
        },
    );
}

#[test]
fn close_column_defers_removal_until_exit_animation_then_removes_it() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::close_column_defers_removal_until_exit_animation_then_removes_it",
        || {
            let fixture = Columns::new(3, 900);
            animations_on();
            let exiting = fixture.shell(1);
            assert!(exiting.parent().is_some());
            fixture.view.browser().close_column(1);
            assert!(
                exiting.has_css_class("column-exiting"),
                "the exit class is applied when a column is closed"
            );
            assert!(
                exiting.parent().is_some(),
                "the widget stays in the tree while the exit animation plays"
            );
            pump_until(
                || exiting.parent().is_none(),
                "exiting column removal after COLUMN_TRANSITION",
            );
            assert!(
                exiting.parent().is_none(),
                "the widget is removed once COLUMN_TRANSITION elapses"
            );
        },
    );
}

#[test]
fn closing_column_shrinks_its_own_width_so_siblings_reflow_instead_of_snapping() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::closing_column_shrinks_its_own_width_so_siblings_reflow_instead_of_snapping",
        || {
            let fixture = Columns::new(3, 900);
            animations_on();
            let exiting = fixture.shell(1);
            let before = exiting.width_request().max(exiting.width());
            assert!(before > 0, "fixture column starts with a real width");
            fixture.view.browser().close_column(1);
            // Sample across several points: a "jump then settle" snap would
            // still pass a one-sample check but fail a multi-sample one.
            let mut samples = vec![exiting.width_request()];
            for _ in 0..6 {
                pump_for(COLUMN_TRANSITION / 8);
                samples.push(exiting.width_request());
            }
            assert!(
                samples.windows(2).all(|pair| pair[1] <= pair[0]),
                "the width never increases during the exit transition: {samples:?}"
            );
            // Headless/debug rendering may only land 2-3 real frames; require
            // at least one real intermediate step, not just start-then-zero.
            let distinct: std::collections::HashSet<_> = samples.iter().copied().collect();
            assert!(
                distinct.len() >= 3,
                "the width passes through at least one real intermediate value across the \
                 transition instead of jumping straight from the resting width to zero: {samples:?}"
            );
            pump_until(
                || exiting.parent().is_none(),
                "exiting column removal after COLUMN_TRANSITION",
            );
        },
    );
}

#[test]
fn switching_to_a_sibling_closes_the_old_child_without_an_exit_animation() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::switching_to_a_sibling_closes_the_old_child_without_an_exit_animation",
        || {
            let fixture = Columns::new(3, 900);
            animations_on();
            let old_child = fixture.shell(1);
            let old_grandchild = fixture.shell(2);
            // Create a sibling of "level-0" under the same root so depth 0 can
            // descend into it, replacing the already-open depth-1/2 subtree.
            let root = fixture._root.path();
            let sibling = root.join("sibling-0");
            std::fs::create_dir_all(&sibling).expect("sibling directory");
            fixture.view.browser().descend(0, Location::local(&sibling));
            pump_until(|| column_loaded(&fixture.view, 1), "sibling column load");
            assert!(
                old_child.parent().is_none(),
                "the replaced child is removed immediately, not left animating"
            );
            assert!(
                !old_child.has_css_class("column-exiting"),
                "a sibling switch is a replacement, not a standalone close — no exit class"
            );
            assert!(
                old_grandchild.parent().is_some() && old_grandchild.has_css_class("column-exiting"),
                "deeper columns of the replaced branch shrink away so the strip slides"
            );
            pump_until(
                || old_grandchild.parent().is_none(),
                "the deeper column's exit animation to finish",
            );
        },
    );
}

#[test]
fn close_column_skips_exit_animation_when_animations_disabled() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::close_column_skips_exit_animation_when_animations_disabled",
        || {
            let fixture = Columns::new(3, 900);
            animations_off();
            let exiting = fixture.shell(1);
            fixture.view.browser().close_column(1);
            assert!(
                exiting.parent().is_none(),
                "with animations disabled the widget is removed immediately"
            );
            assert!(
                !exiting.has_css_class("column-exiting"),
                "no exit class is ever applied with animations disabled"
            );
        },
    );
}

#[test]
fn close_deepest_column_via_escape_also_animates_exit() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::close_deepest_column_via_escape_also_animates_exit",
        || {
            let fixture = Columns::new(3, 900);
            animations_on();
            let deepest = fixture.shell(2);
            fixture.view.browser().clear_active_selection();
            fixture.view.browser().escape();
            assert!(
                deepest.has_css_class("column-exiting"),
                "the escape close path shares the exit animation"
            );
            assert!(deepest.parent().is_some());
            pump_until(
                || deepest.parent().is_none(),
                "escape close removal after COLUMN_TRANSITION",
            );
            assert!(
                deepest.parent().is_none(),
                "the escape close path removes the widget once COLUMN_TRANSITION elapses"
            );
        },
    );
}

#[test]
fn autofit_double_click_eases_width_over_column_transition() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::autofit_double_click_eases_width_over_column_transition",
        || {
            let fixture = Columns::new(1, 900);
            animations_on();
            let shell = fixture.shell(0);
            let gesture = resize_gesture(&fixture.scroller());
            let edge = shell_edge(&fixture.scroller(), &shell);
            gesture.emit_by_name::<()>("drag-begin", &[&edge.0, &edge.1]);
            let before = shell.width_request();
            let target = autofit_target(&shell);
            assert_ne!(
                before, target,
                "fixture column must have an autofit width different from its saved width"
            );
            gesture.emit_by_name::<()>("drag-begin", &[&edge.0, &edge.1]);
            assert_eq!(
                shell.width_request(),
                before,
                "the eased width has not stepped within the same tick"
            );
            let scale = PreferenceManager::shared().interface_scale();
            assert_eq!(
                PreferenceManager::shared().browser_column_width(),
                Some(((f64::from(target) / scale).round() as i32).max(COLUMN_WIDTH)),
                "autofit saves the width it eases to, not the one it starts from"
            );
            pump_for(COLUMN_TRANSITION + Duration::from_millis(60));
            assert_eq!(
                shell.width_request(),
                target,
                "the autofit width is reached exactly after COLUMN_TRANSITION"
            );
        },
    );
}

#[test]
fn autofit_snap_skips_easing_when_animations_disabled() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::autofit_snap_skips_easing_when_animations_disabled",
        || {
            let fixture = Columns::new(1, 900);
            animations_off();
            let shell = fixture.shell(0);
            let gesture = resize_gesture(&fixture.scroller());
            let edge = shell_edge(&fixture.scroller(), &shell);
            gesture.emit_by_name::<()>("drag-begin", &[&edge.0, &edge.1]);
            let target = autofit_target(&shell);
            assert_ne!(shell.width_request(), target);
            gesture.emit_by_name::<()>("drag-begin", &[&edge.0, &edge.1]);
            assert_eq!(
                shell.width_request(),
                target,
                "with animations disabled the width jumps to the target in the same tick"
            );
        },
    );
}

#[test]
fn live_drag_resize_still_tracks_pointer_with_zero_delay() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::live_drag_resize_still_tracks_pointer_with_zero_delay",
        || {
            let fixture = Columns::new(1, 900);
            animations_off();
            let shell = fixture.shell(0);
            let gesture = resize_gesture(&fixture.scroller());
            let edge = shell_edge(&fixture.scroller(), &shell);
            gesture.emit_by_name::<()>("drag-begin", &[&edge.0, &edge.1]);
            let initial = shell.width_request().max(COLUMN_WIDTH);
            for offset in [40.0, 90.0, 150.0] {
                gesture.emit_by_name::<()>("drag-update", &[&offset, &0.0f64]);
                assert_eq!(
                    shell.width_request(),
                    resized_column_width(initial, offset),
                    "live drag-resize updates the width synchronously on every pointer update"
                );
            }
        },
    );
}

#[test]
fn a_finished_edge_drag_resizes_every_open_column() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::a_finished_edge_drag_resizes_every_open_column",
        || {
            let fixture = Columns::new(3, 1400);
            animations_off();
            let dragged = fixture.shell(1);
            let others = [fixture.shell(0), fixture.shell(2)];
            let before: Vec<_> = others.iter().map(gtk::Box::width_request).collect();
            let gesture = resize_gesture(&fixture.scroller());
            let edge = shell_edge(&fixture.scroller(), &dragged);
            gesture.emit_by_name::<()>("drag-begin", &[&edge.0, &edge.1]);
            gesture.emit_by_name::<()>("drag-update", &[&80.0f64, &0.0f64]);
            assert_eq!(
                others
                    .iter()
                    .map(gtk::Box::width_request)
                    .collect::<Vec<_>>(),
                before,
                "only the dragged column follows the pointer"
            );
            gesture.emit_by_name::<()>("drag-end", &[&80.0f64, &0.0f64]);
            for column in &others {
                assert_eq!(
                    column.width_request(),
                    dragged.width_request(),
                    "the other open columns take the dragged width once the drag ends"
                );
            }
        },
    );
}
