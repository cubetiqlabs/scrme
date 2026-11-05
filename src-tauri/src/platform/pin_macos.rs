//! Floating screenshot windows (native NSWindow + NSImageView, no webview).
//!
//! Two flavours share one implementation:
//! - **preview**: small, bottom-right, auto-dismisses after a few seconds unless hovered
//!   (shown after every capture);
//! - **pin**: a persistent, resizable copy (editor's Pin button, or "keep" on a preview).
//!
//! Both have a slim footer with icon buttons (edit, copy, save as, keep, close). Drag to
//! move, scroll/pinch to resize, Esc / double-click / right-click closes.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel,
};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSBackingStoreType, NSButton, NSColor, NSDragOperation,
    NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource, NSEvent, NSImage,
    NSImageScaling, NSImageSymbolConfiguration, NSImageView, NSScreen, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGColor;
use objc2_foundation::{NSArray, NSData, NSObjectProtocol, NSPoint, NSString, NSTimer, NSURL};

/// What the user asked for on a pin/preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Edit,
    Copy,
    SaveAs,
    Close,
}

pub(crate) struct Options {
    /// Seconds before a preview dismisses itself (paused while hovered); `None` = stays.
    pub(crate) auto_dismiss: Option<f64>,
    /// Fit inside this many points (preview thumbnail); `None` = up to 70% of the screen.
    pub(crate) max_points: Option<(f64, f64)>,
    /// Bottom-right corner (preview) instead of centred with a cascade (pin).
    pub(crate) corner: bool,
    /// Makes the picture draggable into other apps (as this file); the window then no longer
    /// moves by dragging. Called when a drag starts.
    pub(crate) drag: Option<Box<dyn Fn() -> Option<PathBuf>>>,
}

struct Ivars {
    on_action: Box<dyn Fn(Action)>,
    hover: Cell<bool>,
    remaining: Cell<f64>,
    auto: Cell<bool>,
    timer: RefCell<Option<Retained<NSTimer>>>,
    drag: Option<Box<dyn Fn() -> Option<PathBuf>>>,
    drag_started: Cell<bool>,
}

const FOOTER_H: f64 = 28.0;

define_class!(
    // SAFETY: NSWindow has no subclassing requirements; only `canBecomeKeyWindow` is overridden.
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
    #[ivars = Ivars]
    struct PinView;

    impl PinView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            self.ivars().drag.is_none() // pins move by dragging; previews are dragged *out*
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            if self.ivars().drag.is_none() {
                // SAFETY: forwards to the superclass implementation (window dragging).
                return unsafe { msg_send![super(self), mouseDragged: event] };
            }
            if !self.ivars().drag_started.replace(true) {
                self.begin_drag(event);
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.ivars().drag_started.set(false);
            // SAFETY: forwards to the superclass implementation.
            unsafe { msg_send![super(self), mouseUp: event] }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if event.clickCount() >= 2 {
                self.dismiss(Action::Close);
            } else {
                // SAFETY: forwards to the superclass implementation (window dragging).
                unsafe { msg_send![super(self), mouseDown: event] }
            }
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, _event: &NSEvent) {
            self.dismiss(Action::Close);
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if event.keyCode() == 53 {
                self.dismiss(Action::Close); // Esc
            }
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            self.keep();
            self.resize_by(1.0 + event.scrollingDeltaY() * 0.004);
        }

        #[unsafe(method(magnifyWithEvent:))]
        fn magnify(&self, event: &NSEvent) {
            self.keep();
            self.resize_by(1.0 + event.magnification());
        }

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            // SAFETY: standard NSView override; call super first.
            let _: () = unsafe { msg_send![super(self), updateTrackingAreas] };
            let options = NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::ActiveAlways
                | NSTrackingAreaOptions::InVisibleRect;
            let area = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    self.bounds(),
                    options,
                    Some(self),
                    None,
                )
            };
            self.addTrackingArea(&area);
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            self.ivars().hover.set(true);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.ivars().hover.set(false);
        }

        // Footer buttons (target = this view)
        #[unsafe(method(scrmeEdit:))]
        fn edit_clicked(&self, _sender: Option<&AnyObject>) {
            self.dismiss(Action::Edit);
        }

        #[unsafe(method(scrmeCopy:))]
        fn copy_clicked(&self, _sender: Option<&AnyObject>) {
            (self.ivars().on_action)(Action::Copy);
        }

        #[unsafe(method(scrmeSave:))]
        fn save_clicked(&self, _sender: Option<&AnyObject>) {
            self.keep(); // the save panel is modal; don't vanish underneath it
            (self.ivars().on_action)(Action::SaveAs);
        }

        #[unsafe(method(scrmeKeep:))]
        fn keep_clicked(&self, _sender: Option<&AnyObject>) {
            self.keep();
        }

        #[unsafe(method(scrmeClose:))]
        fn close_clicked(&self, _sender: Option<&AnyObject>) {
            self.dismiss(Action::Close);
        }
    }

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for PinView {}

    // SAFETY: the two methods match NSDraggingSource's required/optional signatures.
    unsafe impl NSDraggingSource for PinView {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn source_operation_mask(&self, _session: &NSDraggingSession, _context: NSDraggingContext) -> NSDragOperation {
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn drag_ended(&self, _session: &NSDraggingSession, _at: NSPoint, operation: NSDragOperation) {
            self.ivars().drag_started.set(false);
            if operation != NSDragOperation::None {
                self.dismiss(Action::Close); // dropped somewhere: the preview has done its job
            }
        }
    }
);

thread_local! {
    /// Every open window (main thread only) so they stay alive until closed.
    static OPEN: RefCell<Vec<Retained<PinWindow>>> = const { RefCell::new(Vec::new()) };
    /// A closed window is parked, not freed on the spot: dismiss runs inside the view's own
    /// event handler and releasing a view while its method runs would be a use-after-free.
    static PARKED: RefCell<Vec<Retained<PinWindow>>> = const { RefCell::new(Vec::new()) };
}

impl PinView {
    /// Drag the capture out as a file (uploads, chat, design tools…), like the macOS thumbnail.
    fn begin_drag(&self, event: &NSEvent) {
        let Some(path) = self.ivars().drag.as_ref().and_then(|f| f()) else {
            return;
        };
        self.keep(); // never vanish mid-drag
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let writer = ProtocolObject::from_ref(&*url);
        let item = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), writer);
        let image = self.image();
        // SAFETY: the dragging image is just the thumbnail shown in this view.
        unsafe {
            item.setDraggingFrame_contents(self.bounds(), image.as_deref().map(|i| i as &AnyObject))
        };
        let items = NSArray::from_slice(&[&*item]);
        self.beginDraggingSessionWithItems_event_source(
            &items,
            event,
            ProtocolObject::from_ref(self),
        );
    }

    /// Stop auto-dismissal (the preview becomes a normal pin).
    fn keep(&self) {
        let iv = self.ivars();
        iv.auto.set(false);
        if let Some(t) = iv.timer.borrow_mut().take() {
            t.invalidate();
        }
    }

    /// Close the window, then report what the user chose.
    fn dismiss(&self, action: Action) {
        self.keep();
        let Some(win) = self.window() else { return };
        win.orderOut(None);
        OPEN.with(|p| {
            let mut open = p.borrow_mut();
            if let Some(i) = open
                .iter()
                .position(|w| std::ptr::eq(&***w as *const NSWindow, &*win))
            {
                let w = open.remove(i);
                w.setContentView(None); // drop the picture now; the empty shell is freed later
                PARKED.with(|k| k.borrow_mut().push(w));
            }
        });
        (self.ivars().on_action)(action);
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

/// A slim translucent bar with the icon buttons, pinned to the bottom edge.
fn footer(mtm: MainThreadMarker, target: &PinView, width: f64) -> Retained<NSView> {
    let bar = NSView::initWithFrame(
        NSView::alloc(mtm),
        CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width, FOOTER_H)),
    );
    bar.setWantsLayer(true);
    if let Some(l) = bar.layer() {
        l.setBackgroundColor(Some(&CGColor::new_srgb(0.09, 0.07, 0.15, 0.72)));
    }
    bar.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMaxYMargin,
    );

    let items: [(&str, &str, objc2::runtime::Sel); 5] = [
        ("pencil", "Edit", sel!(scrmeEdit:)),
        ("doc.on.doc", "Copy", sel!(scrmeCopy:)),
        ("square.and.arrow.down", "Save as…", sel!(scrmeSave:)),
        ("pin", "Keep on screen", sel!(scrmeKeep:)),
        ("xmark", "Close", sel!(scrmeClose:)),
    ];
    let (bw, gap) = (24.0, 6.0);
    let total = items.len() as f64 * bw + (items.len() as f64 - 1.0) * gap;
    let mut x = ((width - total) / 2.0).max(4.0);
    for (symbol, tip, action) in items {
        let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol),
            Some(&NSString::from_str(tip)),
        )
        // small, evenly weighted glyphs (an unconfigured symbol fills the whole button)
        .and_then(|i| {
            i.imageWithSymbolConfiguration(
                &NSImageSymbolConfiguration::configurationWithPointSize_weight(12.0, 0.23),
            )
        });
        let button = unsafe {
            NSButton::buttonWithImage_target_action(
                &image.unwrap_or_default(),
                Some(target),
                Some(action),
                mtm,
            )
        };
        button.setImageScaling(NSImageScaling::ScaleNone);
        button.setBordered(false);
        button.setContentTintColor(Some(&NSColor::whiteColor()));
        button.setToolTip(Some(&NSString::from_str(tip)));
        button.setFrame(CGRect::new(
            CGPoint::new(x, (FOOTER_H - 20.0) / 2.0),
            CGSize::new(bw, 20.0),
        ));
        bar.addSubview(&button);
        x += bw + gap;
    }
    bar
}

/// Show a floating screenshot. `tiff` is what is displayed, `px` the size of the *original*
/// image in pixels (it decides the on-screen size). Main thread only.
pub(crate) fn show(
    mtm: MainThreadMarker,
    tiff: Vec<u8>,
    px: (u32, u32),
    opts: Options,
    on_action: impl Fn(Action) + 'static,
) {
    PARKED.with(|k| k.borrow_mut().clear());
    let data = NSData::from_vec(tiff);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    let screen = NSScreen::screens(mtm).firstObject();
    let full = screen.as_ref().map_or(
        CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1440.0, 900.0)),
        |s| s.frame(),
    );
    let visible = screen.as_ref().map_or(full, |s| s.visibleFrame());
    let scale = screen.as_ref().map_or(2.0, |s| s.backingScaleFactor());

    // Pixels → points, then fit the box.
    let points = (px.0 as f64 / scale, px.1 as f64 / scale);
    let (bw, bh) = opts
        .max_points
        .unwrap_or((full.size.width * 0.7, full.size.height * 0.7));
    let k = (bw / points.0).min(bh / points.1).min(1.0);
    let (w, h) = ((points.0 * k).max(160.0), (points.1 * k).max(90.0));
    let n = OPEN.with(|p| p.borrow().len()) as f64;
    let origin = if opts.corner {
        CGPoint::new(
            visible.origin.x + visible.size.width - w - 20.0,
            visible.origin.y + 20.0,
        )
    } else {
        CGPoint::new(
            full.origin.x + (full.size.width - w) / 2.0 + n * 28.0,
            full.origin.y + (full.size.height - h) / 2.0 - n * 28.0,
        )
    };
    let frame = CGRect::new(origin, CGSize::new(w, h));

    let this = PinWindow::alloc(mtm).set_ivars(());
    // SAFETY: the designated NSWindow initialiser with valid arguments.
    let window: Retained<PinWindow> = unsafe {
        msg_send![super(this), initWithContentRect: frame, styleMask: NSWindowStyleMask::Borderless, backing: NSBackingStoreType::Buffered, defer: false]
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

    let auto = opts.auto_dismiss;
    let view = PinView::alloc(mtm).set_ivars(Ivars {
        on_action: Box::new(on_action),
        hover: Cell::new(false),
        remaining: Cell::new(auto.unwrap_or(0.0)),
        auto: Cell::new(auto.is_some()),
        timer: RefCell::new(None),
        drag: opts.drag,
        drag_started: Cell::new(false),
    });
    // SAFETY: `initWithFrame:` is NSImageView's designated initialiser.
    let view: Retained<PinView> = unsafe {
        msg_send![super(view), initWithFrame: CGRect::new(CGPoint::new(0.0, 0.0), frame.size)]
    };
    view.setImage(Some(&image));
    view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    view.setWantsLayer(true);
    if let Some(layer) = view.layer() {
        layer.setCornerRadius(10.0);
        layer.setMasksToBounds(true);
        layer.setBorderWidth(1.0);
        layer.setBorderColor(Some(&CGColor::new_srgb(1.0, 1.0, 1.0, 0.35)));
    }
    view.addSubview(&footer(mtm, &view, w));

    if auto.is_some() {
        // Count down while not hovered; the block holds a strong ref until `keep` invalidates it.
        let v = view.clone();
        let tick = RcBlock::new(move |_timer: std::ptr::NonNull<NSTimer>| {
            let iv = v.ivars();
            if !iv.auto.get() {
                return;
            }
            if !iv.hover.get() {
                iv.remaining.set(iv.remaining.get() - 0.25);
            }
            if iv.remaining.get() <= 0.0 {
                v.dismiss(Action::Close);
            }
        });
        let timer =
            unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(0.25, true, &tick) };
        *view.ivars().timer.borrow_mut() = Some(timer);
    }

    window.setContentView(Some(&view));
    if opts.corner {
        window.orderFrontRegardless(); // a preview must not steal focus from what you're doing
    } else {
        window.makeKeyAndOrderFront(None);
        window.makeFirstResponder(Some(&view));
    }
    OPEN.with(|p| p.borrow_mut().push(window));
}

/// Close every preview/pin that is still auto-dismissing (a new capture replaces the old preview).
pub(crate) fn dismiss_previews() {
    let views: Vec<Retained<PinView>> = OPEN.with(|p| {
        p.borrow()
            .iter()
            .filter_map(|w| w.contentView())
            .map(|v| unsafe { Retained::cast_unchecked::<PinView>(v) })
            .filter(|v| v.ivars().auto.get())
            .collect()
    });
    for v in views {
        v.dismiss(Action::Close);
    }
}
