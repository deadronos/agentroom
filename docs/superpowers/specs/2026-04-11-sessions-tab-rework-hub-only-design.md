# Sessions Tab Rework — Hub-Only, Auto-Populate

## Status

Draft — do not implement until approved.

## Overview

Replace the CASS-based Sessions tab with a direct WebSocket connection to the Session Hub. The session list is populated dynamically from the hub's `StateSync`, with no hardcoded agent list — any adapter that produces sessions appears automatically.

---

## Architecture

### Data Source

The Sessions tab connects via WebSocket to `ws://localhost:8081` (the hub's frontend port). It receives:

- `StateSync { sessions: ActiveSession[] }` — full snapshot on connect
- `SessionStarted { ... }` — new session appears in list
- `SessionEnded { ... }` — session marked as ended (grayed out or removed based on staleness)
- `Activity { ... }` — session card updates with latest tool/message preview

The tab no longer calls `listAllSessions()` (CASS) or `searchSessions()`. Historical search can be re-added later with a separate CASS integration, but the default view is live hub state.

### Hub Session vs. Historical Session

The hub tracks **active** sessions (with `last_activity` timestamps). These sessions have a different shape than CASS sessions:

```typescript
interface HubActiveSession {
  session_id: string;
  provider: string;       // "claude", "codex", "openclaw", etc.
  agent_id: string | null;
  agent_type: string;     // "main" | "subagent"
  model: string;
  status: string;
  last_activity: number;  // Unix ms timestamp
  project: string | null;
  last_message: string | null;
  last_tool: string | null;
  last_tool_input: string | null;
  parent_session_id: string | null;
}
```

This maps to the existing `Session` interface with minor differences — the frontend needs an adapter layer to normalize hub sessions into `Session` objects for the existing component props.

### Connection State

Track WebSocket state:
- `connecting` — initial connection pending
- `connected` — receiving events
- `disconnected` — hub unavailable; show banner "Hub offline, waiting to reconnect..."
- `reconnecting` — exponential backoff retry

---

## UI Layout

```
┌─────────────────────────────────────────────────────────────┐
│  [SearchBar: query input + filters]                        │
├──────────────┬────────────────────────────────────────────┤
│              │                                             │
│  Session     │   (hidden by default)                       │
│  List       │                                             │
│  (flat or   │   Transcript / Session Viewer               │
│  grouped)   │   slides in when session is clicked          │
│              │   clicking same session or X button hides   │
│              │                                             │
├──────────────┴────────────────────────────────────────────┤
│  StatusBar                                                 │
└────────────────────────────────────────────────────────────┘
```

### Left Panel: Session List

- Scrollable list of session cards
- View toggle: Grouped (by project) / Flat — same as current
- Filter bar at top (see Filter Bar section)
- Loading skeleton while connecting
- "No sessions available" empty state when list is empty and not searching

### Right Panel: Transcript Viewer

- Hidden by default (zero width)
- Slides in when a session card is clicked
- Clicking the same session again hides it
- Close button ("×") in top-right corner
- Shows full transcript via `getSessionTranscript(sourcePath)` — same CASS call as before for historical data, but triggered on-demand per session
- Header with session metadata (provider, model, project, duration)
- Message list with role coloring

### Responsive

- **Desktop** (>1024px): side-by-side panels
- **Mobile/narrow**: clicking a session replaces the list with the viewer; back button returns to list

---

## Filter Bar

Filters apply to the hub session list client-side (no hub round-trip needed for basic filtering).

- **Provider dropdown** — populated dynamically from `Set(sessions.map(s => s.provider))`. If only claude and gemini have sessions, only those appear. Never hardcoded.
- **Category filter** — from tags service (unchanged)
- **Date range** — filter by `last_activity` relative to now (e.g., "today", "last 7 days", "last 30 days")
- **Subagent toggle** — show/hide sessions where `agent_type === "subagent"`
- **Search query** — local filter on `last_message`, `project`, `last_tool` fields (no CASS call)

---

## Session Card

```
┌──────────────────────────────────────┐
│ [Icon] Session title / last_message  │
│          provider · project · 2m ago │
│          [●] status                  │
└──────────────────────────────────────┘
```

Fields displayed:
- **Agent icon** — first letter of provider in a colored circle (claude=blue, codex=green, openclaw=orange, etc.)
- **Title** — `last_message` preview (first 60 chars), or "Untitled session"
- **Provider badge** — small pill showing provider name
- **Project** — basename of `project` path
- **Relative time** — computed from `last_activity`
- **Status dot** — green if recent activity (< 2 min), gray otherwise

---

## Tagging

Tagging panel (right-click or "tag" button) remains unchanged — calls `tagSession()` via Tauri invoke. Tags display in session cards and in the transcript viewer header.

---

## Real-Time Updates

When WebSocket receives:
- `SessionStarted` — add session to list with animation
- `SessionEnded` — update session status (grayed out, stays in list for 5 min then can be removed or kept based on preference)
- `Activity` — update `last_message` / `last_tool` in the matching card without re-rendering the whole list

---

## Implementation Scope

### Phase 1: Core Rewire (this spec)

1. New `src/services/hub.ts` — WebSocket client for hub connection
2. New `src/adapters/hubSession.ts` — normalize `ActiveSession` → `Session` interface
3. `src/components/SessionList.tsx` — accept `Session[]` from hub, remove CASS calls
4. `src/components/SearchBar.tsx` — update to use hub provider set, local search
5. `src/components/SessionPanel.tsx` — toggle visibility via parent state
6. `src/App.tsx` — manage selected session state, panel visibility, hub WebSocket lifecycle
7. Remove CASS calls from `App.tsx` load flow (keep `getSessionTranscript` for on-demand loading)

### Phase 2: TBD (out of scope for this spec)

- Historical search via CASS (separate "History" tab or modal)
- Real-time transcript updates via hub events

---

## Files to Change

| File | Change |
|------|--------|
| `src/services/hub.ts` | New — WebSocket client |
| `src/adapters/hubSession.ts` | New — ActiveSession → Session normalizer |
| `src/App.tsx` | Hub connection lifecycle, session state |
| `src/components/SearchBar.tsx` | Dynamic provider set from hub |
| `src/components/SessionList.tsx` | Remove CASS, accept hub sessions |
| `src/components/SessionPanel.tsx` | Visibility toggle support |
| `src/types.ts` | Add `HubActiveSession` type |

---

## Open Questions

1. **Session expiry**: should ended sessions disappear from the list immediately, stay for a while (and how long), or persist until the user navigates away?
2. **Subagent display**: should subagents be indented under their parent session, or shown as flat separate cards?
3. **Grouped view default**: should grouped view be the default, or flat?
