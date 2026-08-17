use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name([u8; 16]);

        impl $name {
            pub fn new() -> Self {
                Self(*Uuid::new_v4().as_bytes())
            }

            pub const fn from_bytes(bytes: [u8; 16]) -> Self {
                Self(bytes)
            }

            pub const fn as_bytes(&self) -> &[u8; 16] {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

uuid_id!(ProjectId);
uuid_id!(UnitId);
uuid_id!(TaskId);
uuid_id!(GenerationId);
uuid_id!(ResourceId);
uuid_id!(BindingId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkPriority {
    P0Interactive,
    P1Visible,
    P2Focused,
    P3Background,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevisionKind {
    Edit,
    Undo,
    Redo,
}

impl RevisionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Edit => "Edit",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
        }
    }
}

impl TaskState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Running => "Running",
            Self::Paused => "Paused",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }

    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Pending, Self::Running | Self::Cancelled)
                | (
                    Self::Running,
                    Self::Paused | Self::Completed | Self::Failed | Self::Cancelled
                )
                | (Self::Paused, Self::Running | Self::Cancelled)
                | (Self::Failed, Self::Pending)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_terminal_states_are_not_reopened_silently() {
        assert!(!TaskState::Completed.can_transition_to(TaskState::Running));
        assert!(!TaskState::Cancelled.can_transition_to(TaskState::Running));
        assert!(TaskState::Failed.can_transition_to(TaskState::Pending));
    }
}
