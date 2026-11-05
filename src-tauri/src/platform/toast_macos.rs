//! Native success/failure toast: a click-through, never-key pill with the mascot drawn from
//! CAShapeLayers and animated with Core Animation. Replaces a permanently loaded webview.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSScreen, NSView, NSWindow, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
use objc2_core_foundation::{CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGColor, CGMutablePath, CGPath};
use objc2_foundation::{NSArray, NSNumber, NSString};
use objc2_quartz_core::{
    CABasicAnimation, CACurrentMediaTime, CAGradientLayer, CAKeyframeAnimation, CALayer,
    CAMediaTiming, CAShapeLayer, CATextLayer, CATransaction, kCAAlignmentLeft,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Ok,
    Err,
}

const WIN_W: f64 = 190.0;
const WIN_H: f64 = 44.0;
const PILL_H: f64 = 30.0;

struct Toast {
    window: Retained<NSWindow>,
    pill: Retained<CAGradientLayer>,
    text: Retained<CATextLayer>,
    hop: Retained<CALayer>,
    happy: Retained<CALayer>,
    sad: Retained<CALayer>,
    eyes_happy: Retained<CAShapeLayer>,
}

thread_local! {
    static TOAST: RefCell<Option<Toast>> = const { RefCell::new(None) };
}

fn rgb(r: f64, g: f64, b: f64) -> objc2_core_foundation::CFRetained<CGColor> {
    CGColor::new_srgb(r, g, b, 1.0)
}

fn stroke(path: &CGPath, width: f64) -> Retained<CAShapeLayer> {
    let l = CAShapeLayer::new();
    l.setFillColor(None);
    l.setStrokeColor(Some(&rgb(0.231, 0.165, 0.42))); // #3b2a6b
    l.setLineWidth(width);
    l.setLineCap(unsafe { objc2_quartz_core::kCALineCapRound });
    l.setPath(Some(path));
    l
}

/// SVG `M x y q cx cy x2 y2` (relative quad) in the 40x40 mascot space.
fn quad(path: &CGMutablePath, x: f64, y: f64, cx: f64, cy: f64, dx: f64, dy: f64) {
    unsafe {
        CGMutablePath::move_to_point(Some(path), std::ptr::null(), x, y);
        CGMutablePath::add_quad_curve_to_point(
            Some(path),
            std::ptr::null(),
            x + cx,
            y + cy,
            x + dx,
            y + dy,
        );
    }
}

fn line(path: &CGMutablePath, x: f64, y: f64, dx: f64, dy: f64) {
    unsafe {
        CGMutablePath::move_to_point(Some(path), std::ptr::null(), x, y);
        CGMutablePath::add_line_to_point(Some(path), std::ptr::null(), x + dx, y + dy);
    }
}

fn ellipse(path: &CGMutablePath, cx: f64, cy: f64, rx: f64, ry: f64) {
    let r = CGRect::new(
        CGPoint::new(cx - rx, cy - ry),
        CGSize::new(rx * 2.0, ry * 2.0),
    );
    unsafe { CGMutablePath::add_ellipse_in_rect(Some(path), std::ptr::null(), r) };
}

fn fill(path: &CGPath, color: &CGColor) -> Retained<CAShapeLayer> {
    let l = CAShapeLayer::new();
    l.setPath(Some(path));
    l.setFillColor(Some(color));
    l
}

fn frame(layer: &CALayer, x: f64, y: f64, w: f64, h: f64) {
    layer.setFrame(CGRect::new(CGPoint::new(x, y), CGSize::new(w, h)));
}

impl Toast {
    fn new(mtm: MainThreadMarker) -> Self {
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(WIN_W, WIN_H)),
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setHasShadow(false);
        window.setIgnoresMouseEvents(true);
        window.setLevel(25); // NSStatusWindowLevel
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        let view = NSView::new(mtm);
        view.setWantsLayer(true);
        window.setContentView(Some(&view));
        let root = view.layer().expect("layer-backed");

        let pill = CAGradientLayer::new();
        pill.setStartPoint(CGPoint::new(0.0, 1.0));
        pill.setEndPoint(CGPoint::new(1.0, 0.0));
        pill.setCornerRadius(PILL_H / 2.0);
        pill.setShadowOpacity(0.35);
        pill.setShadowRadius(6.0);
        pill.setShadowOffset(CGSize::new(0.0, -3.0));
        root.addSublayer(&pill);

        // Mascot in SVG space (40x40, y down), scaled to 24 pt.
        let hop = CALayer::new();
        frame(&hop, 3.0, 3.0, 24.0, 24.0);
        pill.addSublayer(&hop);
        let face = CALayer::new();
        face.setBounds(CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(40.0, 40.0)));
        face.setPosition(CGPoint::new(12.0, 12.0));
        face.setGeometryFlipped(true);
        face.setAffineTransform(CGAffineTransform {
            a: 0.6,
            b: 0.0,
            c: 0.0,
            d: 0.6,
            tx: 0.0,
            ty: 0.0,
        });
        hop.addSublayer(&face);

        let disc = CGMutablePath::new();
        ellipse(&disc, 20.0, 20.0, 19.0, 19.0);
        face.addSublayer(&fill(&disc, &CGColor::new_srgb(1.0, 0.957, 0.984, 1.0)));

        let cheeks = CGMutablePath::new();
        ellipse(&cheeks, 9.5, 24.5, 3.4, 2.1);
        ellipse(&cheeks, 30.5, 24.5, 3.4, 2.1);
        face.addSublayer(&fill(&cheeks, &CGColor::new_srgb(1.0, 0.639, 0.769, 1.0)));

        let happy = CALayer::new();
        happy.setBounds(face.bounds());
        happy.setPosition(CGPoint::new(20.0, 20.0));
        let eyes = CGMutablePath::new();
        quad(&eyes, 11.0, 18.0, 2.6, -3.4, 5.2, 0.0);
        quad(&eyes, 23.8, 18.0, 2.6, -3.4, 5.2, 0.0);
        let eyes_happy = stroke(&eyes, 2.6);
        eyes_happy.setBounds(face.bounds());
        eyes_happy.setPosition(CGPoint::new(20.0, 20.0));
        happy.addSublayer(&eyes_happy);
        let smile = CGMutablePath::new();
        quad(&smile, 15.5, 24.5, 4.5, 5.0, 9.0, 0.0);
        happy.addSublayer(&stroke(&smile, 2.4));
        face.addSublayer(&happy);

        let sad = CALayer::new();
        sad.setBounds(face.bounds());
        sad.setPosition(CGPoint::new(20.0, 20.0));
        let sad_eyes = CGMutablePath::new();
        quad(&sad_eyes, 11.0, 17.5, 2.6, 3.2, 5.2, 0.0);
        quad(&sad_eyes, 23.8, 17.5, 2.6, 3.2, 5.2, 0.0);
        sad.addSublayer(&stroke(&sad_eyes, 2.6));
        let brows = CGMutablePath::new();
        line(&brows, 11.5, 15.2, 4.6, -1.7);
        line(&brows, 28.5, 15.2, -4.6, -1.7);
        sad.addSublayer(&stroke(&brows, 2.0));
        let frown = CGMutablePath::new();
        quad(&frown, 15.5, 28.5, 4.5, -4.6, 9.0, 0.0);
        sad.addSublayer(&stroke(&frown, 2.4));
        let tear = CGMutablePath::new();
        ellipse(&tear, 30.0, 27.5, 2.6, 3.2);
        sad.addSublayer(&fill(&tear, &CGColor::new_srgb(0.424, 0.769, 1.0, 1.0)));
        face.addSublayer(&sad);
        for l in [&face, &happy, &sad] {
            l.setContentsScale(2.0);
        }
        eyes_happy.setContentsScale(2.0);

        let text = CATextLayer::new();
        text.setFontSize(12.0);
        text.setForegroundColor(Some(&rgb(1.0, 1.0, 1.0)));
        text.setAlignmentMode(unsafe { kCAAlignmentLeft });
        text.setContentsScale(2.0);
        pill.addSublayer(&text);

        Toast {
            window,
            pill,
            text,
            hop,
            happy,
            sad,
            eyes_happy,
        }
    }

    fn play(&self, mtm: MainThreadMarker, kind: Kind, message: &str) {
        let err = kind == Kind::Err;
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        let colors: [(f64, f64, f64); 2] = if err {
            [(1.0, 0.604, 0.659), (0.941, 0.278, 0.416)] // #ff9aa8 → #f0476a
        } else {
            [(1.0, 0.541, 0.831), (0.486, 0.361, 1.0)] // #ff8ad4 → #7c5cff
        };
        let cg: Vec<Retained<AnyObject>> = colors
            .iter()
            .map(|&(r, g, b)| {
                let c = CGColor::new_srgb(r, g, b, 1.0);
                // SAFETY: CGColor is a CF type that bridges to an Objective-C object.
                unsafe {
                    Retained::retain(
                        objc2_core_foundation::CFRetained::as_ptr(&c)
                            .as_ptr()
                            .cast::<AnyObject>(),
                    )
                }
                .unwrap()
            })
            .collect();
        let arr = NSArray::from_retained_slice(&cg);
        unsafe { self.pill.setColors(Some(&arr)) };
        let shadow = if err {
            rgb(0.941, 0.278, 0.416)
        } else {
            rgb(0.486, 0.361, 1.0)
        };
        self.pill.setShadowColor(Some(&shadow));
        self.happy.setHidden(err);
        self.sad.setHidden(!err);

        let s = NSString::from_str(message);
        let any: &AnyObject = &s;
        unsafe { self.text.setString(Some(any)) };
        let text_w = self.text.preferredFrameSize().width.ceil() + 2.0;
        let pill_w = 3.0 + 24.0 + 6.0 + text_w + 12.0;
        frame(
            &self.pill,
            ((WIN_W - pill_w) / 2.0).round(),
            (WIN_H - PILL_H) / 2.0,
            pill_w,
            PILL_H,
        );
        frame(&self.text, 33.0, (PILL_H - 15.0) / 2.0, text_w, 16.0);
        self.pill.removeAllAnimations();
        self.hop.removeAllAnimations();
        self.eyes_happy.removeAllAnimations();
        CATransaction::commit();

        // Position: bottom-centre of the primary screen, above the Dock.
        if let Some(screen) = NSScreen::screens(mtm).firstObject() {
            let f = screen.frame();
            let x = f.origin.x + (f.size.width - WIN_W) / 2.0;
            let y = f.origin.y + 100.0;
            self.window.setFrameOrigin(CGPoint::new(x, y));
        }
        self.window.orderFrontRegardless();

        // Pop in, hold, fade out (the caller hides the window afterwards).
        let hold = if err { 1.9 } else { 1.35 };
        self.pill.addAnimation_forKey(
            &keyframes(
                "opacity",
                &[0.0, 1.0, 1.0, 0.0],
                &[0.0, 0.12, 1.0 - 0.2 / hold, 1.0],
                hold,
                0.0,
            ),
            Some(&NSString::from_str("fade")),
        );
        self.pill.addAnimation_forKey(
            &keyframes("transform.scale", &[0.7, 1.0], &[0.0, 1.0], 0.22, 0.0),
            Some(&NSString::from_str("pop")),
        );
        if err {
            self.pill.addAnimation_forKey(
                &keyframes(
                    "transform.translation.x",
                    &[0.0, -4.0, 4.0, -3.0, 2.0, 0.0],
                    &[0.0, 0.2, 0.4, 0.6, 0.8, 1.0],
                    0.45,
                    0.12,
                ),
                Some(&NSString::from_str("shake")),
            );
        } else {
            self.hop.addAnimation_forKey(
                &keyframes(
                    "position.y",
                    &[15.0, 21.0, 14.0, 15.0],
                    &[0.0, 0.45, 0.75, 1.0],
                    0.5,
                    0.1,
                ),
                Some(&NSString::from_str("hop")),
            );
            self.eyes_happy.addAnimation_forKey(
                &keyframes(
                    "transform.scale.y",
                    &[1.0, 0.1, 1.0],
                    &[0.0, 0.5, 1.0],
                    0.22,
                    0.95,
                ),
                Some(&NSString::from_str("blink")),
            );
        }
    }
}

/// A keyframe animation on `key_path` with evenly meaningful `times` (0..1), starting `delay` s from now.
fn keyframes(
    key_path: &str,
    values: &[f64],
    times: &[f64],
    duration: f64,
    delay: f64,
) -> Retained<CAKeyframeAnimation> {
    let a = CAKeyframeAnimation::animationWithKeyPath(Some(&NSString::from_str(key_path)));
    let nums: Vec<Retained<NSNumber>> = values.iter().map(|v| NSNumber::new_f64(*v)).collect();
    let vals: Vec<Retained<AnyObject>> = nums
        .into_iter()
        .map(|n| Retained::into_super(Retained::into_super(n)))
        .map(|o| unsafe { Retained::cast_unchecked(o) })
        .collect();
    let tn: Vec<Retained<NSNumber>> = times.iter().map(|v| NSNumber::new_f64(*v)).collect();
    unsafe {
        a.setValues(Some(&NSArray::from_retained_slice(&vals)));
        a.setKeyTimes(Some(&NSArray::from_retained_slice(&tn)));
    }
    a.setDuration(duration);
    if delay > 0.0 {
        a.setBeginTime(CACurrentMediaTime() + delay);
        a.setFillMode(unsafe { objc2_quartz_core::kCAFillModeBackwards });
    }
    a
}

/// Show (or restart) the toast. Main thread only.
pub(crate) fn show(mtm: MainThreadMarker, kind: Kind, message: &str) {
    TOAST.with(|t| {
        let mut t = t.borrow_mut();
        let toast = t.get_or_insert_with(|| Toast::new(mtm));
        toast.play(mtm, kind, message);
    });
}

/// Hide the toast window (it keeps its layers for next time). Main thread only.
pub(crate) fn hide() {
    TOAST.with(|t| {
        if let Some(toast) = t.borrow().as_ref() {
            toast.window.orderOut(None);
        }
    });
}

#[allow(dead_code)]
fn _basic_animation_is_available(_: &CABasicAnimation) {}
