// SPDX-License-Identifier: MIT

use std::rc::Weak;

use super::*;
use crate::ui::{
    browser::{BrowserView, COLUMN_WIDTH, WeakBrowserView, single_pane_preview_reservation},
    browser_modes::BrowserMode,
    window::{
        MIN_SIDEBAR_WIDTH, SidebarState, SidebarView, preferred_sidebar_width, sidebar_rail_width,
    },
};

const MIN_SPLIT_PREVIEW_WIDTH: i32 = 240;
const RAIL_RELEASE_MARGIN: i32 = 24;

#[derive(Default)]
pub(super) struct SplitSizing {
    binding: RefCell<Option<BrowserBinding>>,
    manual_width: Cell<Option<i32>>,
    resizing: Cell<bool>,
    suspended: Cell<bool>,
    resume_media: Cell<bool>,
    reload_on_resume: Cell<bool>,
    sidebar_railed: Cell<bool>,
    sidebar_saved_width: Cell<i32>,
}

impl SplitSizing {
    pub(super) fn close(&self) {
        self.resizing.set(false);
        self.suspended.set(false);
        self.resume_media.set(false);
        self.reload_on_resume.set(false);
    }

    pub(super) fn defer_load(&self) {
        self.reload_on_resume.set(true);
        self.resume_media.set(false);
    }

    pub(super) fn play_or_defer(&self, media: &gtk::MediaStream) {
        if self.suspended.get() {
            self.resume_media.set(true);
        } else {
            media.play();
        }
    }

    pub(super) fn browser(&self) -> Option<BrowserView> {
        self.binding
            .borrow()
            .as_ref()
            .and_then(|binding| binding.browser.upgrade())
    }

    pub(super) fn is_suspended(&self) -> bool {
        self.suspended.get()
    }
}

struct BrowserBinding {
    content: glib::WeakRef<gtk::Paned>,
    browser: WeakBrowserView,
    sidebar: Option<Weak<SidebarState>>,
}

#[derive(Clone, Copy)]
struct Geometry {
    available: i32,
    occupied: i32,
    start_minimum: i32,
    show_minimum: i32,
    separator: i32,
}

impl Geometry {
    fn can_show_preview(self) -> bool {
        self.available - self.separator - self.show_minimum >= MIN_SPLIT_PREVIEW_WIDTH
    }

    fn maximum_width(self) -> i32 {
        (self.available - self.separator - self.start_minimum).max(1)
    }

    fn minimum_width(self, manual: bool) -> i32 {
        let minimum = if manual { COLUMN_WIDTH } else { MIN_WIDTH };
        minimum.min(self.maximum_width())
    }

    fn desired_width(self, manual: Option<i32>) -> i32 {
        let free = (self.available - self.separator - self.occupied).max(0);
        let desired =
            manual.unwrap_or_else(|| free.saturating_mul(9).saturating_div(10).min(MAX_WIDTH));
        desired.clamp(self.minimum_width(manual.is_some()), self.maximum_width())
    }

    fn preview_width(self, manual: Option<i32>) -> i32 {
        self.desired_width(manual).max(MIN_SPLIT_PREVIEW_WIDTH)
    }

    fn position(self, manual: Option<i32>) -> i32 {
        self.available - self.separator - self.preview_width(manual)
    }
}

fn separator(split: &gtk::Paned) -> Option<gtk::Widget> {
    let mut child = split.first_child();
    while let Some(widget) = child {
        if widget.css_name() == "separator" {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

pub(in crate::ui) fn separator_width(split: &gtk::Paned) -> i32 {
    separator(split).map_or(0, |handle| {
        handle.measure(gtk::Orientation::Horizontal, -1).0
    })
}

fn sidebar_width(content: &gtk::Paned) -> i32 {
    if content
        .start_child()
        .is_some_and(|child| child.get_visible())
    {
        content.position() + separator_width(content)
    } else {
        0
    }
}

impl PreviewDrawer {
    pub(in crate::ui) fn attach_split(
        &self,
        split: &gtk::Paned,
        content: &gtk::Paned,
        browser: &BrowserView,
        sidebar: Option<&SidebarView>,
    ) {
        self.state.split.replace(Some(split.clone()));
        self.state.sizing.binding.replace(Some(BrowserBinding {
            content: content.downgrade(),
            browser: browser.downgrade(),
            sidebar: sidebar.map(|sidebar| Rc::downgrade(&sidebar.state)),
        }));
        self.state.refresh_panel_action();
        browser.bind_preview_scrolling();
        let weak = Rc::downgrade(&self.state);
        browser.connect_view_mode_changed(move |_| {
            if let Some(state) = weak.upgrade() {
                state.refresh_panel_action();
                if !state.is_enabled() {
                    state.hide_panel();
                    state.release_sidebar_rail();
                }
            }
        });
        let weak = Rc::downgrade(&self.state);
        let weak_browser = browser.downgrade();
        browser.connect_search_selection_changed(Rc::new(move || {
            let request_at_selection = weak.upgrade().and_then(|state| state.current_request.get());
            let weak = weak.clone();
            let weak_browser = weak_browser.clone();
            glib::idle_add_local_once(move || {
                let Some(state) = weak.upgrade().filter(|state| {
                    state.is_enabled() && state.current_request.get() == request_at_selection
                }) else {
                    return;
                };
                let Some(browser) = weak_browser.upgrade() else {
                    return;
                };
                if browser.selected_search_results().is_none() {
                    return;
                }
                let entry = if browser.results_replace_listing() {
                    browser
                        .browser()
                        .active_depth()
                        .and_then(|depth| browser.displayed_cursor_entry(depth))
                } else {
                    browser.selected_search_result()
                };
                if let Some(entry) = preview_target(entry) {
                    state.show_after_focus_change(entry, browser.browser().active_depth());
                } else {
                    state.clear_target();
                }
            });
        }));
        split.set_end_child(Some(&self.state.slot));
        self.state.revealer.set_visible(self.state.is_enabled());
        self.state.slot.set_visible(self.state.is_enabled());
        let weak = Rc::downgrade(&self.state);
        split.add_tick_callback(move |split, _| {
            let Some(state) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if state.is_enabled() {
                state.sync_split(split);
            }
            glib::ControlFlow::Continue
        });
        let weak = Rc::downgrade(&self.state);
        split.connect_unmap(move |_| {
            if let Some(state) = weak.upgrade()
                && state.is_enabled()
            {
                state.suspend_panel();
            }
        });
        let weak = Rc::downgrade(&self.state);
        split.connect_unrealize(move |_| {
            if let Some(state) = weak.upgrade() {
                state.stop();
            }
        });
        if let Some(handle) = separator(split) {
            handle.set_cursor_from_name(Some("col-resize"));
        }
        install_resize(split, &self.state);
    }
}

impl PreviewState {
    pub(super) fn selected_entry(&self) -> (Option<FileEntry>, Option<usize>) {
        if let Some(binding) = self.sizing.binding.borrow().as_ref()
            && let Some(browser) = binding.browser.upgrade()
        {
            return (
                preview_target(
                    browser
                        .selected_search_result()
                        .or_else(|| browser.browser().focused_entry()),
                ),
                browser.browser().active_depth(),
            );
        }
        (
            preview_target(self.current.borrow().clone()),
            self.current_depth.get(),
        )
    }

    pub(super) fn reserves_empty_preview(&self) -> bool {
        self.is_enabled()
            && self
                .sizing
                .binding
                .borrow()
                .as_ref()
                .and_then(|binding| binding.browser.upgrade())
                .is_some_and(|browser| {
                    matches!(
                        browser.view_mode(),
                        BrowserMode::Columns | BrowserMode::Icons
                    )
                })
    }

    fn geometry(&self, split: &gtk::Paned) -> Geometry {
        let available = split.width();
        let mut geometry = Geometry {
            available,
            occupied: available.saturating_sub(DEFAULT_WIDTH),
            start_minimum: 0,
            show_minimum: 0,
            separator: separator_width(split),
        };
        if let Some(binding) = self.sizing.binding.borrow().as_ref()
            && let Some(content) = binding.content.upgrade()
        {
            let sidebar = self.intended_sidebar_width(binding, &content);
            geometry.occupied =
                sidebar + single_pane_preview_reservation((available - sidebar).max(0));
            geometry.start_minimum = sidebar.saturating_add(COLUMN_WIDTH);
            geometry.show_minimum = geometry.start_minimum;
        }
        geometry
    }

    // Budget the intended sidebar width so a clamped divider can recover.
    fn intended_sidebar_width(&self, binding: &BrowserBinding, content: &gtk::Paned) -> i32 {
        let current = sidebar_width(content);
        if current == 0
            || binding
                .sidebar
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some_and(|sidebar| sidebar.rail.get())
        {
            return current;
        }
        current.max(self.saved_sidebar_width(binding) + separator_width(content))
    }

    fn saved_sidebar_width(&self, binding: &BrowserBinding) -> i32 {
        binding
            .sidebar
            .as_ref()
            .and_then(Weak::upgrade)
            .and_then(|sidebar| sidebar.saved_width.get())
            .unwrap_or_else(|| {
                let saved = self.sizing.sidebar_saved_width.get();
                if saved > 0 {
                    saved
                } else {
                    preferred_sidebar_width()
                }
            })
            .max(MIN_SIDEBAR_WIDTH)
    }

    pub(super) fn can_show_in(&self, split: &gtk::Paned) -> bool {
        split.is_mapped() && self.geometry(split).can_show_preview()
    }

    pub(super) fn show_panel(&self) {
        // The revealer is only a visibility latch now: `animate_reveal` slides
        // the split itself, so the revealer toggles instantly.
        self.revealer.set_transition_duration(0);
        self.pane.set_width_request(0);
        self.slot.set_visible(true);
        self.revealer.set_visible(true);
        self.revealer.set_reveal_child(true);
    }

    pub(super) fn release_sidebar_rail(&self) {
        if !self.sizing.sidebar_railed.replace(false) {
            return;
        }
        if let Some(binding) = self.sizing.binding.borrow().as_ref()
            && let Some(content) = binding.content.upgrade()
        {
            let available = content
                .root()
                .map_or_else(|| content.width(), |r| r.width());
            let needs_full = preferred_sidebar_width() + COLUMN_WIDTH + 1;
            let keep_railed = available > 0 && available < needs_full;
            let sidebar = binding.sidebar.as_ref().and_then(Weak::upgrade);
            if let Some(sidebar) = sidebar.as_ref() {
                sidebar.set_rail(keep_railed);
            }
            if content
                .start_child()
                .is_some_and(|sidebar| sidebar.get_visible())
            {
                if keep_railed {
                    content.set_position(sidebar_rail_width());
                } else {
                    let restore = sidebar
                        .as_ref()
                        .and_then(|s| s.saved_width.get())
                        .unwrap_or_else(|| {
                            let saved = self.sizing.sidebar_saved_width.get();
                            if saved > 0 {
                                saved
                            } else {
                                preferred_sidebar_width()
                            }
                        })
                        .max(MIN_SIDEBAR_WIDTH);
                    content.set_position(restore);
                }
            }
        }
    }

    pub(super) fn hide_panel(&self) {
        let restore_browser_focus = self.begin_hide();
        self.finish_hide(restore_browser_focus);
    }

    /// Pins the pane to its resting width while the split sweeps, so the panel
    /// is laid out once at its final size and merely clipped by the divider —
    /// it slides as a whole instead of reflowing or revealing instantly.
    fn pin_pane_width(&self, split: &gtk::Paned) {
        self.pane.set_width_request(
            self.geometry(split)
                .preview_width(self.sizing.manual_width.get()),
        );
    }

    /// Whether the browser view needs focus restored once the panel is gone.
    fn hide_intent(&self) -> bool {
        self.pane
            .root()
            .and_then(|root| root.focus())
            .is_some_and(|focused| {
                focused == self.pane
                    || focused.is_ancestor(&self.pane)
                    // GTK may focus a divider while allocating a smaller split.
                    || self.split.borrow().as_ref().is_some_and(|split| split.has_focus())
                    || self.sizing.binding.borrow().as_ref().is_some_and(|binding| {
                        binding.content.upgrade().is_some_and(|content| content.has_focus())
                    })
            })
    }

    /// Collapses the revealer instantly without tearing down slot/split state
    /// yet; `hide_panel` follows with `finish_hide`, while the animated close
    /// (`animate_reveal`) keeps the pane revealed and lets the divider sweep
    /// carry it out before `finish_hide` retires the slot.
    fn begin_hide(&self) -> bool {
        let restore_browser_focus = self.hide_intent();
        self.revealer.set_transition_duration(0);
        self.revealer.set_reveal_child(false);
        restore_browser_focus
    }

    fn finish_hide(&self, restore_browser_focus: bool) {
        self.revealer.set_reveal_child(false);
        self.revealer.set_visible(false);
        if let Some(split) = self.split.borrow().as_ref() {
            self.slot.set_visible(false);
            split.set_resize_start_child(true);
            split.set_resize_end_child(false);
            split.set_position(split.width());
        }
        if restore_browser_focus
            && let Some(split) = self.split.borrow().as_ref()
            && let Some(browser) = self
                .sizing
                .binding
                .borrow()
                .as_ref()
                .and_then(|binding| binding.browser.upgrade())
        {
            split.add_tick_callback(move |_, _| {
                browser.focus_file_view();
                glib::ControlFlow::Break
            });
        }
    }

    fn suspend_panel(&self) {
        if self.sizing.suspended.replace(true) {
            return;
        }
        self.animation_generation
            .set(self.animation_generation.get().saturating_add(1));
        self.animating.set(false);
        self.sizing.resizing.set(false);
        let media = self.media.borrow().clone();
        self.sizing
            .resume_media
            .set(media.as_ref().is_some_and(|media| media.is_playing()));
        if let Some(media) = media {
            media.pause();
        }
        self.hide_panel();
    }

    pub(super) fn sync_split(self: &Rc<Self>, split: &gtk::Paned) {
        let mut geometry = self.geometry(split);
        let preview_present = self.current.borrow().is_some() || self.reserves_empty_preview();
        if preview_present
            && let Some(binding) = self.sizing.binding.borrow().as_ref()
            && let Some(content) = binding.content.upgrade()
        {
            let sidebar = binding.sidebar.as_ref().and_then(Weak::upgrade);
            let visible = content
                .start_child()
                .is_some_and(|sidebar| sidebar.get_visible());
            let is_railed = sidebar.as_ref().is_some_and(|s| s.rail.get());
            let saved_width = sidebar
                .as_ref()
                .and_then(|s| s.saved_width.get())
                .unwrap_or_else(|| {
                    let saved = self.sizing.sidebar_saved_width.get();
                    if saved > 0 {
                        saved
                    } else {
                        preferred_sidebar_width()
                    }
                })
                .max(MIN_SIDEBAR_WIDTH);
            let full = if sidebar.is_none() {
                0
            } else if is_railed || !visible {
                saved_width.max(preferred_sidebar_width())
            } else {
                // A manually narrowed sidebar must not prevent railing.
                content.position().max(preferred_sidebar_width())
            };
            let content_sep = separator_width(&content);
            let resizing_columns = binding
                .browser
                .upgrade()
                .is_some_and(|browser| browser.is_resizing_columns());
            let occupied = COLUMN_WIDTH;
            // A previously clamped manual width must not defeat the preview minimum.
            let preview_needed = self
                .sizing
                .manual_width
                .get()
                .unwrap_or(MIN_SPLIT_PREVIEW_WIDTH)
                .max(MIN_SPLIT_PREVIEW_WIDTH);
            let needs = full + content_sep + occupied + geometry.separator + preview_needed;
            let content_has_room = content.width() <= 0 || content.width() >= full + COLUMN_WIDTH;
            // Measure outside the split so railing cannot change its own threshold.
            let available = split
                .parent()
                .map(|parent| parent.width())
                .filter(|width| *width > 0)
                .unwrap_or(geometry.available);
            // Hysteresis prevents toggling at the threshold.
            let wants_rail = if is_railed {
                available < needs + RAIL_RELEASE_MARGIN
            } else {
                available < needs
            };
            let change_applies = !resizing_columns
                && !self.sizing.resizing.get()
                && if wants_rail {
                    !is_railed && sidebar.is_some()
                } else {
                    is_railed && content_has_room
                };
            let squeezed_room = !is_railed
                && visible
                && !wants_rail
                && !resizing_columns
                && !self.sizing.resizing.get()
                && content.position() < saved_width
                && content.width() >= saved_width + content_sep + COLUMN_WIDTH;
            if squeezed_room {
                content.set_position(saved_width);
                geometry = self.geometry(split);
            }
            if change_applies {
                if wants_rail {
                    if visible {
                        let squeezed =
                            content.position() + content_sep + COLUMN_WIDTH >= content.width();
                        let width = if squeezed {
                            saved_width
                        } else {
                            content.position().max(MIN_SIDEBAR_WIDTH)
                        };
                        self.sizing.sidebar_saved_width.set(width);
                        if let Some(sidebar) = sidebar.as_ref() {
                            sidebar.saved_width.set(Some(width));
                        }
                    }
                    if let Some(sidebar) = sidebar.as_ref() {
                        sidebar.set_rail(true);
                    }
                    if visible {
                        content.set_position(sidebar_rail_width());
                    }
                    self.sizing.sidebar_railed.set(true);
                } else {
                    if let Some(sidebar) = sidebar.as_ref() {
                        sidebar.set_rail(false);
                    }
                    if visible {
                        content.set_position(saved_width);
                    }
                    self.sizing.sidebar_railed.set(false);
                }
                geometry = self.geometry(split);
            }
        }
        self.slot.set_width_request(0);
        if self.current.borrow().is_none() {
            let reserves_empty_preview = self.reserves_empty_preview();
            if !reserves_empty_preview || !geometry.can_show_preview() {
                if self.revealer.reveals_child() {
                    self.hide_panel();
                }
                if !reserves_empty_preview {
                    self.release_sidebar_rail();
                    self.sizing.suspended.set(false);
                }
                return;
            }
            self.show_placeholder();
        }
        if !split.is_mapped() || !geometry.can_show_preview() {
            self.suspend_panel();
            return;
        }
        if self.animating.get() || self.sizing.resizing.get() {
            return;
        }
        let manual = self.sizing.manual_width.get();
        let position = geometry.position(manual);
        split.set_resize_start_child(true);
        split.set_resize_end_child(false);
        let restored = self.sizing.suspended.replace(false);
        if restored || !self.revealer.reveals_child() {
            self.show_panel();
        }
        let minimum = geometry.minimum_width(manual.is_some());
        if self.pane.width_request() != minimum || split.position() != position {
            self.pane.set_width_request(minimum);
            split.set_position(position);
        }
        if self.sizing.reload_on_resume.replace(false) {
            let entry = self.current.borrow().clone();
            if let Some(entry) = entry {
                self.load(entry, 0);
            }
        } else if restored && self.sizing.resume_media.replace(false) {
            let media = self.media.borrow().clone();
            if let Some(media) = media {
                media.play();
            }
        }
        if restored {
            self.resume_keyboard_claim();
        }
    }

    pub(super) fn opening_width(&self, available: i32) -> i32 {
        self.split
            .borrow()
            .as_ref()
            .map_or(DEFAULT_WIDTH.min(available), |split| {
                self.geometry(split)
                    .preview_width(self.sizing.manual_width.get())
            })
    }

    /// Slides the preview panel in or out like a drawer: the `Paned` divider
    /// sweeps between the tucked-away and resting positions while the pane is
    /// pinned to its resting width and clipped by the slot, so the whole
    /// panel — background, header and content — glides as one unit instead of
    /// revealing instantly or reflowing mid-animation.
    ///
    /// `on_settled` runs exactly once, either synchronously (reduced motion,
    /// the divider is already at rest, or the slot can't show a preview at
    /// all) or once the sweep completes. Callers hang any state change that
    /// depends on the panel actually being open or closed off it (e.g.
    /// `close()`'s logical teardown must not run until the panel has visually
    /// closed).
    pub(super) fn animate_reveal(
        self: &Rc<Self>,
        split: &gtk::Paned,
        expanded: bool,
        on_settled: impl FnOnce(&Rc<Self>) + 'static,
    ) {
        self.sync_split(split);
        let geometry = self.geometry(split);
        if expanded && !geometry.can_show_preview() {
            on_settled(self);
            return;
        }

        let animation_id = self.animation_generation.get().saturating_add(1);
        self.animation_generation.set(animation_id);

        let target = if expanded {
            geometry.position(self.sizing.manual_width.get())
        } else {
            split.width()
        };
        // Opening always sweeps in from the panel fully tucked away so the
        // whole thing animates; closing sweeps out from wherever it rests.
        // The divider is the only thing that moves — the pane keeps its
        // resting width and is clipped, so nothing reflows.
        let (start, restore_browser_focus) = if expanded {
            split.set_resize_start_child(false);
            split.set_resize_end_child(true);
            self.show_panel();
            self.pin_pane_width(split);
            let start = split.width();
            split.set_position(start);
            (start, false)
        } else {
            self.pin_pane_width(split);
            (split.position(), self.hide_intent())
        };

        if !super::super::motion::animations_enabled() || start == target {
            split.set_position(target);
            self.animating.set(false);
            self.pane.set_width_request(0);
            if expanded {
                self.sync_split(split);
            } else {
                self.finish_hide(restore_browser_focus);
            }
            on_settled(self);
            return;
        }

        self.animating.set(true);
        let started = Instant::now();
        let duration = TRANSITION;
        let weak = Rc::downgrade(self);
        let split = split.clone();
        let on_settled = RefCell::new(Some(on_settled));
        let _tick = split.clone().add_tick_callback(move |split, _| {
            let Some(state) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if state.animation_generation.get() != animation_id {
                return glib::ControlFlow::Break;
            }
            let target = if expanded {
                state
                    .geometry(split)
                    .position(state.sizing.manual_width.get())
            } else {
                split.width()
            };
            let progress =
                (started.elapsed().as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0);
            let eased = super::super::motion::emphasized_deceleration(progress);
            let position = (f64::from(start) + f64::from(target - start) * eased).round() as i32;
            split.set_position(position);
            state.pin_pane_width(split);
            if progress < 1.0 {
                return glib::ControlFlow::Continue;
            }
            state.animating.set(false);
            split.set_position(target);
            if expanded {
                state.sync_split(split);
            } else {
                state.finish_hide(restore_browser_focus);
            }
            if let Some(callback) = on_settled.borrow_mut().take() {
                callback(&state);
            }
            glib::ControlFlow::Break
        });
    }

    pub(super) fn resize_preview(self: &Rc<Self>, split: &gtk::Paned, position: i32) {
        let geometry = self.geometry(split);
        if !geometry.can_show_preview() {
            return;
        }
        let width = (geometry.available - geometry.separator - position)
            .clamp(geometry.minimum_width(true), geometry.maximum_width());
        self.sizing.manual_width.set(Some(width));
        self.sync_split(split);
    }
}

fn install_resize(split: &gtk::Paned, state: &Rc<PreviewState>) {
    // Observe input without competing with GtkPaned's own drag gesture.
    let pointer = gtk::EventControllerLegacy::new();
    pointer.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = Rc::downgrade(state);
    pointer.connect_event(move |controller, event| {
        let Some(state) = weak.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let Some(split) = controller.widget().and_downcast::<gtk::Paned>() else {
            return glib::Propagation::Proceed;
        };
        match event.event_type() {
            gtk::gdk::EventType::ButtonPress | gtk::gdk::EventType::TouchBegin
                if (event.event_type() == gtk::gdk::EventType::TouchBegin
                    || event
                        .downcast_ref::<gtk::gdk::ButtonEvent>()
                        .is_some_and(|e| e.button() == 1))
                    && state.is_enabled()
                    && !state.sizing.is_suspended()
                    && on_separator(&split, event) =>
            {
                state
                    .animation_generation
                    .set(state.animation_generation.get().saturating_add(1));
                state.animating.set(false);
                state.sizing.resizing.set(true);
                let minimum = state.geometry(&split).minimum_width(true);
                state.pane.set_width_request(minimum);
                state.slot.set_width_request(minimum);
            }
            gtk::gdk::EventType::ButtonRelease
            | gtk::gdk::EventType::TouchEnd
            | gtk::gdk::EventType::TouchCancel
            | gtk::gdk::EventType::GrabBroken => {
                state.sizing.resizing.set(false);
            }
            _ => {}
        }
        glib::Propagation::Proceed
    });
    split.add_controller(pointer);
    let weak = Rc::downgrade(state);
    split.connect_position_notify(move |split| {
        if let Some(state) = weak.upgrade()
            && state.is_enabled()
            && state.sizing.resizing.replace(false)
        {
            state.resize_preview(split, split.position());
            state.sizing.resizing.set(true);
        }
    });

    // Unhandled browser keys also reach GtkPaned; only handle-focused actions are resizes.
    let weak = Rc::downgrade(state);
    split.connect_move_handle(move |split, _| {
        if split.has_focus() {
            if let Some(state) = weak.upgrade()
                && state.is_enabled()
                && !state.sizing.is_suspended()
            {
                let minimum = state.geometry(split).minimum_width(true);
                state.pane.set_width_request(minimum);
                state.slot.set_width_request(minimum);
            }
            remember_keyboard_width(weak.clone());
        }
        false
    });
    let weak = Rc::downgrade(state);
    split.connect_cancel_position(move |split| {
        if split.has_focus() {
            remember_keyboard_width(weak.clone());
        }
        false
    });
}

fn on_separator(split: &gtk::Paned, event: &gtk::gdk::Event) -> bool {
    let Some((x, y)) = event.position() else {
        return false;
    };
    let Some(native) = split.native() else {
        return false;
    };
    let (dx, dy) = native.surface_transform();
    let native: gtk::Widget = native.upcast();
    let Some(point) = native.compute_point(
        split,
        &gtk::graphene::Point::new((x + dx) as f32, (y + dy) as f32),
    ) else {
        return false;
    };
    split.pick(
        f64::from(point.x()),
        f64::from(point.y()),
        gtk::PickFlags::DEFAULT,
    ) == separator(split)
}

fn remember_keyboard_width(weak: std::rc::Weak<PreviewState>) {
    let Some(state) = weak
        .upgrade()
        .filter(|state| state.is_enabled() && !state.sizing.is_suspended())
    else {
        return;
    };
    let Some(split) = state.split.borrow().clone() else {
        return;
    };
    let before = split.position();
    state.sizing.resizing.set(true);
    // Wait for GTK's default action handler before resuming automatic layout.
    glib::idle_add_local_once(move || {
        if let Some(state) = weak.upgrade() {
            state.sizing.resizing.set(false);
            if state.is_enabled() && split.position() != before {
                state.resize_preview(&split, split.position());
            }
        }
    });
}
