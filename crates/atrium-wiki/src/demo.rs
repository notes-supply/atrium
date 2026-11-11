//! The design's invented wiki, served by a stand-in for Mneme in this process, so the design
//! can be looked at, checked and captured without a Mneme.

use jiff::Timestamp;

use crate::stub::{self, Stub};

/// The demo's now: Wednesday 11 November 2026 at 09:41 in Stockholm.
pub const NOW: Timestamp = Timestamp::constant(1_794_386_460, 0);
/// The demo reader's last visit, at 07:48.
pub const LAST_VISIT: i64 = 1_794_379_680;

/// Starts the stand-in and returns its base URL. With `away`, nothing answers there.
///
/// # Errors
///
/// Fails when no local port can be bound.
pub async fn serve(away: bool) -> Result<String, std::io::Error> {
    let running = stub::serve(if away { Stub::Down } else { Stub::Fixtures }).await;
    Ok(running.base)
}
