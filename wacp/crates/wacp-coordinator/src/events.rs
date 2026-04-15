use wacp_types::*;

/// An event emitted by the coordinator for streaming subscribers.
#[derive(Debug, Clone)]
pub enum CoordinatorEvent {
    TrailEntry {
        workspace_id: Option<WorkspaceId>,
        event_type: String,
        sequence: u64,
    },
    GateOpened(GateEvent),
    EscalationRaised(EscalationEvent),
    WorkspaceStateChanged {
        workspace_id: WorkspaceId,
        from: WorkspaceState,
        to: WorkspaceState,
    },
    TaskStatusChanged {
        task_id: TaskId,
        from: TaskStatus,
        to: TaskStatus,
    },
}

/// Callback-based event bus for coordinator events.
///
/// Subscribers register callbacks. The coordinator calls `emit` when events
/// occur. Subscribers receive all events — filtering is the subscriber's
/// responsibility.
///
/// This is a synchronous, single-threaded bus (no tokio channels). The
/// coordinator calls `emit` during its message processing. For the async
/// streaming RPCs, the runtime wraps this in a tokio broadcast channel.
type EventCallback = Box<dyn Fn(&CoordinatorEvent) + Send>;

pub struct EventBus {
    subscribers: Vec<EventCallback>,
    /// Collected events for testing (when no subscribers are registered).
    buffer: Vec<CoordinatorEvent>,
    buffering: bool,
}

impl EventBus {
    /// Create an event bus. If `buffering` is true, events are collected
    /// in an internal buffer (for testing).
    pub fn new(buffering: bool) -> Self {
        Self {
            subscribers: Vec::new(),
            buffer: Vec::new(),
            buffering,
        }
    }

    /// Register a subscriber callback.
    pub fn subscribe(&mut self, callback: Box<dyn Fn(&CoordinatorEvent) + Send>) {
        self.subscribers.push(callback);
    }

    /// Emit an event to all subscribers and optionally buffer it.
    pub fn emit(&mut self, event: CoordinatorEvent) {
        for sub in &self.subscribers {
            sub(&event);
        }
        if self.buffering {
            self.buffer.push(event);
        }
    }

    /// Drain the buffer (for testing).
    pub fn drain(&mut self) -> Vec<CoordinatorEvent> {
        std::mem::take(&mut self.buffer)
    }

    /// Number of buffered events.
    pub fn buffered_count(&self) -> usize {
        self.buffer.len()
    }

    /// Number of subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(false)
    }
}
