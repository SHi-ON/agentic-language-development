//! Run lifecycle: register → execute → checkpoint → anchor → verify.
//!
//! The linear state machine that
//! gates every run. Transitions are total functions returning the next
//! state or a domain error; illegal transitions are unrepresentable at
//! runtime and rejected here.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("illegal transition from {from} on {event}")]
    Illegal { from: String, event: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunState {
    Registered,
    Executing,
    Checkpointed,
    Anchored,
    Verified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    BeginExecution,
    Checkpoint,
    Anchor,
    Verify,
}

pub fn transition(state: RunState, event: LifecycleEvent) -> Result<RunState, LifecycleError> {
    match (state, event) {
        (RunState::Registered, LifecycleEvent::BeginExecution) => Ok(RunState::Executing),
        (RunState::Executing, LifecycleEvent::Checkpoint) => Ok(RunState::Checkpointed),
        (RunState::Checkpointed, LifecycleEvent::Anchor) => Ok(RunState::Anchored),
        (RunState::Anchored, LifecycleEvent::Verify) => Ok(RunState::Verified),
        _ => Err(LifecycleError::Illegal {
            from: format!("{state:?}"),
            event: format!("{event:?}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path() {
        let mut s = RunState::Registered;
        for e in [
            LifecycleEvent::BeginExecution,
            LifecycleEvent::Checkpoint,
            LifecycleEvent::Anchor,
            LifecycleEvent::Verify,
        ] {
            s = transition(s, e).unwrap();
        }
        assert_eq!(s, RunState::Verified);
    }

    #[test]
    fn skip_rejected() {
        assert!(transition(RunState::Registered, LifecycleEvent::Anchor).is_err());
    }
}
