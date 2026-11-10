//! Blinc Platform Abstraction Layer
//!
//! This crate provides platform-agnostic traits and types for windowing,
//! input handling, and application lifecycle management.
//!
//! # Architecture
//!
//! The platform abstraction is built around three main traits:
//!
//! - [`Platform`] - The top-level platform abstraction
//! - [`Window`] - Window management and properties
//! - [`EventLoop`] - Event handling and application lifecycle
//!
//! # Platform Implementations
//!
//! - `blinc_platform_desktop` - Desktop platforms (macOS, Windows, Linux) using winit
//! - `blinc_platform_android` - Android using NDK
//! - `blinc_platform_ios` - iOS using UIKit (planned)
//!
//! # Example
//!
//! ```ignore
//! use blinc_platform::*;
//! use blinc_platform_desktop::DesktopPlatform;
//!
//! fn main() -> Result<(), PlatformError> {
//!     let platform = DesktopPlatform::new()?;
//!     let event_loop = platform.create_event_loop()?;
//!
//!     event_loop.run(|event, window| {
//!         match event {
//!             Event::Frame => {
//!                 // Render frame
//!             }
//!             Event::Window(WindowEvent::CloseRequested) => {
//!                 return ControlFlow::Exit;
//!             }
//!             _ => {}
//!         }
//!         ControlFlow::Continue
//!     })
//! }
//! ```

pub mod assets;
pub mod deep_link;
mod error;
mod event;
mod input;
mod platform;
mod window;

// Re-export all public types
pub use error::{PlatformError, Result};
pub use event::{ControlFlow, Event, EventLoop, LifecycleEvent, WindowEvent};
pub use input::{
    InputEvent, Key, KeyState, KeyboardEvent, Modifiers, MouseButton, MouseEvent, ScrollPhase,
    TouchEvent,
};
pub use platform::Platform;
pub use window::{
    AnimationFps, AnimationThreadMode, Cursor, Window, WindowConfig, WindowId, WindowLevel,
};

// Re-export commonly used asset types
pub use assets::{AssetLoader, AssetPath, FilesystemAssetLoader};

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::assets::{
        AssetLoader, AssetPath, FilesystemAssetLoader, asset_exists, asset_url, load_asset,
        load_asset_string, preload_settled,
    };
    pub use crate::error::{PlatformError, Result};
    pub use crate::event::{ControlFlow, Event, EventLoop, LifecycleEvent, WindowEvent};
    pub use crate::input::{
        InputEvent, Key, KeyState, KeyboardEvent, Modifiers, MouseButton, MouseEvent, ScrollPhase,
        TouchEvent,
    };
    pub use crate::platform::Platform;
    pub use crate::window::{
        AnimationFps, AnimationThreadMode, Cursor, Window, WindowConfig, WindowId, WindowLevel,
    };
}

/// Temporary probe timing.
pub mod probe_timing {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::OnceLock;
    use std::time::Instant;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    static T0: AtomicU64 = AtomicU64::new(0);
    fn now() -> u64 { EPOCH.get_or_init(Instant::now).elapsed().as_micros() as u64 + 1 }
    pub fn key() { if std::env::var_os("BLINC_KEYTIME").is_some() { let n=now(); T0.store(n, Ordering::SeqCst); eprintln!("KEYTIME key t={n}"); } }
    pub fn present() {
        let t = T0.swap(0, Ordering::SeqCst);
        if std::env::var_os("BLINC_KEYTIME").is_some() { let n=now(); eprintln!("KEYTIME present t={n} since_key_us={}", if t!=0 {n-t} else {0}); }
    }
    pub fn stage(name: &str) {
        let t = T0.load(Ordering::SeqCst);
        if t != 0 { eprintln!("KEYTIME   {name} at_us={}", now() - t); }
    }
}
