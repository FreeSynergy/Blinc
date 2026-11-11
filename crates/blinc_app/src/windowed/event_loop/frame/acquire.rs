//! Taking the swapchain image for a frame that is going to be drawn.
//!
//! The image is taken only after the frame is known to be drawn, and the caller presents every
//! image it receives: an image that is acquired and neither presented nor returned stays held.

use blinc_platform::ControlFlow;
use std::time::Duration;

/// Proof that the frame passed the draw decision; `acquire_for_draw` needs one.
pub(super) struct DrawToken(());

pub(super) enum Decision {
    /// The frame is not drawn; leave with this flow.
    Skip(ControlFlow),
    Draw(DrawToken),
}

impl Decision {
    /// `skip` is what the rebuild, build and animation phases returned: `Some` ends the frame.
    pub(super) fn after_phases(skip: Option<ControlFlow>) -> Self {
        match skip {
            Some(flow) => Decision::Skip(flow),
            None => Decision::Draw(DrawToken(())),
        }
    }
}

/// A swapchain the frame loop draws into.
pub(super) trait Swapchain {
    type Image;
    fn acquire(&mut self) -> Result<Self::Image, AcquireError>;
    fn reconfigure(&mut self);
}

/// How the loop asks for a frame again.
pub(super) trait FrameSignals {
    /// The next frame has something to draw even if no input arrives.
    fn mark_dirty(&mut self);
    /// Wake the loop for a redraw after `delay`.
    fn redraw_after(&mut self, delay: Duration);
}

#[derive(Debug)]
pub(super) enum AcquireError {
    Lost,
    Outdated,
    Timeout,
    OutOfMemory,
    Other(String),
}

const RETRY_FIRST: Duration = Duration::from_millis(16);
const RETRY_MAX: Duration = Duration::from_millis(250);

/// Delay before the retry after `failures` consecutive failed acquires (at least 1).
fn retry_delay(failures: u32) -> Duration {
    RETRY_FIRST
        .saturating_mul(1u32 << (failures - 1).min(8))
        .min(RETRY_MAX)
}

pub(super) enum Acquired<I> {
    Image(I),
    /// No image this frame; a new frame has been asked for.
    Retry,
    Exit,
}

/// `failures` counts consecutive failed acquires of the window; a success resets it.
pub(super) fn acquire_for_draw<S: Swapchain, G: FrameSignals>(
    _decided: DrawToken,
    sc: &mut S,
    signals: &mut G,
    failures: &mut u32,
) -> Acquired<S::Image> {
    match sc.acquire() {
        Ok(image) => {
            *failures = 0;
            Acquired::Image(image)
        }
        Err(AcquireError::OutOfMemory) => Acquired::Exit,
        Err(e) => {
            *failures = failures.saturating_add(1);
            // The first failure and every 64th after it, so a persistent error does not flood the log.
            if *failures % 64 == 1 {
                tracing::warn!("swapchain acquire failed ({} in a row): {:?}", *failures, e);
            }
            if !matches!(e, AcquireError::Other(_)) {
                sc.reconfigure();
            }
            // The dirty flags of this frame are already consumed.
            signals.mark_dirty();
            signals.redraw_after(retry_delay(*failures));
            Acquired::Retry
        }
    }
}

/// The window's wgpu surface.
pub(super) struct SurfaceSwapchain<'a> {
    pub surface: &'a wgpu::Surface<'static>,
    pub device: &'a wgpu::Device,
    pub config: &'a wgpu::SurfaceConfiguration,
}

impl Swapchain for SurfaceSwapchain<'_> {
    type Image = wgpu::SurfaceTexture;

    fn acquire(&mut self) -> Result<Self::Image, AcquireError> {
        self.surface.get_current_texture().map_err(|e| match e {
            wgpu::SurfaceError::Lost => AcquireError::Lost,
            wgpu::SurfaceError::Outdated => AcquireError::Outdated,
            wgpu::SurfaceError::Timeout => AcquireError::Timeout,
            wgpu::SurfaceError::OutOfMemory => AcquireError::OutOfMemory,
            e => AcquireError::Other(e.to_string()),
        })
    }

    fn reconfigure(&mut self) {
        self.surface.configure(self.device, self.config);
    }
}

/// The loop's dirty flag and wake timer.
pub(super) struct WindowSignals<'a> {
    pub wake: &'a blinc_platform_desktop::WakeProxy,
    pub dirty: &'a std::sync::atomic::AtomicBool,
}

impl FrameSignals for WindowSignals<'_> {
    fn mark_dirty(&mut self) {
        self.dirty.store(true, std::sync::atomic::Ordering::Release);
    }

    fn redraw_after(&mut self, delay: Duration) {
        self.wake.wake_at(delay);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        next: Vec<Result<u32, AcquireError>>,
        acquired: u32,
        reconfigured: u32,
    }

    impl Swapchain for Fake {
        type Image = u32;
        fn acquire(&mut self) -> Result<u32, AcquireError> {
            let r = self.next.remove(0);
            if r.is_ok() {
                self.acquired += 1;
            }
            r
        }
        fn reconfigure(&mut self) {
            self.reconfigured += 1;
        }
    }

    #[derive(Default)]
    struct Signals {
        dirty: u32,
        delays: Vec<Duration>,
    }

    impl FrameSignals for Signals {
        fn mark_dirty(&mut self) {
            self.dirty += 1;
        }
        fn redraw_after(&mut self, delay: Duration) {
            self.delays.push(delay);
        }
    }

    fn token() -> DrawToken {
        let Decision::Draw(token) = Decision::after_phases(None) else {
            unreachable!()
        };
        token
    }

    /// One frame as the loop runs it: the decision comes first and the only way to an acquire.
    fn frame(
        sc: &mut Fake,
        sig: &mut Signals,
        skip: Option<ControlFlow>,
        failures: &mut u32,
        presented: &mut u32,
    ) {
        let Decision::Draw(token) = Decision::after_phases(skip) else {
            return;
        };
        if let Acquired::Image(_) = acquire_for_draw(token, sc, sig, failures) {
            *presented += 1;
        }
    }

    #[test]
    fn skipped_frames_take_no_image() {
        let mut sc = Fake {
            next: vec![Ok(1), Ok(2)],
            ..Default::default()
        };
        let mut sig = Signals::default();
        let (mut failures, mut presented) = (0, 0);
        for _ in 0..50 {
            frame(
                &mut sc,
                &mut sig,
                Some(ControlFlow::Continue),
                &mut failures,
                &mut presented,
            );
        }
        frame(&mut sc, &mut sig, None, &mut failures, &mut presented);
        for _ in 0..50 {
            frame(
                &mut sc,
                &mut sig,
                Some(ControlFlow::Continue),
                &mut failures,
                &mut presented,
            );
        }
        frame(&mut sc, &mut sig, None, &mut failures, &mut presented);
        assert_eq!(sc.acquired, 2);
        assert_eq!(presented, sc.acquired);
        assert!(sig.delays.is_empty());
    }

    #[test]
    fn a_skip_keeps_its_flow() {
        assert!(matches!(
            Decision::after_phases(Some(ControlFlow::Exit)),
            Decision::Skip(ControlFlow::Exit)
        ));
        assert!(matches!(Decision::after_phases(None), Decision::Draw(_)));
    }

    #[test]
    fn a_failed_acquire_does_not_lose_the_frame() {
        for (err, reconfigures) in [
            (AcquireError::Timeout, 1),
            (AcquireError::Lost, 1),
            (AcquireError::Outdated, 1),
            (AcquireError::Other("x".into()), 0),
        ] {
            let mut sc = Fake {
                next: vec![Err(err)],
                ..Default::default()
            };
            let mut sig = Signals::default();
            let mut failures = 0;
            assert!(matches!(
                acquire_for_draw(token(), &mut sc, &mut sig, &mut failures),
                Acquired::Retry
            ));
            assert_eq!(sc.reconfigured, reconfigures);
            assert_eq!((sig.dirty, sig.delays.len(), failures), (1, 1, 1));
        }
    }

    #[test]
    fn a_persistent_error_backs_off_and_a_success_resets() {
        let mut sc = Fake {
            next: (0..8)
                .map(|_| Err(AcquireError::Lost))
                .chain([Ok(1), Err(AcquireError::Lost)])
                .collect(),
            ..Default::default()
        };
        let mut sig = Signals::default();
        let mut failures = 0;
        for _ in 0..8 {
            acquire_for_draw(token(), &mut sc, &mut sig, &mut failures);
        }
        let ms: Vec<u128> = sig.delays.iter().map(|d| d.as_millis()).collect();
        assert_eq!(ms, [16, 32, 64, 128, 250, 250, 250, 250]);
        assert!(matches!(
            acquire_for_draw(token(), &mut sc, &mut sig, &mut failures),
            Acquired::Image(1)
        ));
        assert_eq!(failures, 0);
        acquire_for_draw(token(), &mut sc, &mut sig, &mut failures);
        assert_eq!(sig.delays.last().unwrap().as_millis(), 16);
    }

    #[test]
    fn out_of_memory_exits() {
        let mut sc = Fake {
            next: vec![Err(AcquireError::OutOfMemory)],
            ..Default::default()
        };
        let mut sig = Signals::default();
        let mut failures = 0;
        assert!(matches!(
            acquire_for_draw(token(), &mut sc, &mut sig, &mut failures),
            Acquired::Exit
        ));
        assert!(sig.delays.is_empty());
    }
}
