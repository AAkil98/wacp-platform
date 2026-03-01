# WACP Cross-Impact Matrix

> Tracks how changes to one WACP spec ripple to others.
> Primary use: prevent silent spec drift during MADA-OS kernel drafting.

---

## § 1 — Purpose

The kernel is the first protocol-aware OS layer. It reads all 20 WACP specs simultaneously. During kernel drafting we will discover gaps, contradictions, and improvements needed in protocol specs. A change to one WACP spec can silently invalidate assumptions in others.

This document provides two instruments:

1. **Cross-Impact Matrix (§ 3)** — a 20×20 map showing which specs must be reviewed when a given spec changes.
2. **Change Log (§ 6)** — a chronological record of every cross-spec finding discovered during kernel drafting.

The matrix is pre-populated from formal `depends_on` frontmatter. Discovered impacts are added as `i` marks during drafting.

---

## § 2 — Abbreviations

| Code | Spec | Layer |
|------|------|-------|
| CLK | clock.md | foundations |
| ROL | roles.md | foundations |
| WS | workspace.md | primitives |
| ENV | envelope.md | primitives |
| SIG | signal.md | primitives |
| CKP | checkpoint.md | primitives |
| TRL | trail.md | primitives |
| TSK | task.md | primitives |
| IDN | identity.md | primitives |
| USR | user.md | primitives |
| TRE | tree.md | topology |
| GRA | graph.md | topology |
| CAU | causation.md | topology |
| OWN | ownership.md | topology |
| CHN | channels.md | topology |
| VIS | visibility.md | topology |
| INT | integration.md | mechanisms |
| HH | human-highway.md | mechanisms |
| REC | recovery.md | mechanisms |
| SEC | security.md | mechanisms |

---

## § 3 — Cross-Impact Matrix

**Reading the matrix:** Row = spec that changed. Column = spec that needs review.

- `d` — formal dependency (column spec lists row spec in `depends_on`)
- `i` — discovered impact (found during kernel drafting, not a formal dependency)
- `×` — self (diagonal)
- `—` — no known impact

Derived from verified `depends_on` frontmatter of all 20 WACP specs.

|        | CLK | ROL | WS | ENV | SIG | CKP | TRL | TSK | IDN | USR | TRE | GRA | CAU | OWN | CHN | VIS | INT | HH | REC | SEC | **Σ** |
|--------|-----|-----|----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|----|-----|-----|-------|
| **CLK** | ×  | —   | d  | d   | —   | —   | d   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —  | d   | —   | **4** |
| **ROL** | —  | ×   | d  | d   | —   | —   | d   | —   | —   | —   | —   | —   | —   | —   | d   | d   | —   | —  | —   | d   | **6** |
| **WS**  | —  | —   | ×  | d   | d   | d   | d   | d   | —   | d   | d   | d   | d   | d   | d   | d   | d   | d  | d   | d   | **16** |
| **ENV** | —  | —   | —  | ×   | —   | —   | d   | —   | —   | —   | —   | —   | —   | —   | d   | —   | —   | d  | d   | d   | **5** |
| **SIG** | —  | —   | —  | d   | ×   | d   | d   | d   | —   | —   | d   | —   | —   | —   | d   | —   | d   | d  | d   | d   | **10** |
| **CKP** | —  | —   | —  | —   | —   | ×   | d   | d   | —   | —   | —   | —   | —   | —   | —   | —   | d   | d  | d   | d   | **6** |
| **TRL** | —  | —   | —  | —   | —   | —   | ×   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —  | d   | d   | **2** |
| **TSK** | —  | —   | —  | —   | —   | —   | d   | ×   | —   | —   | —   | d   | —   | —   | —   | —   | —   | d  | —   | —   | **3** |
| **IDN** | —  | —   | d  | d   | —   | —   | d   | —   | ×   | d   | d   | d   | d   | d   | d   | d   | —   | —  | —   | d   | **11** |
| **USR** | —  | —   | d  | d   | —   | —   | d   | —   | —   | ×   | d   | —   | d   | d   | —   | —   | d   | d  | —   | d   | **9** |
| **TRE** | —  | —   | —  | —   | —   | —   | —   | —   | —   | —   | ×   | d   | d   | d   | d   | d   | —   | —  | —   | —   | **5** |
| **GRA** | —  | —   | —  | —   | —   | —   | —   | —   | —   | —   | —   | ×   | —   | —   | —   | —   | —   | —  | —   | —   | **0** |
| **CAU** | —  | —   | —  | —   | —   | —   | —   | —   | —   | —   | —   | —   | ×   | —   | —   | —   | —   | —  | —   | —   | **0** |
| **OWN** | —  | —   | —  | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | ×   | —   | —   | —   | —  | —   | —   | **0** |
| **CHN** | —  | —   | —  | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | ×   | —   | —   | —  | —   | —   | **0** |
| **VIS** | —  | —   | —  | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | ×   | —   | —  | —   | —   | **0** |
| **INT** | —  | —   | —  | —   | —   | —   | d   | —   | —   | —   | —   | —   | —   | —   | —   | —   | ×   | d  | —   | —   | **2** |
| **HH**  | —  | —   | —  | —   | —   | —   | d   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | ×  | —   | d   | **2** |
| **REC** | —  | —   | —  | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —  | ×   | —   | **0** |
| **SEC** | —  | —   | —  | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —   | —  | —   | ×   | **0** |

**High-impact rows** (most dependents — change with caution):

| Rank | Spec | Dependents | Note |
|------|------|-----------|------|
| 1 | WS | 16 | Nearly universal — workspace is the central primitive |
| 2 | IDN | 11 | Identity threads through every layer that names things |
| 3 | SIG | 10 | Signals are the primary state-transition mechanism |
| 4 | USR | 9 | User bridges identity and workspace ownership |
| 5 | ROL / CKP | 6 | Roles shape permissions; checkpoints shape durability |
| 6 | TRE / ENV | 5 | Tree structures topology; envelope structures messaging |

**Leaf specs** (no dependents — safe to change in isolation): GRA, CAU, OWN, CHN, VIS, REC, SEC.

**Note:** WS and USR have a mutual dependency (`d` in both directions). This is intentional — workspace defines the container, user defines the occupant, and each references the other.

---

## § 4 — How to Use

1. **Before changing a WACP spec**, check its row in the matrix. Every `d` or `i` column must be reviewed for ripple effects.
2. **When kernel drafting reveals an impact** not captured by `depends_on`, add an `i` mark to the appropriate cell and log the finding in § 6.
3. **Log every cross-spec finding** in § 6 with severity, affected specs, and resolution status.
4. **The matrix is a map, not a process.** It tells you where to look. It does not tell you what to do — that judgment belongs to the spec author.

---

## § 5 — Kernel Drafting Progress

Advisory mapping — which WACP concepts each kernel spec consumes. Refined during drafting.

| Kernel Spec | Status | WACP Specs Consumed |
|-------------|--------|---------------------|
| syscall.md | pending | WS, ENV, SIG, CKP, TSK, ROL, IDN |
| process.md | pending | WS, SIG, ROL, IDN, USR |
| scheduler.md | pending | WS, ROL, TSK |
| ipc.md | pending | ENV, SIG, ROL, CHN, VIS, TRE |
| vmm.md | pending | WS, VIS, SIG |
| device.md | pending | WS, IDN |

---

## § 6 — Change Log

Every cross-spec finding discovered during kernel drafting is logged here.

**Severity levels:**
- **high** — blocks kernel drafting (fix now)
- **medium** — inconsistency (fix before kernel synthesis)
- **low** — improvement (can defer)

| # | Date | Source | Changed Spec | Affected Spec(s) | Finding | Severity | Resolution | Status |
|---|------|--------|--------------|-------------------|---------|----------|------------|--------|
| | | | | | | | | |

*Entry IDs: CIM-001, CIM-002, etc.*

---

*WACP cross-impact tracker — authored by Akil Abderrahim and Claude Opus 4.6*
