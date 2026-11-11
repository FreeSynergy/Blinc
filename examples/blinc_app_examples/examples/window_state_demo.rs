//! Prints the window state on every UI build.
//!
//! ```bash
//! cargo run -p blinc_app_examples --example window_state_demo
//! ```

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedApp;

fn main() -> Result<()> {
    WindowedApp::run(WindowConfig::default(), |ctx| {
        let s = ctx.window_state();
        println!(
            "STATE maximized={} fullscreen={} inner={}x{} outer={}x{} scale={} focused={} normal={:?}",
            s.maximized,
            s.fullscreen,
            s.inner_size.0,
            s.inner_size.1,
            s.outer_size.0,
            s.outer_size.1,
            s.scale_factor,
            s.focused,
            s.normal_size_logical(),
        );
        div()
            .w(ctx.width)
            .h(ctx.height)
            .bg(Color::rgb(0.1, 0.1, 0.15))
            .justify_center()
            .items_center()
            .child(text("window state").size(32.0).color(Color::WHITE))
    })
}
