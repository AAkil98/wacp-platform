# WACP: Security

## Metadata

```yaml
title: "WACP: Security"
id: wacp-spec-security
type: constituent-spec
tier: abstract
category: mechanisms
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §11
depends_on:
  - wacp-spec-identity
  - wacp-spec-roles
  - wacp-spec-envelope
  - wacp-spec-signal
  - wacp-spec-checkpoint
  - wacp-spec-trail
  - wacp-spec-workspace
  - wacp-spec-user
  - wacp-spec-human-highway
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, security, integrity, authentication, confidentiality, non-repudiation, threat-model]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Threat Model](#2-threat-model)
3. [Identity and Authentication](#3-identity-and-authentication)
4. [Message Integrity](#4-message-integrity)
5. [Trail Integrity](#5-trail-integrity)
6. [Confidentiality](#6-confidentiality)
7. [Non-Repudiation](#7-non-repudiation)
8. [Physical Resource Protection](#8-physical-resource-protection)
9. [Trail Events](#9-trail-events)
10. [Conformance Requirements](#10-conformance-requirements)
11. [Implementation Notes](#11-implementation-notes)

---

## 1. Purpose

The protocol's coordination mechanisms — roles, permissions, visibility, authority, trail immutability — are structural guarantees within a trusted runtime. They define what agents are allowed to do. The security model defines how that enforcement survives adversarial conditions. A permission matrix that can be bypassed is a suggestion. A trail that can be rewritten is a fiction.

The protocol defines the security model's policy: what it trusts, what it defends against, and seven invariants that must hold (PROTOCOL §11). This spec defines the mechanisms: how identity is verified, how messages are integrity-protected, how the trail resists tampering, how confidentiality is enforced across deployment models, and how non-repudiation is achieved through the combination of these mechanisms.

Five concerns:

- **Identity and authentication** — the detailed requirements for verifying agent, coordinator, and human identity. Every entity that participates in the protocol must be authenticated before it can act.
- **Message integrity** — how envelopes, signals, and checkpoints are protected against modification. Each message type has different integrity requirements reflecting its trust model.
- **Trail integrity** — the cryptographic mechanisms that make the trail tamper-evident: hash chains, signing, and cross-scope anchoring. These are the mechanisms explicitly delegated from PROTOCOL §11.5.
- **Confidentiality** — the trust boundary model and three deployment models that determine how visibility grants are enforced. These are the mechanisms explicitly delegated from PROTOCOL §11.6.
- **Physical resource protection** — how compute, memory, storage, and network are protected at each deployment model. The protocol's four physical resources (PROTOCOL §2.9) each have protection requirements that scale with the deployment context.

## 2. Threat Model

The protocol's threat model is defined in PROTOCOL §11.1–§11.2. This section expands on the threat actors and scope boundaries that the security mechanisms in this spec are designed to address.

### 2.1 Trust Root

The protocol runtime is the trust root. The runtime is the process (or set of processes) that implements the protocol: it delivers envelopes, enforces the permission matrix, manages workspace lifecycles, writes trail entries, and enforces budgets. The protocol assumes the runtime is correct and uncompromised.

This is the same trust model as an operating system kernel: the kernel is trusted, processes are not. If the kernel is compromised, all guarantees collapse. The protocol does not attempt to defend against a compromised runtime — doing so would require a fundamentally different architecture (Byzantine fault tolerance, hardware enclaves, or formal verification of the runtime itself). These are out of scope.

**What the runtime must be:**
- Correct in its implementation of the protocol.
- Protected from unauthorized modification (deployment security is an operational concern, not a protocol concern).
- The sole authority for trail writes, envelope delivery, state transitions, and permission enforcement.

### 2.2 Threat Actors

The protocol defends against four classes of threat actor, ordered by increasing severity.

**Rogue agent.** An agent within a workspace that attempts to exceed its permissions — reading resources outside its visibility, modifying resources outside its authority, sending envelopes without a valid send right (Roles spec, §5), or impersonating another agent. This is the most common threat and the one the permission matrix (Roles spec, §3) and port rights are designed to handle. The protocol requires that the runtime enforce permissions independently of agent cooperation — an agent cannot grant itself permissions it was not assigned.

**Compromised transport.** An adversary that can observe, modify, or inject messages on the communication channel between workspaces. This includes man-in-the-middle attacks on envelopes, signal injection, and replay attacks. The protocol requires message integrity (§4) and origin authentication to defend against this class.

**Compromised storage.** An adversary that can read or modify trail entries, checkpoints, or workspace data at rest. This includes tampering with the audit log to cover tracks, modifying checkpoints to alter artifacts, or reading confidential workspace data. The protocol requires trail integrity verification (§5) and defines confidentiality boundaries (§6) to defend against this class.

**Insider with elevated access.** A human operator or administrator who has legitimate access to the infrastructure but acts maliciously — modifying trail entries through direct storage access, injecting envelopes that bypass the highway, or altering workspace configurations. The protocol's non-repudiation requirements (§7) make such actions detectable, though not preventable.

### 2.3 Out of Scope

The following threats are explicitly out of scope:

- **Compromised runtime.** If the runtime itself is malicious or subverted, all protocol guarantees are void. Runtime integrity is an operational security concern.
- **Side-channel attacks.** Timing analysis, resource consumption patterns, or electromagnetic emanation. These require hardware-level or implementation-level mitigations.
- **Denial of service at the infrastructure level.** Network flooding, storage exhaustion, or compute starvation targeting the runtime itself. The protocol's admission control and budget limits (Workspace spec, §5) handle resource exhaustion at the coordination level, not the infrastructure level.
- **Agent-internal vulnerabilities.** Prompt injection, model jailbreaking, or other attacks on the LLM behind an agent. The protocol treats the agent as a black box that communicates through signals, envelopes, and checkpoints. What happens inside the agent is outside the protocol's purview — though the permission matrix ensures that a compromised agent's blast radius is limited to its workspace.

## 3. Identity and Authentication

Every entity that participates in the protocol — agents, the coordinator, and humans — must be authenticated. The protocol requires verified identity but does not prescribe a specific authentication mechanism, consistent with transport agnosticism (PROTOCOL §2.6). This section defines the detailed requirements for each actor type.

### 3.1 Agent Identity

Each agent operating within a workspace must possess a verifiable identity. The identity is bound to the workspace at creation and cannot change during the workspace's lifetime (consistent with the immutable role assignment, Roles spec, §2).

**Requirements:**

1. **Unique identity.** Every agent has a unique identifier that distinguishes it from all other agents in the system. This is distinct from the workspace ID — multiple workspaces may be operated by the same agent identity (sequentially, not concurrently), and agent migration changes the agent identity within a workspace.

2. **Verifiable at message origin.** When an agent emits a signal, creates a checkpoint, or sends an envelope, the runtime MUST verify that the message originates from the agent assigned to that workspace. An agent in workspace A cannot emit signals that appear to come from workspace B.

3. **Bound to workspace.** The `workspace_created` trail entry (Trail spec, §9) MUST include the authenticated agent identity. This binding is the foundation for non-repudiation (§7) — every action in the trail traces back to a verified identity, not a self-declared claim.

**What the protocol does NOT prescribe:**
- Whether identity is based on cryptographic keys, API tokens, mutual TLS certificates, or any other mechanism.
- How identities are provisioned, rotated, or revoked.
- Whether agents authenticate per-message or per-session.

### 3.2 Coordinator Identity

The coordinator is the most privileged entity in the protocol. It creates workspaces, manages budgets, performs integration, and can abort any workspace. Impersonating the coordinator grants full control over the system.

**Requirements:**

1. **Singleton verification.** The runtime MUST guarantee that exactly one coordinator exists per protocol instance (PROTOCOL §5.2). Any entity claiming coordinator authority MUST be verified against the authenticated coordinator identity.

2. **Elevated authentication.** The coordinator's identity MUST be established before any workspaces are created. The coordinator authenticates to the runtime, not to individual agents — agents trust the runtime to have verified the coordinator.

3. **Non-repudiable actions.** Every coordinator action — workspace creation, aborts, budget modifications, integration decisions, visibility grants — is recorded in the trail with the coordinator's verified identity. The coordinator cannot deny having taken an action if the trail records it with its identity.

### 3.3 Human Identity

Humans interact with the protocol through the highway (Human Highway spec). Human actions — gate approvals, envelope injections, escalation responses — carry the acting human's `user_id` in the `actor` field of trail entries (User spec, §2).

**Requirements:**

1. **Authenticated before action.** A human MUST be authenticated before they can approve gates, inject envelopes, or respond to escalations. An unauthenticated entity MUST NOT interact with the highway. Authentication produces a `user_id` that the runtime carries through all subsequent actions.

2. **Identity recorded in trail.** The `actor` field for highway events carries the authenticated human's `user_id` — the specific person, not a generic "human" marker. If multiple humans have highway access, the trail distinguishes them by `user_id`. Trail queries filtering on a specific `user_id` return exactly that human's actions.

3. **Authentication mechanism is application-defined.** Consistent with the interface boundary (Human Highway spec, §7), the protocol requires authentication but delegates the mechanism to the application. The protocol defines what identity information must be recorded (`user_id`); the application defines how identity is verified. See the User spec for lifecycle, ownership, capabilities, and deployment models.

## 4. Message Integrity

Every message in the protocol — envelopes, signals, and checkpoints — must be integrity-protected. Integrity means: the receiver can verify that the message was not modified after the sender created it. Each message type has different integrity requirements reflecting its trust model and transport path.

### 4.1 Envelope Integrity

Envelopes carry instructions (directives), evaluation (feedback), and questions (queries). Tampering with an envelope could redirect an agent, alter feedback, or change a query — with no visible indication that the original was modified.

**Requirements:**

1. **Integrity proof.** Every envelope MUST carry an integrity proof that the receiver (the runtime, acting on behalf of the target workspace) can verify. The proof covers the entire envelope: `id`, `from`, `to`, `type`, `payload`, `in_reply_to`, `rights`, `priority`, and `origin`. Modifying any field after the proof is computed invalidates the proof.

2. **Origin binding.** The integrity proof MUST be bound to the sender's identity. The receiver can verify not only that the envelope was not modified, but that it was created by the claimed sender. This converts the `from` field from a self-declared claim into a verified assertion.

3. **Verification before delivery.** The runtime verifies envelope integrity as part of the validation step (Envelope spec, §4). An envelope that fails integrity verification is rejected with `reason: integrity_violation` and recorded in the trail as an `envelope_rejected` entry with the violation details.

**Schema extension:**

```yaml
envelope:
  # ... existing fields (Envelope spec, §3) ...
  integrity:
    proof: bytes              # integrity proof covering all envelope fields
    algorithm: string         # identifier for the integrity mechanism used
```

The `algorithm` field is a string identifier, not a protocol-prescribed value. Implementations choose their integrity mechanism (HMAC, digital signature, etc.) and record which one was used. This enables mixed-algorithm environments and algorithm migration.

### 4.2 Signal Integrity

Signals are simpler than envelopes — they carry less data and travel only upward through the workspace tree. But they trigger state transitions, and a forged `complete` or `failed` signal could corrupt the workspace lifecycle.

**Requirements:**

1. **Origin verification.** The runtime MUST verify that a signal originates from the workspace it claims to come from. This is enforced at the runtime level — since signals are emitted within a workspace and delivered by the runtime to the parent, the runtime is in a position to verify origin without requiring a per-signal cryptographic proof.

2. **Transport integrity.** If signals travel over an untrusted transport (network, shared filesystem), they MUST carry integrity protection equivalent to envelopes. If signals are delivered entirely within the runtime's process boundary (in-memory), the runtime's own integrity is sufficient.

The protocol does not require a per-signal integrity proof field in the signal schema (Signal spec, §5) because the runtime — the trust root — mediates all signal delivery. This is a deliberate asymmetry with envelopes: envelopes may cross trust boundaries (between separately deployed agents), while signals travel through the runtime's internal machinery.

### 4.3 Checkpoint Integrity

Checkpoints contain the actual artifacts agents produce — code, reports, datasets. Tampering with a checkpoint could alter the integrated result without any visible trail anomaly.

**Requirements:**

1. **Content integrity.** Every checkpoint MUST carry an integrity proof covering its full content: `id`, `workspace`, `type`, `payload`, `intent`, `parent`, `status`, `confidence`, `resource_usage`. The proof is computed by the agent (or the runtime on behalf of the agent) at creation time.

2. **Integrity recorded in trail.** The `checkpoint_created` trail entry MUST include the integrity proof. This creates a verifiable chain: the trail entry proves the checkpoint existed with specific content at a specific time, and the integrity proof proves the content has not been modified since.

3. **Verification at integration.** When the coordinator reads a checkpoint for integration (Integration spec, §2), the runtime MUST verify the checkpoint's integrity proof before presenting the content. A checkpoint that fails verification is flagged — the coordinator is notified and may reject the integration.

## 5. Trail Integrity

The trail is the protocol's memory and the foundation of its audit capability. A trail that can be silently tampered with undermines every guarantee the protocol makes — replay, comparison, accountability, and recovery all depend on the trail being truthful. Trail integrity requires more than the append-only invariant (Trail spec, §4): it requires cryptographic tamper evidence.

The trail spec defines the hash chain's data structure — the `prev_hash` field that links entries into a chain (Trail spec, §4). This section defines the security mechanisms built on that chain: entry hashing, signing, verification procedures, and cross-scope anchoring.

### 5.1 Hash Chain

**Requirement.** Trail entries MUST form a hash chain. Each entry includes the cryptographic hash of the previous entry in the same scope (workspace-local or global). This creates a tamper-evident chain — modifying any entry invalidates the chain from that point forward.

**Entry hash computation.** Each trail entry has an `entry_hash` — the hash of the entry's contents. The hash covers all fields including `prev_hash` (the previous entry's hash) but excluding `entry_hash` itself.

```yaml
trail_entry:
  # ... existing fields (Trail spec, §2) ...
  integrity:
    entry_hash: bytes         # hash of this entry (all fields except entry_hash)
    algorithm: string         # hash algorithm identifier
```

**Properties:**

1. **First entry.** The first trail entry in a scope has `prev_hash: null` (Trail spec, §2). Its `entry_hash` is computed over all other fields.

2. **Subsequent entries.** Each entry's `entry_hash` is computed over all fields including `prev_hash`. This binds each entry to its predecessor, creating the chain.

3. **Verification.** To verify trail integrity, read entries in order and confirm that each entry's `prev_hash` matches the preceding entry's `entry_hash`. A mismatch indicates tampering or corruption.

4. **Scope.** Each workspace-local trail maintains its own hash chain. The global trail maintains a separate chain. Entries appear in both chains (local and global) with potentially different `prev_hash` values, since the chains have different predecessors.

**Hash algorithm.** The protocol does not mandate a specific hash algorithm. The runtime selects an algorithm at initialization and records it in the first trail entry's body (Trail spec, §4). All entries in a run use the same algorithm. Implementations SHOULD use a cryptographically strong hash (SHA-256 or better). The choice is recorded, not prescribed, so that the protocol can outlive any single algorithm.

### 5.2 Trail Signing

**Recommendation (not requirement).** Implementations SHOULD sign trail entries. Each entry's `entry_hash` is signed by the runtime's identity, producing a signature that proves the runtime — the trust root — vouches for the entry's authenticity.

```yaml
trail_entry:
  # ... existing fields ...
  integrity:
    entry_hash: bytes
    algorithm: string
    signature: bytes          # optional: runtime's signature over entry_hash
    signer: string            # optional: identity of the signing runtime
```

Signing strengthens tamper evidence beyond what the hash chain alone provides. The hash chain detects that tampering occurred (a mismatch in the chain). A signed chain also proves that the untampered entries were genuinely written by the runtime — an adversary who rewrites the chain from a tampered entry forward must forge the runtime's signature on every subsequent entry.

Signing is recommended rather than required because it depends on key management infrastructure that the protocol does not prescribe. The hash chain is required because it depends only on a hash function — a minimal cryptographic primitive that every implementation can provide.

### 5.3 Cross-Scope Anchoring

The global trail and workspace-local trails are related but maintain independent hash chains (Trail spec, §4). To prevent an attack where a local trail is tampered with independently of the global trail, the protocol requires cross-scope anchoring.

**Requirement.** When a trail entry is recorded in both a local and global trail, the global trail entry MUST include the local trail's `entry_hash` as an additional field. This anchors the local chain into the global chain — tampering with the local trail is detectable by comparing against the global trail's anchor.

```yaml
# Global trail entry for a workspace-scoped event
trail_entry:
  # ... standard fields ...
  integrity:
    entry_hash: bytes         # hash of this global entry
    algorithm: string
    local_hash: bytes         # entry_hash from the corresponding local trail entry
```

**Why cross-scope anchoring matters.** Without anchoring, an adversary with storage access could rewrite a workspace's local trail (replacing entries, recalculating hashes to maintain the local chain) without disturbing the global trail. The coordinator, reading the global trail, would see the correct history — but the workspace's local trail, used for workspace timeline views and recovery replay, would contain the tampered version. Cross-scope anchoring makes this attack detectable: the global entry's `local_hash` will not match the tampered local entry's recalculated hash.

## 6. Confidentiality

The protocol defines visibility grants (Workspace spec, §8) that control which workspaces can read which resources. These grants are coordination semantics — they tell agents what they are permitted to see. Confidentiality is the enforcement question: are agents *unable* to see what they are not permitted to see?

### 6.1 Trust Boundary Model

The protocol defines a trust boundary rather than prescribing encryption. The trust boundary is the perimeter within which the runtime enforces visibility grants. Inside the boundary, the runtime controls access — an agent requesting a resource it has no visibility grant for is denied by the runtime. Outside the boundary, enforcement requires additional mechanisms.

**Three deployment models, in order of increasing security:**

| Model | Trust boundary | Visibility enforcement | When to use |
|-------|---------------|----------------------|-------------|
| **Single-runtime** | The runtime process | Runtime-enforced access control (no encryption needed) | All agents run within a single runtime; storage is local |
| **Shared-infrastructure** | The deployment environment (e.g., a private network, a Kubernetes cluster) | Runtime-enforced access control + transport encryption between runtime components | Agents are distributed but infrastructure is controlled |
| **Zero-trust** | Each workspace boundary | Per-workspace encryption of data at rest and in transit; visibility grants are enforced cryptographically | Agents run on untrusted infrastructure; storage is shared or adversary-accessible |

The protocol requires that implementations declare which model they operate under. The choice affects what confidentiality guarantees the implementation can provide. A single-runtime deployment need not encrypt inter-workspace communication; a zero-trust deployment must.

### 6.2 Minimum Confidentiality Requirements

Regardless of deployment model, the following access control boundaries MUST be enforced:

1. **Trail access control.** The global trail is readable only by the coordinator and entities with explicit global trail access. Workspace-local trails are readable only by the workspace's agent, entities with visibility grants to that workspace, and the coordinator. The runtime enforces these restrictions (Trail spec, §3).

2. **Checkpoint access control.** A checkpoint's content is accessible only to entities with visibility into the workspace that produced it. The integration record references a checkpoint — accessing the referenced content requires visibility into the source workspace.

3. **Envelope payload access control.** An envelope's payload is accessible only to the sender, the receiver, and the coordinator (which has visibility into all workspaces). The transport layer MUST NOT expose envelope payloads to other workspaces.

4. **Human highway confidentiality.** The highway provides full visibility (Human Highway spec, §2.1), meaning authenticated humans can see all trail entries, envelopes, and checkpoints. This is by design — the human is a supervisor. But highway access MUST be restricted to authenticated humans (§3.3), and the highway interface MUST protect data in transit.

### 6.3 Visibility Grant Enforcement

Dynamic visibility grants (Workspace spec, §8) expand a workspace's read access during execution. The security implications:

1. **Grants are additive and irreversible.** Once granted, visibility cannot be revoked within the workspace's lifetime. This is a security property, not just a convenience — it prevents a race condition where an agent acts on data that is subsequently made invisible.

2. **Grants are recorded in the trail.** The `visibility_granted` trail entry creates an audit record of every access expansion. Post-hoc analysis can verify that every resource access was backed by a valid grant.

3. **Grants do not override confidentiality.** In a zero-trust deployment, granting visibility to a resource means providing the workspace with the decryption key or access token for that resource. The grant is the authorization; the key is the enforcement.

## 7. Non-Repudiation

Non-repudiation means that an entity cannot deny having taken an action if the trail records it. The combination of authenticated identity (§3), message integrity (§4), and trail integrity (§5) produces non-repudiation as an emergent property — not a separate mechanism, but a consequence of the other three.

**The chain of proof:**

1. An agent's identity is verified at workspace creation (§3.1). The `workspace_created` trail entry binds the identity to the workspace.
2. Every action by that agent — signals, checkpoints, envelopes — carries an integrity proof bound to the agent's identity (§4).
3. Every action is recorded in the trail with the verified identity in the `actor` field (Trail spec, §2).
4. The trail entry is linked into the hash chain (§5.1), making it tamper-evident.
5. If the trail entry is signed by the runtime (§5.2), the runtime vouches for the entry's authenticity.

**Result.** To deny an action, an entity would need to either break the integrity proof, break the hash chain, or prove that its identity was stolen. The protocol does not make non-repudiation absolute — identity theft is always possible. But it raises the bar from "anyone can claim anything" to "denial requires breaking cryptographic guarantees."

**Non-repudiation for each actor type:**

| Actor | What's non-repudiable | How |
|-------|----------------------|-----|
| Agent | Signals emitted, checkpoints created, envelopes sent | Identity bound to workspace (§3.1); integrity proofs on messages (§4) |
| Coordinator | Workspace creation, aborts, budget modifications, integration decisions, visibility grants | Coordinator identity verified (§3.2); all actions in trail |
| Human | Gate approvals/rejections, envelope injections, escalation responses | Human identity recorded in trail (§3.3); highway events are trail entries |
| Runtime | Auto-emitted signals, timeout transitions, budget enforcement | Runtime identity in signed trail entries (§5.2) |

## 8. Physical Resource Protection

The protocol's security model extends to the four physical resources that back every protocol operation (PROTOCOL §2.9). Each physical resource has protection requirements; the appropriate enforcement level depends on the deployment model (§6.1).

### 8.1 Per-Resource Requirements

| Physical resource | Protection requirements |
|---|---|
| **Compute** | Fair scheduling across workspaces. Privilege boundary between the driver and workspace context — the driver isolates inference calls so that one workspace's generation cannot observe or influence another's. |
| **Memory** | Workspace context isolation — one workspace's context window MUST NOT be readable by another workspace. Confidentiality of context contents in memory. |
| **Storage** | Integrity at rest — stored checkpoints, trail entries, and envelopes MUST NOT be silently modified. Confidentiality at rest is deployment-model-dependent (§8.2). |
| **Network** | Confidentiality in transit — envelope and signal content MUST NOT be observable by unauthorized parties during delivery. Access control on delivery paths is deployment-model-dependent (§8.2). |

### 8.2 Deployment Model Extension

The three deployment models (§6.1) have different physical protection requirements:

| Deployment model | Compute | Memory | Storage | Network |
|---|---|---|---|---|
| **Single-runtime** | Process isolation (driver runs in same process; workspace isolation is logical) | In-process memory isolation (separate context buffers per workspace) | Filesystem permissions; integrity via write-once semantics | In-process delivery; no wire exposure |
| **Shared-infrastructure** | Container/VM isolation between workspaces sharing physical hosts | Memory encryption or separate address spaces per workspace | Encryption at rest; access control on storage paths | TLS for inter-process delivery; mutual authentication |
| **Zero-trust** | Hardware-backed isolation (TEE/enclave) or equivalent cryptographic guarantees | Encrypted memory with per-workspace keys | End-to-end encryption at rest with key management | End-to-end encryption in transit; certificate pinning; network-level access control |

The protocol requires that physical resources are protected; the deployment model determines the enforcement mechanism. A single-runtime deployment that uses in-process isolation for all four resources is conformant. A zero-trust deployment that lacks encryption at rest for storage is not.

## 9. Trail Events

Two trail event types support security observability.

**`authentication_failed`** — recorded when an entity fails to authenticate.

```yaml
authentication_failed:
  entity: string              # identifier of the entity that failed (if available)
  context: string             # what the entity was attempting (workspace_creation, envelope_send, highway_access)
  reason: string              # why authentication failed (invalid_credential, expired_token, unknown_identity)
  source: string              # where the attempt originated (transport address, interface identifier)
```

**`integrity_violation`** — recorded when a message or trail entry fails integrity verification.

```yaml
integrity_violation:
  subject_type: enum          # envelope | signal | checkpoint | trail_entry
  subject_id: string          # ID of the object that failed verification
  violation: enum             # hash_mismatch | signature_invalid | proof_missing | chain_broken
  expected: string            # what was expected (e.g., expected hash value)
  actual: string              # what was found
  action_taken: enum          # rejected | flagged | quarantined
```

These events are always recorded in the global trail regardless of which workspace they relate to — security events are system-wide concerns. The coordinator receives them and may escalate to the human highway. A pattern of `authentication_failed` events from the same source suggests an active attack. A single `integrity_violation` on a trail entry suggests storage corruption or tampering.

**Security events are never silent** (PROTOCOL §11.8, invariant 6). Every authentication failure, integrity violation, and permission denial MUST be recorded in the trail. The User spec (§8) defines the full set of user-related security events: `user_created` (identity enters the system), `authentication_succeeded` and `authentication_failed` (credential verification), `capability_granted` and `capability_revoked` (privilege changes), and `capability_denied` (enforcement). The `authentication_failed` schema defined here is the canonical definition — the User spec references it. The `integrity_violation` event defined here covers non-authentication security failures. Together, these events provide complete security observability.

## 10. Conformance Requirements

| Requirement | Level | Description |
|-------------|-------|-------------|
| Agent identity verification | Core | The runtime MUST verify that messages originate from the agent assigned to the emitting workspace (§3.1). |
| Coordinator singleton | Core | The runtime MUST guarantee exactly one coordinator per protocol instance, verified before workspace creation (§3.2). |
| Human authentication | Core | Humans MUST be authenticated before any highway action. The `actor` field MUST carry the authenticated `user_id` (§3.3). |
| Envelope integrity proof | Core | Every envelope MUST carry an integrity proof covering all fields. The runtime MUST verify before delivery (§4.1). |
| Envelope origin binding | Core | Envelope integrity proofs MUST be bound to the sender's identity (§4.1). |
| Signal origin verification | Core | The runtime MUST verify signal origin — a signal MUST come from the workspace it claims (§4.2). |
| Checkpoint content integrity | Core | Every checkpoint MUST carry an integrity proof. The runtime MUST verify at integration time (§4.3). |
| Trail hash chain | Core | Trail entries MUST form a hash chain via `prev_hash` (§5.1). The runtime MUST validate chain integrity. |
| Trail access control | Core | The runtime MUST enforce trail access rules: workers see own local trail only, coordinators see global trail (§6.2). |
| Security event recording | Core | Authentication failures, integrity violations, and permission denials MUST be recorded in the trail. No security event may be silent (§9). |
| Integrity verification enforcement | Core | Messages that fail integrity verification MUST be rejected and logged, never silently accepted (PROTOCOL §11.8, invariant 3). |
| Deployment model declaration | Standard | Implementations MUST declare which deployment model they operate under (§6.1). |
| Minimum confidentiality | Standard | Implementations MUST enforce the four minimum confidentiality requirements regardless of deployment model (§6.2). |
| Visibility grant audit | Standard | Visibility grants MUST be recorded in the trail. Every resource access MUST be backed by a valid grant (§6.3). |
| Cross-scope anchoring | Standard | Global trail entries for workspace-scoped events MUST include the local trail's `entry_hash` as an anchor (§5.3). |
| Checkpoint verification at integration | Standard | The runtime MUST verify checkpoint integrity before presenting content for integration (§4.3). |
| Physical resource isolation | Standard | Workspace contexts MUST be isolated — one workspace's context MUST NOT be readable by another (§8.1). |
| Integrity violation trail events | Standard | `integrity_violation` trail events MUST include `subject_type`, `subject_id`, `violation`, and `action_taken` per the schema in §9. |
| Trail signing | Full | Implementations SHOULD sign trail entries with the runtime's identity (§5.2). |
| Deployment-appropriate protection | Full | Physical resources SHOULD be protected at the level required by the declared deployment model (§8.2). |
| Non-repudiation chain | Full | The combination of identity, integrity, and trail mechanisms SHOULD produce a verifiable chain of proof for every protocol action (§7). |
| Signal transport integrity | Full | Signals traveling over untrusted transport SHOULD carry integrity protection equivalent to envelopes (§4.2). |

## 11. Implementation Notes

These notes are non-normative. They capture practical guidance for implementers.

**Envelope integrity as HMAC.** The simplest conformant implementation uses HMAC-SHA256 for envelope integrity. The runtime generates a shared secret per workspace at creation time. The sender computes `HMAC(secret, canonical_serialize(envelope_fields))` and places the result in `integrity.proof`. The runtime verifies by recomputing the HMAC. This satisfies origin binding (only the assigned workspace knows the secret) and integrity (modifying any field changes the HMAC). For deployments where the sender and runtime share memory (single-runtime model), the shared secret is trivially managed.

**Signal integrity in distributed deployments.** In a single-runtime deployment, signal integrity is free — the runtime mediates all signal delivery within its own process. In a shared-infrastructure or zero-trust deployment, signals may cross process boundaries. The simplest approach: if the inter-process transport is TLS-protected (shared-infrastructure) or end-to-end encrypted (zero-trust), the transport layer provides integrity. No per-signal proof is needed beyond what the transport provides.

**Checkpoint integrity as content hash.** Checkpoints may be large (code files, datasets). Computing HMAC over the full content is straightforward. The integrity proof is the HMAC of the canonical serialization of the checkpoint's fields. The `checkpoint_created` trail entry includes this proof. At integration time, the runtime recomputes the HMAC and compares. If the checkpoint was stored externally (not inline in the trail), this detects storage-level tampering.

**Hash chain verification cost.** Full chain verification is O(N) in the number of trail entries. For large trails, this may be expensive. Implementations can optimize with periodic anchoring: every M entries, the runtime writes a summary entry whose hash is computed over the last M entries' hashes. Verification then checks the summaries (O(N/M)) and spot-checks individual entries within each summary block. This is a performance optimization, not a protocol mechanism — the full chain must still be verifiable if needed.

**Cross-scope anchoring storage.** The `local_hash` field on global trail entries adds storage overhead — one hash value per entry. For SHA-256, this is 32 bytes per entry. For a run with 10,000 trail entries, the overhead is ~320KB — negligible. Implementations should not optimize away cross-scope anchoring; the storage cost is minimal and the security benefit is significant.

**Deployment model enforcement.** The protocol requires that implementations declare their deployment model (§6.1) but does not prescribe how the declaration is made. A practical approach: the runtime's configuration file includes a `security.deployment_model` field set to `single_runtime`, `shared_infrastructure`, or `zero_trust`. The runtime validates at startup that the available security mechanisms match the declared model — a `zero_trust` declaration without encryption at rest should fail validation.

**Key management.** The protocol deliberately does not prescribe key management. For signing (§5.2), the runtime needs a private key. For envelope integrity (§4.1), each workspace needs a shared secret. For zero-trust encryption (§6.1, §8.2), each workspace needs encryption keys. Key provisioning, storage, rotation, and revocation are implementation concerns. The protocol records the algorithm and identity in trail entries so that verification can be performed with the correct key — but key lookup is the implementation's responsibility.

**Testing security conformance.** A conformant implementation should be testable against these scenarios:

1. **Envelope integrity.** Modify an envelope after integrity proof computation. Verify rejection with `reason: integrity_violation`.
2. **Origin verification.** Attempt to emit a signal from workspace A claiming to be from workspace B. Verify rejection.
3. **Trail tampering detection.** Modify a trail entry at rest. Verify that hash chain verification detects the tampering.
4. **Access control.** Request a workspace's local trail from a different workspace with no visibility grant. Verify empty result.
5. **Cross-scope anchoring.** Modify a local trail entry. Verify that the global trail's `local_hash` detects the mismatch.
6. **Security event recording.** Trigger an authentication failure. Verify an `authentication_failed` trail entry is recorded.
7. **Checkpoint verification.** Modify a stored checkpoint. Verify that integration-time verification detects the tampering.

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
