use std::fmt;

/// Exit codes returned by every command function.
///
/// `UserError`    (1) — bad input, missing branch, dirty tree, etc.
/// `ConflictExit` (2) — rebase hit a conflict and paused; user must resolve
/// `AbortExit`    (3) — `rebase --abort` completed; branches restored
pub enum CmdError {
    /// Generic user-visible error (exit 1). Message already printed by caller.
    UserError,
    /// Rebase paused on a conflict (exit 2).
    ConflictExit,
    /// Abort completed successfully (exit 3).
    AbortExit,
}

impl CmdError {
    pub fn exit_code(&self) -> u8 {
        match self {
            CmdError::UserError => 1,
            CmdError::ConflictExit => 2,
            CmdError::AbortExit => 3,
        }
    }
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CmdError::UserError => write!(f, "command failed"),
            CmdError::ConflictExit => write!(f, "rebase conflict"),
            CmdError::AbortExit => write!(f, "abort complete"),
        }
    }
}

impl fmt::Debug for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for CmdError {}

/// Convenience alias for command functions.
pub type CmdResult = Result<(), CmdError>;
