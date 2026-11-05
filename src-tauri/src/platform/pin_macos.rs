//! Pin to screen: the screenshot floats in a borderless always-on-top window. Drag to move,
//! scroll or pinch to resize, Esc / double-click / right-click to close. Native NSWindow +
//! NSImageView, so a pin costs the image and nothing else (no webview, no helper processes).

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSEvent, NSImage, NSImageScaling, NSImageView, NSScreen, NSWindow,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGColor;
use objc2_foundation::NSData;

define_class!(
    // SAFETY: NSWindow has no subclassing requirements; only `canBecomeKeyWindow` is overridden
    // (a borderless window must opt in to receive key events such as Esc).
    #[unsafe(super = NSWindow)]
    #[thread_kind = MainThreadOnly]
    #[name = "ScrmePinWindow"]
    struct PinWindow;

    impl PinWindow {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool {
            true
        }
    }
);

define_class!(
    // SAFETY: NSImageView has no subclassing requirements for these overrides.
    #[unsafe(super = NSImageView)]
    #[thread_kind = MainThreadOnly]
    #[name = "ScrmePinView"]
    struct PinView;

    impl PinView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            true // dragging anywhere on the picture moves the pin
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if event.clickCount() >= 2 {
                self.close();
            } else {
                // SAFETY: forwards to the superclass implementation (window dragging).
                unsafe { msg_send![super(self), mouseDown: event] }
            }
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, _event: &NSEvent) {
            self.close();
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if event.keyCode() == 53 {
                self.close(); // Esc
            }
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            self.resize_by(1.0 + event.scrollingDeltaY() * 0.004);
        }

        #[unsafe(method(magnifyWithEvent:))]
        fn magnify(&self, event: &NSEvent) {
            self.resize_by(1.0 + event.magnification());
        }
    }
);

thread_local! {
    /// Every open pin (main thread only), so they stay alive until closed.
    static PINS: RefCell<Vec<Retained<PinWindow>>> = const { RefCell::new(Vec::new()) };
    /// A closed pin is parked, not freed on the spot: `close` runs inside the view's own
    /// event handler and releasing a view while its method runs would be a use-after-free.
    static PARKED: RefCell<Vec<Retained<PinWindow>>> = const { RefCell::new(Vec::new()) };
}

impl PinView {
    fn close(&self) {
        let Some(win) = self.window() else { return };
        win.orderOut(None);
        PINS.with(|p| {
            let mut pins = p.borrow_mut();
            if let Some(i) = pins
                .iter()
                .position(|w| std::ptr::eq(&***w as *const NSWindow, &*win))
            {
                let w = pins.remove(i);
                w.setContentView(None); // drop the picture now; the empty shell is freed later
                PARKED.with(|k| k.borrow_mut().push(w));
            }
        });
    }

    /// Scale the window around its centre, keeping the aspect ratio.
    fn resize_by(&self, factor: f64) {
        let Some(win) = self.window() else { return };
        let f = win.frame();
        let screen = win.screen().map_or(f.size, |s| s.frame().size);
        let factor = factor.clamp(0.8, 1.25);
        let (mut w, mut h) = (f.size.width * factor, f.size.height * factor);
        if w.min(h) < 80.0 && factor < 1.0 {
            return; // don't shrink to a speck
        }
        let cap = (screen.width * 0.95 / w)
            .min(screen.height * 0.95 / h)
            .min(1.0);
        (w, h) = (w * cap, h * cap);
        let origin = CGPoint::new(
            f.origin.x + (f.size.width - w) / 2.0,
            f.origin.y + (f.size.height - h) / 2.0,
        );
        win.setFrame_display(CGRect::new(origin, CGSize::new(w, h)), true);
    }
}

/// Open a pin showing a TIFF image of `px` pixels. Main thread only.
pub(crate) fn show(mtm: MainThreadMarker, tiff: Vec<u8>, px: (u32, u32)) {
    PARKED.with(|k| k.borrow_mut().clear());
    let data = NSData::from_vec(tiff);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    let screen = NSScreen::screens(mtm).firstObject();
    let sf = screen.as_ref().map_or(
        CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1440.0, 900.0)),
        |s| s.frame(),
    );

    // Pixels → points on this screen, fit within 70% of it, then cascade each new pin a little.
    let scale = screen.as_ref().map_or(2.0, |s| s.backingScaleFactor());
    let points = (px.0 as f64 / scale, px.1 as f64 / scale);
    let k = (sf.size.width * 0.7 / points.0)
        .min(sf.size.height * 0.7 / points.1)
        .min(1.0);
    let (w, h) = (points.0 * k, points.1 * k);
    let n = PINS.with(|p| p.borrow().len()) as f64;
    let origin = CGPoint::new(
        sf.origin.x + (sf.size.width - w) / 2.0 + n * 28.0,
        sf.origin.y + (sf.size.height - h) / 2.0 - n * 28.0,
    );
    let frame = CGRect::new(origin, CGSize::new(w, h));

    let this = PinWindow::alloc(mtm).set_ivars(());
    // SAFETY: the designated NSWindow initialiser with valid arguments.
    let window: Retained<PinWindow> = unsafe {
        msg_send![
            super(this),
            initWithContentRect: frame,
            styleMask: NSWindowStyleMask::Borderless,
            backing: NSBackingStoreType::Buffered,
            defer: false
        ]
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setLevel(3); // NSFloatingWindowLevel
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setHasShadow(true);
    window.setMovableByWindowBackground(true);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );

    let view = PinView::alloc(mtm).set_ivars(());
    // SAFETY: `initWithFrame:` is NSImageView's designated initialiser.
    let view: Retained<PinView> = unsafe {
        msg_send![super(view), initWithFrame: CGRect::new(CGPoint::new(0.0, 0.0), frame.size)]
    };
    view.setImage(Some(&image));
    view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    view.setWantsLayer(true);
    if let Some(layer) = view.layer() {
        layer.setCornerRadius(8.0);
        layer.setMasksToBounds(true);
        layer.setBorderWidth(1.0);
        layer.setBorderColor(Some(&CGColor::new_srgb(1.0, 1.0, 1.0, 0.35)));
    }
    window.setContentView(Some(&view));
    window.makeKeyAndOrderFront(None);
    window.makeFirstResponder(Some(&view));
    PINS.with(|p| p.borrow_mut().push(window));
}
