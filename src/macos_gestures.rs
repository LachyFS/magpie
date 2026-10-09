//! GPUI 0.2.2 forwards scroll events but not AppKit magnification events.
//! A window-local monitor bridges native trackpad pinch into the same camera.
#![allow(unexpected_cfgs)] // objc 0.2 macros test the legacy cargo-clippy feature.

use crate::app::Magpie;
use block::ConcreteBlock;
use cocoa::{
    base::{id, nil},
    foundation::{NSPoint, NSRect},
};
use gpui::{Context, Window};
use objc::{
    class, msg_send,
    rc::StrongPtr,
    runtime::{BOOL, YES},
    sel, sel_impl,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(crate) struct MagnifyMonitor(StrongPtr);

impl MagnifyMonitor {
    pub(crate) fn install(window: &Window, cx: &Context<Magpie>) -> Option<Self> {
        let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw()
        else {
            return None;
        };
        let context = window.to_async(cx);
        let entity = cx.weak_entity();
        // SAFETY: GPUI provides a live AppKit NSView on the main thread. Retain
        // it for the block lifetime. AppKit invokes local monitors on that same
        // thread, before dispatch (outside GPUI's event callback/borrow).
        unsafe {
            let view = StrongPtr::retain(handle.ns_view.as_ptr().cast());
            let handler = ConcreteBlock::new(move |event: id| -> id {
                let owner: id = msg_send![*view, window];
                let event_window: id = msg_send![event, window];
                if owner == nil || event_window != owner {
                    return event;
                }
                let location: NSPoint = msg_send![event, locationInWindow];
                let point: NSPoint = msg_send![*view, convertPoint:location fromView:nil];
                let flipped: BOOL = msg_send![*view, isFlipped];
                let bounds: NSRect = msg_send![*view, bounds];
                let anchor = [
                    point.x - bounds.origin.x,
                    if flipped == YES {
                        point.y - bounds.origin.y
                    } else {
                        bounds.origin.y + bounds.size.height - point.y
                    },
                ];
                let magnification: f64 = msg_send![event, magnification];
                let handled = context
                    .clone()
                    .update(|_, cx| {
                        entity
                            .update(cx, |this, cx| this.pinch(anchor, magnification, cx))
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if handled { nil } else { event }
            })
            .copy();
            // NSEventTypeMagnify = 30. Cocoa's monitor copies the block.
            let token: id = msg_send![class!(NSEvent),
                addLocalMonitorForEventsMatchingMask: (1_u64 << 30)
                handler: &*handler];
            if token == nil {
                None
            } else {
                Some(Self(StrongPtr::retain(token)))
            }
        }
    }
}

impl Drop for MagnifyMonitor {
    fn drop(&mut self) {
        // SAFETY: retained monitor token, removed on the owning UI thread.
        unsafe {
            let _: () = msg_send![class!(NSEvent), removeMonitor: *self.0];
        }
    }
}
