// SPDX-License-Identifier: MIT

//! Captions naming what a resize edge changes, shown in the window overlay.

use gtk::{glib, prelude::*};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    time::Duration,
};

const FADE: Duration = Duration::from_millis(150);
/// Long enough that sweeping the pointer across edges does not flash captions.
const HINT_DELAY: Duration = Duration::from_millis(400);

/// Where a hint centers: a point on the edge, in window overlay coordinates.
pub(super) type EdgePoint = Rc<dyn Fn(&gtk::Overlay) -> Option<(f32, f32)>>;

pub(super) fn caption(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("resize-caption");
    label.set_can_target(false);
    label
}

/// Fades a feedback widget out of its overlay, at once under reduced motion.
pub(super) fn fade_out(widget: &impl IsA<gtk::Widget>) {
    let widget = widget.clone().upcast::<gtk::Widget>();
    let Some(overlay) = widget.parent().and_downcast::<gtk::Overlay>() else {
        return;
    };
    if !super::motion::animations_enabled() {
        overlay.remove_overlay(&widget);
        return;
    }
    widget.add_css_class("fading");
    glib::timeout_add_local_once(FADE, move || {
        if widget.parent().is_some() {
            overlay.remove_overlay(&widget);
        }
    });
}

/// A caption that appears once the pointer rests on a resize edge and can then
/// follow that edge through a drag.
#[derive(Default)]
pub(super) struct EdgeHint {
    target: RefCell<Option<gtk::Widget>>,
    text: RefCell<&'static str>,
    edge: RefCell<Option<EdgePoint>>,
    caption: RefCell<Option<gtk::Label>>,
    pending: RefCell<Option<glib::SourceId>>,
}

impl EdgeHint {
    /// Shows the caption for `target` after a short rest; repeated motion over the
    /// same edge keeps the pending caption instead of restarting it.
    pub(super) fn hover(
        self: &Rc<Self>,
        target: &impl IsA<gtk::Widget>,
        text: &'static str,
        edge: EdgePoint,
    ) {
        let target = target.as_ref();
        if self.target.borrow().as_ref() == Some(target) {
            return;
        }
        self.hide();
        self.aim(target, text, edge);
        let weak: Weak<Self> = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(HINT_DELAY, move || {
            if let Some(hint) = weak.upgrade() {
                hint.pending.take();
                hint.reveal();
            }
        });
        self.pending.replace(Some(source));
    }

    /// Shows the caption at once, as a drag on `target` begins.
    pub(super) fn show(&self, target: &impl IsA<gtk::Widget>, text: &'static str, edge: EdgePoint) {
        let target = target.as_ref();
        if self.target.borrow().as_ref() != Some(target) {
            self.hide();
            self.aim(target, text, edge);
        } else {
            self.edge.replace(Some(edge));
        }
        if let Some(source) = self.pending.take() {
            source.remove();
        }
        self.reveal();
    }

    /// Re-centers a shown caption on its edge, which moves during a drag.
    pub(super) fn follow(&self) {
        let (Some(caption), Some(edge)) =
            (self.caption.borrow().clone(), self.edge.borrow().clone())
        else {
            return;
        };
        let Some(overlay) = caption.parent().and_downcast::<gtk::Overlay>() else {
            return;
        };
        if let Some((x, y)) = edge(&overlay) {
            // Measurements include the margins this sets, so take them back out.
            let width = caption.measure(gtk::Orientation::Horizontal, -1).1
                - caption.margin_start()
                - caption.margin_end();
            caption.set_margin_start((x.round() as i32 - width / 2).max(0));
            caption.set_margin_top(y.round() as i32 + 10);
        }
    }

    pub(super) fn hide(&self) {
        if let Some(source) = self.pending.take() {
            source.remove();
        }
        self.target.take();
        self.edge.take();
        if let Some(caption) = self.caption.take() {
            fade_out(&caption);
        }
    }

    fn aim(&self, target: &gtk::Widget, text: &'static str, edge: EdgePoint) {
        self.target.replace(Some(target.clone()));
        self.text.replace(text);
        self.edge.replace(Some(edge));
    }

    fn reveal(&self) {
        if self.caption.borrow().is_none() {
            let Some(overlay) = self
                .target
                .borrow()
                .as_ref()
                .and_then(super::modal::window_overlay)
            else {
                return;
            };
            let caption = caption(&crate::i18n::tr(*self.text.borrow()));
            caption.set_halign(gtk::Align::Start);
            caption.set_valign(gtk::Align::Start);
            overlay.add_overlay(&caption);
            self.caption.replace(Some(caption));
        }
        self.follow();
    }
}
