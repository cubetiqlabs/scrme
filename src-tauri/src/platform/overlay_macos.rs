//! Native area-select overlay: a borderless window showing the frozen screenshot (the
//! `CGImage` is the layer contents, no pixel copy), a dim layer with a cut-out for the
//! selection, a border and a size label. No webview, no JS, no helper processes.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSCursor, NSEvent, NSEventModifierFlags, NSScreen, NSView,
    NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGColor, CGImage, CGMutablePath};
use objc2_foundation::NSString;
use objc2_quartz_core::{CALayer, CAShapeLayer, CATextLayer, CATransaction, kCAFillRuleEvenOdd};

use crate::capture::geometry::{RectF, drag_rect, view_to_pixels};

/// What the user chose; pixel rect in the frozen frame (top-left origin).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Selection {
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) w: i64,
    pub(crate) h: i64,
    /// ⇧ held on release → open the editor.
    pub(crate) edit: bool,
}

/// `CGImage` is immutable and thread-safe; wrapped so it can cross to the main thread.
pub(crate) struct SendImage(pub(crate) CFRetained<CGImage>);
// SAFETY: see above (Apple documents CGImage as thread-safe).
unsafe impl Send for SendImage {}

type Done = Box<dyn Fn(Option<Selection>)>;

struct Ivars {
    done: Done,
    scale: f64,
    drag: RefCell<Option<(f64, f64)>>,
    dim: Retained<CAShapeLayer>,
    border: Retained<CAShapeLayer>,
    label: Retained<CATextLayer>,
}

define_class!(
    // SAFETY: NSWindow has no subclassing requirements; we only override `canBecomeKeyWindow`.
    #[unsafe(super = NSWindow)]
    #[thread_kind = MainThreadOnly]
    #[name = "ScrmeOverlayWindow"]
    struct OverlayWindow;

    impl OverlayWindow {
        // Borderless windows refuse key status by default; we need it for Esc.
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool {
            true
        }
    }
);

define_class!(
    // SAFETY: NSView has no subclassing requirements; `Ivars` has no Drop impl issues.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[name = "ScrmeOverlayView"]
    #[ivars = Ivars]
    struct OverlayView;

    impl OverlayView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            let cursor = NSCursor::crosshairCursor();
            self.addCursorRect_cursor(self.bounds(), &cursor);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let p = self.point_of(event);
            *self.ivars().drag.borrow_mut() = Some(p);
            self.show_selection(None);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let Some(start) = *self.ivars().drag.borrow() else { return };
            let b = self.bounds().size;
            let r = drag_rect(start, self.point_of(event), b.width, b.height);
            self.show_selection(Some(r));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            let Some(start) = self.ivars().drag.borrow_mut().take() else { return };
            let b = self.bounds().size;
            let r = drag_rect(start, self.point_of(event), b.width, b.height);
            let outcome = (r.w >= 2.0 && r.h >= 2.0).then(|| {
                let (x, y, w, h) = view_to_pixels(r, b.height, self.ivars().scale);
                let edit = event.modifierFlags().contains(NSEventModifierFlags::Shift);
                Selection { x, y, w, h, edit }
            });
            (self.ivars().done)(outcome);
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if event.keyCode() == 53 {
                // Esc
                (self.ivars().done)(None);
            }
        }
    }
);

impl OverlayWindow {
    fn new(mtm: MainThreadMarker, frame: CGRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: the designated NSWindow initialiser, called with valid arguments.
        unsafe {
            msg_send![
                super(this),
                initWithContentRect: frame,
                styleMask: NSWindowStyleMask::Borderless,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ]
        }
    }
}

impl OverlayView {
    fn new(
        mtm: MainThreadMarker,
        frame: CGRect,
        image: &CGImage,
        scale: f64,
        done: Done,
    ) -> Retained<Self> {
        let sublayer = |l: &CALayer| l.setContentsScale(scale);
        let dim = CAShapeLayer::new();
        dim.setFillRule(unsafe { kCAFillRuleEvenOdd });
        dim.setFillColor(Some(&CGColor::new_srgb(0.0, 0.0, 0.0, 0.4)));
        let border = CAShapeLayer::new();
        border.setFillColor(None);
        border.setStrokeColor(Some(&CGColor::new_srgb(1.0, 1.0, 1.0, 1.0)));
        border.setLineWidth(1.0);
        let label = CATextLayer::new();
        label.setFontSize(11.0);
        label.setForegroundColor(Some(&CGColor::new_srgb(1.0, 1.0, 1.0, 1.0)));
        label.setBackgroundColor(Some(&CGColor::new_srgb(0.486, 0.361, 1.0, 0.93)));
        label.setCornerRadius(9.0);
        label.setAlignmentMode(unsafe { objc2_quartz_core::kCAAlignmentCenter });
        label.setHidden(true);
        for l in [dim.as_super(), border.as_super(), label.as_super()] {
            sublayer(l);
        }

        let this = Self::alloc(mtm).set_ivars(Ivars {
            done,
            scale,
            drag: RefCell::new(None),
            dim,
            border,
            label,
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        this.setWantsLayer(true);
        let root = this.layer().expect("layer-backed view");
        // The frozen frame: CALayer.contents takes a CGImage directly (zero copy).
        let picture = CALayer::new();
        picture.setFrame(CGRect::new(CGPoint::new(0.0, 0.0), frame.size));
        let any: &AnyObject = unsafe { &*(image as *const CGImage as *const AnyObject) };
        unsafe { picture.setContents(Some(any)) };
        picture.setContentsScale(scale);
        root.addSublayer(&picture);
        root.addSublayer(&this.ivars().dim);
        root.addSublayer(&this.ivars().border);
        root.addSublayer(&this.ivars().label);
        this.show_selection(None);
        this
    }

    /// Point in view coordinates (origin bottom-left, points).
    fn point_of(&self, event: &NSEvent) -> (f64, f64) {
        let p = self.convertPoint_fromView(event.locationInWindow(), None);
        (p.x, p.y)
    }

    /// Redraw dim cut-out, border and size label without implicit animations.
    fn show_selection(&self, sel: Option<RectF>) {
        let iv = self.ivars();
        let b = self.bounds();
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        let path = CGMutablePath::new();
        unsafe { CGMutablePath::add_rect(Some(&path), std::ptr::null(), b) };
        match sel {
            Some(r) => {
                let rect = CGRect::new(CGPoint::new(r.x, r.y), CGSize::new(r.w, r.h));
                unsafe { CGMutablePath::add_rect(Some(&path), std::ptr::null(), rect) };
                let outline = CGMutablePath::new();
                unsafe { CGMutablePath::add_rect(Some(&outline), std::ptr::null(), rect) };
                iv.border.setPath(Some(&outline));
                let text = NSString::from_str(&format!(
                    "{} × {}",
                    (r.w * iv.scale).round(),
                    (r.h * iv.scale).round()
                ));
                let text: &AnyObject = &text;
                unsafe { iv.label.setString(Some(text)) };
                let y = if r.y + r.h + 26.0 < b.size.height {
                    r.y + r.h + 4.0
                } else {
                    r.y + r.h - 22.0
                };
                iv.label
                    .setFrame(CGRect::new(CGPoint::new(r.x, y), CGSize::new(80.0, 18.0)));
                iv.label.setHidden(false);
            }
            None => {
                iv.border.setPath(None);
                iv.label.setHidden(true);
            }
        }
        iv.dim.setPath(Some(&path));
        CATransaction::commit();
    }
}

thread_local! {
    /// The window currently on screen (main thread only).
    static ACTIVE: RefCell<Option<Retained<OverlayWindow>>> = const { RefCell::new(None) };
    /// A dismissed window is parked here instead of being released on the spot: `dismiss`
    /// is called from inside the view's own `mouseUp:`/`keyDown:`, and freeing a view
    /// while one of its methods is still running would be a use-after-free.
    static PARKED: RefCell<Option<Retained<OverlayWindow>>> = const { RefCell::new(None) };
}

/// Hide the overlay now and drop its heavy contents (the frozen image); the empty window
/// shell is released at the next `show`. Main thread only.
pub(crate) fn dismiss() {
    let Some(w) = ACTIVE.with(|a| a.borrow_mut().take()) else {
        return;
    };
    w.orderOut(None);
    if let Some(layer) = w.contentView().and_then(|v| v.layer()) {
        // SAFETY: removing sublayers (incl. the CGImage-backed one) is always valid.
        unsafe { layer.setSublayers(None) };
    }
    PARKED.with(|p| *p.borrow_mut() = Some(w));
}

/// Show the overlay on the primary screen. `done` runs on the main thread once, with the
/// selection or `None` for cancel; the overlay is already dismissed by then.
pub(crate) fn show(
    mtm: MainThreadMarker,
    image: &CGImage,
    done: impl Fn(Option<Selection>) + 'static,
) {
    dismiss();
    PARKED.with(|p| p.borrow_mut().take()); // release the previous (hidden, empty) shell
    let Some(screen) = NSScreen::screens(mtm).firstObject() else {
        return;
    };
    let frame = screen.frame();
    let scale = screen.backingScaleFactor();
    let window = OverlayWindow::new(mtm, frame);
    unsafe { window.setReleasedWhenClosed(false) };
    window.setLevel(1000); // NSScreenSaverWindowLevel: above the menu bar and Dock
    window.setOpaque(true);
    window.setHasShadow(false);
    window.setBackgroundColor(Some(&NSColor::blackColor()));
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    let view = OverlayView::new(
        mtm,
        CGRect::new(CGPoint::new(0.0, 0.0), frame.size),
        image,
        scale,
        Box::new(move |outcome| {
            dismiss();
            done(outcome);
        }),
    );
    window.setContentView(Some(&view));
    window.makeKeyAndOrderFront(None);
    window.makeFirstResponder(Some(&view));
    window.invalidateCursorRectsForView(&view);
    ACTIVE.with(|a| *a.borrow_mut() = Some(window));
}
