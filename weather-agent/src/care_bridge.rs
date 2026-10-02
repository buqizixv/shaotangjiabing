//! Explicit requests from the weather module to the shell's Rinx messenger.
//! The shell verifies the originating module and routes the request through
//! Rinx's own confirmation and Matrix account checks.

#[derive(Clone, Debug)]
pub enum CareBridgeRequest {
    ListRooms,
    SendMessage { room: String, text: String, event_key: Option<String> },
}
