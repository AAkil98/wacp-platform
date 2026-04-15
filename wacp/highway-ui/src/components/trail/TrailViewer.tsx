import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { useStore, selectFilteredTrail, type TrailEntry } from "@/store";

// ── Event type → category mapping for badge colors ──

const EVENT_CATEGORIES: Record<string, string> = {
  gate_triggered: "gate",
  gate_resolved: "gate",
  gate_timeout: "gate",
  human_injection: "injection",
  escalation_raised: "escalation",
  escalation_resolved: "escalation",
  escalation_timeout: "escalation",
  workspace_created: "workspace",
  workspace_state_changed: "workspace",
  workspace_terminated: "workspace",
  envelope_sent: "envelope",
  envelope_delivered: "envelope",
  envelope_rejected: "envelope",
  signal_emitted: "signal",
  checkpoint_created: "checkpoint",
  task_status_changed: "task",
  task_created: "task",
  integration_started: "integration",
  integration_completed: "integration",
  migration_started: "workspace",
  migration_completed: "workspace",
};

const CATEGORY_COLORS: Record<string, string> = {
  gate: "bg-amber-700 text-amber-100",
  escalation: "bg-red-700 text-red-100",
  workspace: "bg-blue-700 text-blue-100",
  envelope: "bg-cyan-700 text-cyan-100",
  signal: "bg-green-700 text-green-100",
  checkpoint: "bg-purple-700 text-purple-100",
  task: "bg-indigo-700 text-indigo-100",
  integration: "bg-emerald-700 text-emerald-100",
  injection: "bg-orange-700 text-orange-100",
};

const KNOWN_EVENT_TYPES = Object.keys(EVENT_CATEGORIES);

const ROW_HEIGHT = 28;
const EXPANDED_HEIGHT = 140;
const BUFFER_ROWS = 10;

// ── HLC timestamp formatting ──

function formatHlcTime(physicalUs: bigint): string {
  const ms = Number(physicalUs / 1000n);
  const d = new Date(ms);
  const hh = d.getHours().toString().padStart(2, "0");
  const mm = d.getMinutes().toString().padStart(2, "0");
  const ss = d.getSeconds().toString().padStart(2, "0");
  const mmm = d.getMilliseconds().toString().padStart(3, "0");
  return `${hh}:${mm}:${ss}.${mmm}`;
}

function formatHlcFull(physicalUs: bigint): string {
  const ms = Number(physicalUs / 1000n);
  return new Date(ms).toISOString();
}

// ── Component ──

export interface TrailViewerProps {
  /** When set, operates in scoped mode for a single workspace. */
  workspaceId?: string;
  /** Whether to show the filter bar. Defaults to true. */
  showFilters?: boolean;
}

export function TrailViewer({ workspaceId, showFilters = true }: TrailViewerProps) {
  const navigate = useNavigate();
  const trail = useStore((s) => s.trail);
  const setPaused = useStore((s) => s.setTrailPaused);
  const setTrailFilter = useStore((s) => s.setTrailFilter);
  const clearTrailFilters = useStore((s) => s.clearTrailFilters);
  const setExpandedEntry = useStore((s) => s.setExpandedEntry);
  const views = useStore((s) => s.workspaces.views);

  const { paused, filters, expandedEntryIndex } = trail;

  // Derive filtered entries with stable reference when inputs unchanged
  const filteredEntries = useMemo(() => selectFilteredTrail(trail), [trail]);

  // In scoped mode, further filter to this workspace
  const displayEntries = useMemo(
    () => (workspaceId ? filteredEntries.filter((e) => e.workspaceId === workspaceId) : filteredEntries),
    [filteredEntries, workspaceId],
  );

  // ── Virtualization state ──
  const containerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [containerHeight, setContainerHeight] = useState(0);
  const [isAtBottom, setIsAtBottom] = useState(true);

  // Track container size
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const observer = new ResizeObserver((entries) => {
      const first = entries[0];
      if (first) setContainerHeight(first.contentRect.height);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Auto-scroll to bottom on new entries if user was at bottom
  const prevLengthRef = useRef(displayEntries.length);
  useEffect(() => {
    if (displayEntries.length > prevLengthRef.current && isAtBottom && !paused) {
      const el = containerRef.current;
      if (el) el.scrollTop = el.scrollHeight;
    }
    prevLengthRef.current = displayEntries.length;
  }, [displayEntries.length, isAtBottom, paused]);

  const handleScroll = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;
    setScrollTop(el.scrollTop);
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < ROW_HEIGHT * 2;
    setIsAtBottom(atBottom);
  }, []);

  const jumpToLatest = useCallback(() => {
    const el = containerRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
      setIsAtBottom(true);
    }
  }, []);

  // ── Compute visible window ──
  function getRowHeight(index: number): number {
    return index === expandedEntryIndex ? ROW_HEIGHT + EXPANDED_HEIGHT : ROW_HEIGHT;
  }

  // Calculate total height and visible range
  let totalHeight = 0;
  const rowTops: number[] = [];
  for (let i = 0; i < displayEntries.length; i++) {
    rowTops.push(totalHeight);
    totalHeight += getRowHeight(i);
  }

  let startIdx = 0;
  let endIdx = displayEntries.length;
  if (containerHeight > 0 && displayEntries.length > 0) {
    // Binary search for first visible row
    let lo = 0, hi = displayEntries.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if ((rowTops[mid] ?? 0) + getRowHeight(mid) < scrollTop) lo = mid + 1;
      else hi = mid;
    }
    startIdx = Math.max(0, lo - BUFFER_ROWS);

    // Find last visible row
    const bottomEdge = scrollTop + containerHeight;
    let end = lo;
    while (end < displayEntries.length && (rowTops[end] ?? 0) < bottomEdge) end++;
    endIdx = Math.min(displayEntries.length, end + BUFFER_ROWS);
  }

  const visibleEntries = displayEntries.slice(startIdx, endIdx);

  // ── Filter bar ──
  const [eventTypeOpen, setEventTypeOpen] = useState(false);

  const workspaceIds = useMemo(() => [...views.keys()], [views]);

  function toggleEventType(type: string) {
    const next = new Set(filters.eventTypes);
    if (next.has(type)) next.delete(type);
    else next.add(type);
    setTrailFilter({ eventTypes: next });
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-lg font-semibold">
          {workspaceId ? `Trail — ${workspaceId.slice(0, 12)}` : "Trail"}
        </h2>
        <div className="flex items-center gap-2">
          {!isAtBottom && (
            <button
              onClick={jumpToLatest}
              className="text-xs px-2 py-1 rounded bg-blue-700 hover:bg-blue-600"
            >
              Jump to latest
            </button>
          )}
          <button
            onClick={() => setPaused(!paused)}
            className="text-sm px-3 py-1 rounded bg-zinc-800 hover:bg-zinc-700"
          >
            {paused ? "Resume" : "Pause"}
          </button>
        </div>
      </div>

      {/* Filter bar */}
      {showFilters && (
        <div className="flex items-center gap-3 mb-3 flex-wrap">
          {/* Event type multi-select */}
          <div className="relative">
            <button
              onClick={() => setEventTypeOpen(!eventTypeOpen)}
              className="text-xs px-2 py-1 rounded bg-zinc-800 hover:bg-zinc-700 border border-zinc-700"
            >
              Event type{" "}
              {filters.eventTypes.size > 0 && (
                <span className="ml-1 text-blue-400">({filters.eventTypes.size})</span>
              )}
            </button>
            {eventTypeOpen && (
              <div className="absolute z-20 top-full left-0 mt-1 bg-zinc-800 border border-zinc-700 rounded shadow-lg max-h-60 overflow-auto w-56">
                {KNOWN_EVENT_TYPES.map((type) => (
                  <label
                    key={type}
                    className="flex items-center gap-2 px-3 py-1 hover:bg-zinc-700 cursor-pointer text-xs"
                  >
                    <input
                      type="checkbox"
                      checked={filters.eventTypes.has(type)}
                      onChange={() => toggleEventType(type)}
                      className="accent-blue-500"
                    />
                    <span>{type}</span>
                  </label>
                ))}
              </div>
            )}
          </div>

          {/* Workspace filter */}
          <input
            type="text"
            placeholder="Workspace ID"
            value={filters.workspaceId}
            onChange={(e) => setTrailFilter({ workspaceId: e.target.value })}
            list="workspace-ids"
            className="text-xs px-2 py-1 rounded bg-zinc-800 border border-zinc-700 w-36 placeholder-zinc-500"
          />
          <datalist id="workspace-ids">
            {workspaceIds.map((id) => (
              <option key={id} value={id} />
            ))}
          </datalist>

          {/* Actor filter */}
          <input
            type="text"
            placeholder="Actor"
            value={filters.actor}
            onChange={(e) => setTrailFilter({ actor: e.target.value })}
            className="text-xs px-2 py-1 rounded bg-zinc-800 border border-zinc-700 w-32 placeholder-zinc-500"
          />

          {/* Clear */}
          {(filters.eventTypes.size > 0 || filters.workspaceId || filters.actor) && (
            <button
              onClick={clearTrailFilters}
              className="text-xs px-2 py-1 rounded text-zinc-400 hover:text-zinc-200"
            >
              Clear
            </button>
          )}
        </div>
      )}

      {/* Table */}
      {displayEntries.length === 0 ? (
        <div className="text-zinc-500 text-sm">No trail entries.</div>
      ) : (
        <div
          ref={containerRef}
          onScroll={handleScroll}
          className="flex-1 overflow-auto font-mono text-xs"
        >
          {/* Table header */}
          <div
            className="sticky top-0 z-10 bg-zinc-950 flex text-zinc-500 border-b border-zinc-800 text-left"
            style={{ height: ROW_HEIGHT }}
          >
            <div className="py-1 pr-3 w-24 shrink-0">Time</div>
            <div className="py-1 pr-3 w-40 shrink-0">Event</div>
            <div className="py-1 pr-3 w-32 shrink-0">Actor</div>
            <div className="py-1 pr-3 w-36 shrink-0">Workspace</div>
            <div className="py-1 pr-3 flex-1">Summary</div>
          </div>

          {/* Virtualized rows */}
          <div style={{ height: totalHeight, position: "relative" }}>
            {visibleEntries.map((entry, vi) => {
              const idx = startIdx + vi;
              const isExpanded = idx === expandedEntryIndex;
              return (
                <div
                  key={idx}
                  style={{
                    position: "absolute",
                    top: rowTops[idx] ?? 0,
                    left: 0,
                    right: 0,
                    height: getRowHeight(idx),
                  }}
                >
                  <div
                    onClick={() => setExpandedEntry(isExpanded ? null : idx)}
                    className="flex items-center border-b border-zinc-900 hover:bg-zinc-900 cursor-pointer"
                    style={{ height: ROW_HEIGHT }}
                  >
                    <div className="py-1 pr-3 w-24 shrink-0 text-zinc-500" title={formatHlcFull(entry.timestamp)}>
                      {formatHlcTime(entry.timestamp)}
                    </div>
                    <div className="py-1 pr-3 w-40 shrink-0">
                      <EventBadge eventType={entry.eventType} />
                    </div>
                    <div className="py-1 pr-3 w-32 shrink-0 text-zinc-400 truncate">
                      {entry.actor}
                    </div>
                    <div className="py-1 pr-3 w-36 shrink-0">
                      {entry.workspaceId ? (
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            navigate(`/workspaces/${entry.workspaceId}`);
                          }}
                          className="text-blue-400 hover:text-blue-300 truncate"
                        >
                          {entry.workspaceId.slice(0, 12)}
                        </button>
                      ) : (
                        <span className="text-zinc-600">{"\u2014"}</span>
                      )}
                    </div>
                    <div className="py-1 pr-3 flex-1 text-zinc-400 truncate">
                      {summarize(entry)}
                    </div>
                  </div>

                  {/* Expanded detail */}
                  {isExpanded && (
                    <div className="bg-zinc-900 border border-zinc-800 rounded p-3 mx-1 text-xs overflow-auto" style={{ height: EXPANDED_HEIGHT }}>
                      <ExpandedDetail entry={entry} />
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

// ── Sub-components ──

function EventBadge({ eventType }: { eventType: string }) {
  const category = EVENT_CATEGORIES[eventType] ?? "workspace";
  const colors = CATEGORY_COLORS[category] ?? "bg-zinc-700 text-zinc-300";
  return (
    <span className={`inline-block px-1.5 py-0.5 rounded text-[10px] leading-tight ${colors}`}>
      {eventType}
    </span>
  );
}

function ExpandedDetail({ entry }: { entry: TrailEntry }) {
  // Parse body as JSON if possible, otherwise show raw
  let parsed: Record<string, unknown> | null = null;
  try {
    if (entry.body.startsWith("{")) {
      parsed = JSON.parse(entry.body) as Record<string, unknown>;
    }
  } catch {
    // not JSON
  }

  return (
    <div className="space-y-1">
      <div className="flex gap-4 text-zinc-500 mb-2">
        <span>Seq: {entry.sequenceNumber.toString()}</span>
        <span>Full timestamp: {formatHlcFull(entry.timestamp)}</span>
      </div>
      {parsed ? (
        <table className="text-xs">
          <tbody>
            {Object.entries(parsed).map(([key, value]) => (
              <tr key={key} className="border-b border-zinc-800">
                <td className="pr-4 py-0.5 text-zinc-500 align-top">{key}</td>
                <td className="py-0.5 text-zinc-300">
                  {typeof value === "object" ? JSON.stringify(value) : String(value)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <pre className="text-zinc-300 whitespace-pre-wrap break-all">
          {entry.body || "(empty)"}
        </pre>
      )}
    </div>
  );
}

function summarize(entry: TrailEntry): string {
  // Derive a one-line summary from event type
  switch (entry.eventType) {
    case "workspace_created":
      return `Workspace created`;
    case "workspace_state_changed":
      return `State changed`;
    case "workspace_terminated":
      return `Workspace terminated`;
    case "gate_triggered":
      return `Gate awaiting response`;
    case "gate_resolved":
      return `Gate resolved`;
    case "gate_timeout":
      return `Gate timed out`;
    case "human_injection":
      return `Envelope injected`;
    case "escalation_raised":
      return `Agent escalated`;
    case "escalation_resolved":
      return `Escalation resolved`;
    case "escalation_timeout":
      return `Escalation timed out`;
    case "envelope_sent":
      return `Envelope sent`;
    case "envelope_delivered":
      return `Envelope delivered`;
    case "signal_emitted":
      return `Signal emitted`;
    case "checkpoint_created":
      return `Checkpoint created`;
    case "task_status_changed":
      return `Task status changed`;
    case "task_created":
      return `Task created`;
    case "integration_started":
      return `Integration started`;
    case "integration_completed":
      return `Integration completed`;
    default:
      return entry.eventType;
  }
}
