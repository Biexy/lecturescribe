mod execution;
mod feedback;
mod inputs;
mod settings;
mod transcript;

pub use execution::*;
pub use feedback::*;
pub use inputs::*;
pub use settings::*;
pub use transcript::*;

pub const EVENT_SCHEMA_VERSION: u16 = 1;
pub const TRANSCRIPT_SCHEMA_VERSION: u16 = 2;
pub const DEFAULT_MODEL: &str = "gemini-3.1-flash-lite";
