use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use wacp_types::*;

/// Parameters for workspace creation.
#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    pub id: WorkspaceId,
    pub role: String,
    pub base_role: BaseRole,
    pub parent: WorkspaceId,
    pub owner: UserId,
    pub originator: Originator,
    pub directive: Envelope,
    pub context: Vec<u8>,
    pub visibility: HashSet<String>,
    pub authority: HashSet<String>,
    pub delegate: bool,
    pub budget: Option<ResourceBudget>,
}

/// Runtime resource tracking.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceMeter {
    pub usage: ResourceUsage,
    pub budget: Option<ResourceBudget>,
}

/// The nine-component workspace state (PROTOCOL.md §6.1).
#[derive(Debug)]
pub struct WorkspaceState {
    pub id: WorkspaceId,
    pub status: wacp_types::WorkspaceState,
    pub role: String,
    pub base_role: BaseRole,
    pub parent: WorkspaceId,
    pub owner: UserId,
    pub originator: Originator,
    pub delegate: bool,

    // 1. Directive — immutable after creation
    directive: Envelope,
    // 2. Inbox — append by runtime, consume by agent
    inbox: VecDeque<Envelope>,
    // 3. Context — immutable after creation
    context: Vec<u8>,
    // 4. Working memory — mutable by agent
    pub working_memory: Vec<u8>,
    // 5. Checkpoint register — append-only
    checkpoint_register: Vec<Checkpoint>,
    // 6. Resource meter — runtime-managed
    pub resource_meter: ResourceMeter,
    // 7. Local trail sequence tracking
    pub trail_sequence: u64,
    // 8. Visibility set — expandable only
    visibility_set: HashSet<String>,
    // 9. Authority set — frozen
    authority_set: HashSet<String>,

    // Envelope dedup tracking
    delivered_envelope_ids: HashSet<String>,
}

impl WorkspaceState {
    /// Create a new workspace in Idle state.
    pub fn new(config: WorkspaceConfig) -> Self {
        Self {
            id: config.id,
            status: wacp_types::WorkspaceState::Idle,
            role: config.role,
            base_role: config.base_role,
            parent: config.parent,
            owner: config.owner,
            originator: config.originator,
            delegate: config.delegate,
            directive: config.directive,
            inbox: VecDeque::new(),
            context: config.context,
            working_memory: Vec::new(),
            checkpoint_register: Vec::new(),
            resource_meter: ResourceMeter {
                usage: ResourceUsage::default(),
                budget: config.budget,
            },
            trail_sequence: 0,
            visibility_set: config.visibility,
            authority_set: config.authority,
            delivered_envelope_ids: HashSet::new(),
        }
    }

    // --- Immutable accessors ---

    pub fn directive(&self) -> &Envelope {
        &self.directive
    }

    pub fn context(&self) -> &[u8] {
        &self.context
    }

    pub fn authority(&self) -> &HashSet<String> {
        &self.authority_set
    }

    pub fn visibility(&self) -> &HashSet<String> {
        &self.visibility_set
    }

    // --- Visibility (additive only) ---

    pub fn grant_visibility(&mut self, resources: &[String]) {
        for r in resources {
            self.visibility_set.insert(r.clone());
        }
    }

    // --- Inbox (priority-ordered) ---

    /// Push an envelope into the inbox, maintaining priority order.
    /// Blocking > Urgent > Normal. Within same priority, FIFO.
    pub fn push_inbox(&mut self, envelope: Envelope) {
        let pos = self
            .inbox
            .iter()
            .position(|e| priority_rank(e.priority) < priority_rank(envelope.priority))
            .unwrap_or(self.inbox.len());
        self.inbox.insert(pos, envelope);
    }

    /// Pop the highest-priority envelope from the inbox.
    pub fn pop_inbox(&mut self) -> Option<Envelope> {
        let envelope = self.inbox.pop_front()?;
        self.delivered_envelope_ids
            .insert(envelope.id.to_string());
        Some(envelope)
    }

    /// Check if an envelope ID has already been delivered.
    pub fn is_envelope_delivered(&self, id: &str) -> bool {
        self.delivered_envelope_ids.contains(id)
    }

    /// Peek at inbox length.
    pub fn inbox_len(&self) -> usize {
        self.inbox.len()
    }

    // --- Checkpoint register (append-only) ---

    pub fn push_checkpoint(&mut self, cp: Checkpoint) {
        self.checkpoint_register.push(cp);
    }

    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoint_register
    }

    pub fn last_checkpoint(&self) -> Option<&Checkpoint> {
        self.checkpoint_register.last()
    }

    // --- Terminal state ---

    /// Consume into an immutable archive. Only valid for terminal states.
    pub fn archive(self) -> ArchivedWorkspace {
        debug_assert!(self.status.is_terminal());
        ArchivedWorkspace {
            id: self.id,
            terminal_state: self.status,
            role: self.role,
            parent: self.parent,
            owner: self.owner,
            originator: self.originator,
            directive: self.directive,
            context: self.context,
            checkpoints: self.checkpoint_register,
            final_usage: self.resource_meter.usage,
            visibility: self.visibility_set,
            authority: self.authority_set,
        }
    }
}

/// Immutable snapshot of a workspace in terminal state.
#[derive(Debug)]
pub struct ArchivedWorkspace {
    pub id: WorkspaceId,
    pub terminal_state: wacp_types::WorkspaceState,
    pub role: String,
    pub parent: WorkspaceId,
    pub owner: UserId,
    pub originator: Originator,
    pub directive: Envelope,
    pub context: Vec<u8>,
    pub checkpoints: Vec<Checkpoint>,
    pub final_usage: ResourceUsage,
    pub visibility: HashSet<String>,
    pub authority: HashSet<String>,
}

fn priority_rank(p: EnvelopePriority) -> u8 {
    match p {
        EnvelopePriority::Blocking => 2,
        EnvelopePriority::Urgent => 1,
        EnvelopePriority::Normal => 0,
    }
}
