// SPDX-License-Identifier: MIT

//! Behavioral regressions for the Appearance menu's Content, Frame Top Bar, and Frame
//! Side Bar density accordion rows (Units 3 and the Frame area split): expand/collapse,
//! independent row state, and that no interaction ever closes the Appearance menu. No
//! layout/geometry assertions.

use std::rc::Rc;
use std::time::{Duration, Instant};

use super::super::*;
use crate::services::{
    LoadHandle, Preview, PreviewContent, PreviewEvent, PreviewProvider, PreviewRequest,
};
use crate::test_support::gtk_test;
use crate::ui::browser_modes::{BrowserDensity, FrameDensity};
use crate::ui::preview::PreviewDrawer;

struct NoPreview;

impl PreviewProvider for NoPreview {
    fn load(&self, request: PreviewRequest, emit: Rc<dyn Fn(PreviewEvent)>) -> LoadHandle {
        glib::idle_add_local_once(move || {
            emit(PreviewEvent::Ready(Preview {
                request_id: request.id,
                entry: request.entry,
                content_type: "text/plain".into(),
                content: PreviewContent::Text {
                    content: String::new(),
                    truncated: false,
                },
            }))
        });
        LoadHandle::new(|| {})
    }
}

fn settles(condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        if Instant::now() >= deadline {
            return false;
        }
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(2));
    }
    true
}

fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut widgets = vec![widget.clone()];
    let mut child = widget.first_child();
    while let Some(current) = child {
        widgets.extend(descendants(&current));
        child = current.next_sibling();
    }
    widgets
}

fn shows_label(widget: &gtk::Widget, text: &str) -> bool {
    descendants(widget).into_iter().any(|widget| {
        widget
            .downcast_ref::<gtk::Label>()
            .is_some_and(|label| label.text() == text)
    })
}

/// The three accordion toggles, in menu order: Content, then Frame's Top Bar, then
/// Frame's Side Bar.
fn density_toggles(popover: &gtk::Popover) -> Vec<gtk::Widget> {
    descendants(popover.upcast_ref())
        .into_iter()
        .filter(|widget| widget.has_css_class("appearance-density-toggle"))
        .collect()
}

/// The (decrease, increase) buttons of the nth density row's custom stepper, in menu
/// order: Content=0, Top Bar=1, Side Bar=2. The stepper mirrors Text Size's
/// `[decrease][value][increase]` control (see `controls::stepper`), not a native
/// `GtkSpinButton`.
fn density_stepper(popover: &gtk::Popover, index: usize) -> (gtk::Button, gtk::Button) {
    let stepper_box = descendants(popover.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Box>().ok())
        .filter(|widget| widget.has_css_class("appearance-density-custom-stepper"))
        .nth(index)
        .expect("density custom stepper");
    let buttons: Vec<gtk::Button> = descendants(stepper_box.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .collect();
    assert_eq!(
        buttons.len(),
        3,
        "expected decrease/value/increase buttons in the stepper"
    );
    (buttons[0].clone(), buttons[2].clone())
}

fn click_n_times(button: &gtk::Button, times: u32) {
    for _ in 0..times {
        button.emit_clicked();
    }
}

fn density_option(popover: &gtk::Popover, label: &str) -> gtk::Button {
    descendants(popover.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .filter(|widget| widget.has_css_class("appearance-option"))
        .find(|widget| shows_label(widget.upcast_ref(), label))
        .unwrap_or_else(|| panic!("no {label} option in the Appearance menu"))
}

fn open_appearance_menu(preferences: Rc<PreferenceManager>) -> (gtk::Window, gtk::Popover) {
    let view = browser_for_window();
    let preview = PreviewDrawer::new(Rc::new(NoPreview), false);
    let menu = build_appearance_menu(&view, &view.browser(), preferences, &preview);
    let window = gtk::Window::builder().child(&menu).build();
    window.present();
    menu.popup();
    let popover = menu.popover().expect("appearance popover");
    assert!(
        settles(|| popover.is_visible()),
        "the Appearance menu did not open"
    );
    (window, popover)
}

#[test]
fn all_three_density_rows_expand_independently_without_a_nested_popover() {
    gtk_test(
        "ui::window::tests::appearance_density::all_three_density_rows_expand_independently_without_a_nested_popover",
        || {
            PreferenceManager::seed_saved_preferences_for_test();
            let preferences = PreferenceManager::shared();
            let (window, popover) = open_appearance_menu(preferences);
            let toggles = density_toggles(&popover);
            assert_eq!(toggles.len(), 3, "Content, Top Bar, and Side Bar rows");
            let revealer_of = |toggle: &gtk::Widget| -> gtk::Revealer {
                toggle
                    .next_sibling()
                    .and_downcast::<gtk::Revealer>()
                    .expect("accordion revealer")
            };
            let content_revealer = revealer_of(&toggles[0]);
            let top_bar_revealer = revealer_of(&toggles[1]);
            let side_bar_revealer = revealer_of(&toggles[2]);
            assert!(!content_revealer.reveals_child());
            assert!(!top_bar_revealer.reveals_child());
            assert!(!side_bar_revealer.reveals_child());

            let click = |widget: &gtk::Widget| {
                widget
                    .downcast_ref::<gtk::Button>()
                    .expect("toggle")
                    .emit_clicked();
            };

            click(&toggles[0]);
            assert!(content_revealer.reveals_child(), "Content row expands");
            assert!(
                !top_bar_revealer.reveals_child() && !side_bar_revealer.reveals_child(),
                "expanding Content must not expand Top Bar or Side Bar"
            );
            assert!(popover.is_visible(), "expanding never closes the menu");

            click(&toggles[1]);
            assert!(
                content_revealer.reveals_child(),
                "Content row stays expanded"
            );
            assert!(top_bar_revealer.reveals_child(), "Top Bar row also expands");
            assert!(
                !side_bar_revealer.reveals_child(),
                "expanding Top Bar must not expand Side Bar"
            );
            assert!(
                popover.is_visible(),
                "two rows expanded keeps the menu open"
            );

            click(&toggles[2]);
            assert!(content_revealer.reveals_child());
            assert!(top_bar_revealer.reveals_child());
            assert!(
                side_bar_revealer.reveals_child(),
                "Side Bar row also expands"
            );
            assert!(
                popover.is_visible(),
                "all three rows expanded keeps the menu open"
            );

            click(&toggles[0]);
            assert!(!content_revealer.reveals_child(), "Content row collapses");
            assert!(
                top_bar_revealer.reveals_child() && side_bar_revealer.reveals_child(),
                "collapsing Content must not collapse Top Bar or Side Bar"
            );

            window.destroy();
        },
    );
}

#[test]
fn selecting_a_density_option_leaves_the_menu_open_and_only_changes_its_own_row() {
    gtk_test(
        "ui::window::tests::appearance_density::selecting_a_density_option_leaves_the_menu_open_and_only_changes_its_own_row",
        || {
            PreferenceManager::seed_saved_preferences_for_test();
            let preferences = PreferenceManager::shared();
            preferences.set_browser_density(BrowserDensity::Compact);
            preferences.set_frame_top_bar_density(FrameDensity::Compact);
            preferences.set_frame_side_bar_density(FrameDensity::Compact);
            let (window, popover) = open_appearance_menu(preferences.clone());

            // The first "Airy" option in menu order belongs to the Content row.
            density_option(&popover, "Airy").emit_clicked();
            assert!(
                popover.is_visible(),
                "choosing Airy must not close the Appearance menu"
            );
            assert_eq!(preferences.browser_density(), BrowserDensity::Airy);
            assert_eq!(
                preferences.frame_top_bar_density(),
                FrameDensity::Compact,
                "Content changes must not touch Top Bar"
            );
            assert_eq!(
                preferences.frame_side_bar_density(),
                FrameDensity::Compact,
                "Content changes must not touch Side Bar"
            );

            window.destroy();
        },
    );
}

#[test]
fn editing_a_custom_stepper_selects_custom_and_keeps_the_menu_open_without_touching_other_rows() {
    gtk_test(
        "ui::window::tests::appearance_density::editing_a_custom_stepper_selects_custom_and_keeps_the_menu_open_without_touching_other_rows",
        || {
            // Seeded fixture custom px: Content 11, Top Bar 9, Side Bar 6.
            PreferenceManager::seed_saved_preferences_for_test();
            let preferences = PreferenceManager::shared();
            preferences.set_browser_density(BrowserDensity::Compact);
            preferences.set_frame_top_bar_density(FrameDensity::Compact);
            preferences.set_frame_side_bar_density(FrameDensity::Compact);
            let (window, popover) = open_appearance_menu(preferences.clone());

            let (content_decrease, _) = density_stepper(&popover, 0);
            click_n_times(&content_decrease, 4);
            assert!(
                popover.is_visible(),
                "editing a custom stepper must not close the Appearance menu"
            );
            assert_eq!(preferences.browser_density(), BrowserDensity::custom(7));
            assert_eq!(
                preferences.frame_top_bar_density(),
                FrameDensity::Compact,
                "editing Content's stepper must not touch Top Bar"
            );
            assert_eq!(
                preferences.frame_side_bar_density(),
                FrameDensity::Compact,
                "editing Content's stepper must not touch Side Bar"
            );

            let (_, top_bar_increase) = density_stepper(&popover, 1);
            click_n_times(&top_bar_increase, 3);
            assert_eq!(
                preferences.frame_top_bar_density(),
                FrameDensity::custom(12)
            );
            assert_eq!(
                preferences.browser_density(),
                BrowserDensity::custom(7),
                "editing Top Bar's stepper must not touch Content"
            );
            assert_eq!(
                preferences.frame_side_bar_density(),
                FrameDensity::Compact,
                "editing Top Bar's stepper must not touch Side Bar"
            );

            let (side_bar_decrease, _) = density_stepper(&popover, 2);
            click_n_times(&side_bar_decrease, 3);
            assert_eq!(
                preferences.frame_side_bar_density(),
                FrameDensity::custom(3)
            );
            assert_eq!(
                preferences.browser_density(),
                BrowserDensity::custom(7),
                "editing Side Bar's stepper must not touch Content"
            );
            assert_eq!(
                preferences.frame_top_bar_density(),
                FrameDensity::custom(12),
                "editing Side Bar's stepper must not touch Top Bar"
            );

            window.destroy();
        },
    );
}
