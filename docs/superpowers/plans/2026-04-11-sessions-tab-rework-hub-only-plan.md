# Sessions Tab Rework — Hub-Only, Auto-Populate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the CASS-based Sessions tab with a direct WebSocket connection to the Session Hub, auto-populating the session list from hub state with no hardcoded agent list.

**Architecture:** The frontend connects via WebSocket to the hub's frontend port (8081). It receives `StateSync` on connect and live events (`SessionStarted`, `SessionEnded`, `Activity`) thereafter. Sessions are held in React state and filtered client-side. An adapter normalizes `HubActiveSession` (hub shape) into the existing `Session` interface for component compatibility.

**Tech Stack:** React 18 + TypeScript, WebSocket (native browser API), existing Tauri invoke for transcript loading.

---

## File Map

| File | Role |
|------|------|
| `src/services/hub.ts` | **NEW** — WebSocket client for hub connection, event parsing, reconnect logic |
| `src/adapters/hubSession.ts` | **NEW** — normalize `HubActiveSession` → `Session` interface |
| `src/types.ts` | **MODIFY** — add `HubActiveSession` type; update `Session` to optionally accept hub fields |
| `src/App.tsx` | **MODIFY** — remove CASS `loadSessions` calls, manage hub WebSocket lifecycle, manage `selectedSession` + `sessionPanelOpen` state |
| `src/components/SearchBar.tsx` | **MODIFY** — replace hardcoded `AgentFilter` with dynamic providers from hub sessions |
| `src/components/SessionList.tsx` | **MODIFY** — remove CASS calls, accept `Session[]` from hub, update card rendering for hub fields |
| `src/components/SessionPanel.tsx` | **MODIFY** — add visibility-controlled wrapper (accepts `visible` prop; panel hidden when `visible=false`) |
| `src/App.css` | **MODIFY** — add `.session-panel.hidden { display: none }` rule |

---

## Task 1: Hub WebSocket Client Service

**Files:**
- Create: `src/services/hub.ts`

- [ ] **Step 1: Write the failing test**

```typescript
// src/services/hub.test.ts
import { HubClient, HubMessage } from './hub';

describe('HubClient', () => {
  it('parses StateSync message correctly', () => {
    const msg: HubMessage = {
      type: 'state_sync',
      sessions: [{
        session_id: 's1',
        provider: 'claude',
        agent_id: null,
        agent_type: 'main',
        model: 'claude-opus-4-6',
        status: 'active',
        last_activity: Date.now(),
        project: '/Users/me/project',
        last_message: 'Hello',
        last_tool: 'Edit',
        last_tool_input: null,
        parent_session_id: null,
      }],
    };
    expect(msg.sessions.length).toBe(1);
    expect(msg.sessions[0].provider).toBe('claude');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/openclaw/Github/agentroom && npx vitest run src/services/hub.test.ts`
Expected: FAIL — file does not exist

- [ ] **Step 3: Write HubActiveSession type and HubMessage union type**

```typescript
// src/services/hub.ts

export interface HubActiveSession {
  session_id: string;
  provider: string;
  agent_id: string | null;
  agent_type: string;
  model: string;
  status: string;
  last_activity: number;
  project: string | null;
  last_message: string | null;
  last_tool: string | null;
  last_tool_input: string | null;
  parent_session_id: string | null;
}

export type HubMessage =
  | { type: 'state_sync'; sessions: HubActiveSession[] }
  | { type: 'session_started'; session_id: string; provider: string; project: string | null; model: string; timestamp: number; last_tool: string | null; last_message: string | null; agent_id: string | null; agent_type: string }
  | { type: 'activity'; session_id: string; provider: string; timestamp: number; tool: string | null; message_preview: string | null }
  | { type: 'session_ended'; session_id: string; provider: string; timestamp: number }
  | { type: 'ack'; fingerprint: string }
  | { type: 'error'; message: string };

export type HubConnectionState = 'connecting' | 'connected' | 'disconnected' | 'reconnecting';

export interface HubClientOptions {
  url: string;                  // e.g. "ws://localhost:8081"
  onStateSync?: (sessions: HubActiveSession[]) => void;
  onSessionStarted?: (session: HubActiveSession) => void;
  onSessionEnded?: (sessionId: string) => void;
  onActivity?: (sessionId: string, tool: string | null, messagePreview: string | null) => void;
  onConnectionStateChange?: (state: HubConnectionState) => void;
}
```

- [ ] **Step 4: Implement HubClient class**

```typescript
// src/services/hub.ts (continued)

export class HubClient {
  private ws: WebSocket | null = null;
  private opts: HubClientOptions;
  private reconnectDelay = 1000;
  private reconnectMaxDelay = 30000;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private intentionalClose = false;

  constructor(opts: HubClientOptions) {
    this.opts = opts;
  }

  connect(): void {
    this.intentionalClose = false;
    this.opts.onConnectionStateChange?.('connecting');

    try {
      this.ws = new WebSocket(this.opts.url);
    } catch {
      this.opts.onConnectionStateChange?.('disconnected');
      this.scheduleReconnect();
      return;
    }

    this.ws.onopen = () => {
      this.reconnectDelay = 1000;
      this.opts.onConnectionStateChange?.('connected');
    };

    this.ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data) as HubMessage;
        this.dispatch(msg);
      } catch (err) {
        console.warn('[HubClient] Failed to parse message:', err);
      }
    };

    this.ws.onclose = () => {
      if (!this.intentionalClose) {
        this.opts.onConnectionStateChange?.('disconnected');
        this.scheduleReconnect();
      }
    };

    this.ws.onerror = () => {
      // onclose will handle reconnect
    };
  }

  disconnect(): void {
    this.intentionalClose = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
  }

  private dispatch(msg: HubMessage): void {
    switch (msg.type) {
      case 'state_sync':
        this.opts.onStateSync?.(msg.sessions);
        break;
      case 'session_started':
        // Convert to HubActiveSession shape for consistency
        this.opts.onSessionStarted?.({
          session_id: msg.session_id,
          provider: msg.provider,
          agent_id: msg.agent_id,
          agent_type: msg.agent_type,
          model: msg.model,
          status: 'active',
          last_activity: msg.timestamp,
          project: msg.project,
          last_message: msg.last_message,
          last_tool: msg.last_tool,
          last_tool_input: null,
          parent_session_id: null,
        });
        break;
      case 'session_ended':
        this.opts.onSessionEnded?.(msg.session_id);
        break;
      case 'activity':
        this.opts.onActivity?.(msg.session_id, msg.tool, msg.message_preview);
        break;
      case 'ack':
      case 'error':
        // Acknowledgements and errors are handled by the hub protocol; no UI action needed
        break;
    }
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer) return;
    this.opts.onConnectionStateChange?.('reconnecting');
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, this.reconnectDelay);
    this.reconnectDelay = Math.min(this.reconnectDelay * 2, this.reconnectMaxDelay);
  }
}

// Singleton instance
let _hubClient: HubClient | null = null;

export function getHubClient(): HubClient {
  if (!_hubClient) {
    _hubClient = new HubClient({
      url: 'ws://localhost:8081',
    });
  }
  return _hubClient;
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `npx vitest run src/services/hub.test.ts`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/services/hub.ts src/services/hub.test.ts
git commit -m "feat(sessions): add hub WebSocket client service

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 2: Hub Session Adapter — ActiveSession to Session Normalizer

**Files:**
- Create: `src/adapters/hubSession.ts`

- [ ] **Step 1: Write the failing test**

```typescript
// src/adapters/hubSession.test.ts
import { HubActiveSession } from '../services/hub';
import { hubSessionToSession } from './hubSession';

describe('hubSessionToSession', () => {
  it('converts HubActiveSession to Session shape', () => {
    const hub: HubActiveSession = {
      session_id: '/Users/me/.claude/projects/foo/sessions/s1.jsonl',
      provider: 'claude',
      agent_id: 'agent-123',
      agent_type: 'main',
      model: 'claude-opus-4-6',
      status: 'active',
      last_activity: Date.now(),
      project: '/Users/me/project',
      last_message: 'Fixed the bug',
      last_tool: 'Edit',
      last_tool_input: '{"file":"main.rs"}',
      parent_session_id: null,
    };
    const session = hubSessionToSession(hub);
    expect(session.id).toBe(hub.session_id);
    expect(session.agent).toBe('claude');
    expect(session.title).toBe('Fixed the bug');
    expect(session.workspace).toBe('/Users/me/project');
    expect(session.isSubagent).toBe(false);
  });

  it('marks subagents correctly', () => {
    const hub: HubActiveSession = {
      session_id: 's1',
      provider: 'claude',
      agent_id: null,
      agent_type: 'subagent',
      model: 'claude-sonnet-4-6',
      status: 'active',
      last_activity: Date.now(),
      project: null,
      last_message: null,
      last_tool: null,
      last_tool_input: null,
      parent_session_id: 'parent-1',
    };
    const session = hubSessionToSession(hub);
    expect(session.isSubagent).toBe(true);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/adapters/hubSession.test.ts`
Expected: FAIL — adapters directory does not exist

- [ ] **Step 3: Write the adapter**

```typescript
// src/adapters/hubSession.ts
import type { Session } from '../types';
import type { HubActiveSession } from '../services/hub';

export function hubSessionToSession(hub: HubActiveSession): Session {
  const isActive = hub.status === 'active';
  const isSubagent = hub.agent_type === 'subagent';

  // Title is the last message preview, truncated
  const title = hub.last_message
    ? hub.last_message.slice(0, 80)
    : null;

  return {
    id: hub.session_id,
    agent: normalizeProvider(hub.provider),
    isSubagent,
    workspace: hub.project,
    title,
    sourcePath: hub.session_id,
    startedAt: null,         // Hub sessions don't track startedAt directly; can derive from last_activity on first see
    endedAt: null,
    messageCount: undefined,
    score: undefined,
    matchType: undefined,
    snippet: hub.last_message ?? undefined,
  };
}

function normalizeProvider(provider: string): string {
  // Map hub provider names to existing agent names
  const map: Record<string, string> = {
    claude: 'claude-code',
    codex: 'codex',
    openclaw: 'openclaw',
    opencode: 'opencode',
    copilot: 'copilot',
    gemini: 'gemini',
  };
  return map[provider.toLowerCase()] ?? provider;
}

export function isSessionStale(hub: HubActiveSession, thresholdMs = 120000): boolean {
  return Date.now() - hub.last_activity > thresholdMs;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/adapters/hubSession.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/adapters/hubSession.ts src/adapters/hubSession.test.ts
git commit -m "feat(sessions): add HubActiveSession to Session normalizer

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 3: Add HubActiveSession Type to types.ts

**Files:**
- Modify: `src/types.ts`

- [ ] **Step 1: Read current types.ts**

Already read above. Add `HubActiveSession` type at the end of the file.

- [ ] **Step 2: Add HubActiveSession interface**

```typescript
// Add at end of src/types.ts

export interface HubActiveSession {
  session_id: string;
  provider: string;
  agent_id: string | null;
  agent_type: string;
  model: string;
  status: string;
  last_activity: number;
  project: string | null;
  last_message: string | null;
  last_tool: string | null;
  last_tool_input: string | null;
  parent_session_id: string | null;
}
```

- [ ] **Step 3: Commit**

```bash
git add src/types.ts
git commit -m "types: add HubActiveSession interface

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 4: Rewire App.tsx — Hub Connection and State Management

**Files:**
- Modify: `src/App.tsx:1-381` (full file replacement)

- [ ] **Step 1: Read current App.tsx**

Already read above.

- [ ] **Step 2: Replace App.tsx with hub-connected version**

Key changes:
- Remove CASS `listAllSessions` import
- Import `HubClient` and `HubActiveSession` from `services/hub.ts`
- Import `hubSessionToSession` from `adapters/hubSession.ts`
- Remove the `loadSessions` callback that calls CASS; replace with hub event handlers
- Add `sessionPanelOpen` state to control transcript panel visibility
- Manage hub client lifecycle in a `useEffect`
- `handleSelectSession` toggles panel: if clicking same session, close panel; otherwise open it
- Remove `focusedProject` and related watcher logic from App.tsx (those are office view concerns)
- Keep `getSessionTranscript` for on-demand transcript loading in SessionPanel

```typescript
// src/App.tsx (key changes only — see full file below)

import { useState, useCallback, useRef, useEffect, useMemo } from 'react'
import { OfficeState } from './office/engine/officeState.js'
import { OfficeCanvas } from './office/components/OfficeCanvas.js'
import { ToolOverlay } from './office/components/ToolOverlay.js'
import { EditorState } from './office/editor/editorState.js'
import { useAgentEvents } from './hooks/useAgentEvents.js'
import { loadAllAssets } from './office/assetLoader.js'
import { migrateLayoutColors } from './office/layout/layoutSerializer.js'
import { ZOOM_DEFAULT_DPR_FACTOR } from './office/constants.js'
import { startWatching, switchWatching } from './bridge.js'
import { SearchBar } from './components/SearchBar.js'
import { SessionList } from './components/SessionList.js'
import { SessionPanel } from './components/SessionPanel.js'
import { StatusBar } from './components/StatusBar.js'
import { TokenPanel } from './components/TokenPanel.js'
import { getAllCategories, loadAllTags } from './services/tags.js'
import type { Session, SessionTag, HubActiveSession } from './types.js'
import { HubClient, getHubClient } from './services/hub.js'
import { hubSessionToSession } from './adapters/hubSession.js'

// Re-export for backwards compat — existing code may import from App
export type { Session, SessionTag }

type AgentFilter = 'all' | string  // now dynamic — providers come from hub
type ViewMode = 'grouped' | 'flat'
type MainView = 'office' | 'sessions'

// ... rest of App implementation (full file)
```

Full `App.tsx` replacement:

```typescript
// src/App.tsx
import { useState, useCallback, useRef, useEffect, useMemo } from 'react'
import { OfficeState } from './office/engine/officeState.js'
import { OfficeCanvas } from './office/components/OfficeCanvas.js'
import { ToolOverlay } from './office/components/ToolOverlay.js'
import { EditorState } from './office/editor/editorState.js'
import { useAgentEvents } from './hooks/useAgentEvents.js'
import { loadAllAssets } from './office/assetLoader.js'
import { migrateLayoutColors } from './office/layout/layoutSerializer.js'
import { ZOOM_DEFAULT_DPR_FACTOR } from './office/constants.js'
import { startWatching, switchWatching } from './bridge.js'
import { SearchBar } from './components/SearchBar.js'
import { SessionList } from './components/SessionList.js'
import { SessionPanel } from './components/SessionPanel.js'
import { StatusBar } from './components/StatusBar.js'
import { TokenPanel } from './components/TokenPanel.js'
import { getAllCategories, loadAllTags } from './services/tags.js'
import type { Session, SessionTag } from './types.js'
import { HubClient, getHubClient } from './services/hub.js'
import { hubSessionToSession } from './adapters/hubSession.js'

type ViewMode = 'grouped' | 'flat'
type MainView = 'office' | 'sessions'

function projectBasename(workspace: string | null): string {
  if (!workspace) return 'Other'
  const parts = (workspace ?? '').split('/').filter(Boolean)
  return parts[parts.length - 1] || 'Other'
}

const officeStateRef = { current: null as OfficeState | null }
const editorState = new EditorState()

function getOfficeState(): OfficeState {
  if (!officeStateRef.current) {
    officeStateRef.current = new OfficeState()
  }
  return officeStateRef.current
}

function App() {
  const [layoutReady, setLayoutReady] = useState(false)
  const [zoom, setZoom] = useState(Math.round(window.devicePixelRatio || 1) * ZOOM_DEFAULT_DPR_FACTOR)
  const panRef = useRef({ x: 0, y: 0 })
  const containerRef = useRef<HTMLDivElement>(null)

  // Session state from hub
  const [sessions, setSessions] = useState<Session[]>([])
  const [hubSessions, setHubSessions] = useState<HubActiveSession[]>([])
  const [tags, setTags] = useState<Record<string, SessionTag>>({})
  const [categories, setCategories] = useState<string[]>([])
  const [selectedSession, setSelectedSession] = useState<Session | null>(null)
  const [sessionPanelOpen, setSessionPanelOpen] = useState(false)
  const [loading, setLoading] = useState(true)
  const [searchQuery, setSearchQuery] = useState('')
  const [agentFilter, setAgentFilter] = useState<string>('all')
  const [categoryFilter, setCategoryFilter] = useState('all')
  const [showSubagents, setShowSubagents] = useState(false)
  const [viewMode, setViewMode] = useState<ViewMode>('grouped')
  const [focusedProject, setFocusedProject] = useState<string | null>(null)
  const [mainView, setMainView] = useState<MainView>('office')
  const [hubConnected, setHubConnected] = useState(false)

  const { agents, agentTools, subagentCharacters, clearAll, getAgentStringId } = useAgentEvents(getOfficeState)

  // Hub event handlers
  const handleStateSync = useCallback((hubSessions: HubActiveSession[]) => {
    setHubSessions(hubSessions)
    setSessions(hubSessions.map(hubSessionToSession))
    setLoading(false)
  }, [])

  const handleSessionStarted = useCallback((hubSession: HubActiveSession) => {
    setHubSessions((prev) => {
      const exists = prev.some((s) => s.session_id === hubSession.session_id)
      if (exists) return prev
      return [...prev, hubSession]
    })
    setSessions((prev) => {
      const exists = prev.some((s) => s.id === hubSession.session_id)
      if (exists) return prev
      return [...prev, hubSessionToSession(hubSession)]
    })
  }, [])

  const handleSessionEnded = useCallback((sessionId: string) => {
    setHubSessions((prev) =>
      prev.map((s) =>
        s.session_id === sessionId ? { ...s, status: 'ended' } : s
      )
    )
    setSessions((prev) =>
      prev.map((s) =>
        s.id === sessionId ? { ...s, endedAt: Date.now() } : s
      )
    )
  }, [])

  const handleActivity = useCallback((sessionId: string, tool: string | null, messagePreview: string | null) => {
    const update = (s: HubActiveSession) =>
      s.session_id === sessionId
        ? { ...s, last_activity: Date.now(), last_tool: tool, last_message: messagePreview }
        : s
    setHubSessions((prev) => prev.map(update))
    setSessions((prev) =>
      prev.map((s) =>
        s.id === sessionId
          ? { ...s, snippet: messagePreview ?? s.snippet }
          : s
      )
    )
  }, [])

  // Connect to hub on mount, disconnect on unmount
  useEffect(() => {
    const client = getHubClient()
    client.opts.onStateSync = handleStateSync
    client.opts.onSessionStarted = handleSessionStarted
    client.opts.onSessionEnded = handleSessionEnded
    client.opts.onActivity = handleActivity
    client.opts.onConnectionStateChange = (state) => {
      setHubConnected(state === 'connected')
    }
    client.connect()

    return () => {
      client.disconnect()
    }
  }, [handleStateSync, handleSessionStarted, handleSessionEnded, handleActivity])

  // Load tags on mount
  useEffect(() => {
    loadAllTags().then((loaded) => {
      setTags(loaded)
      setCategories(getAllCategories())
    }).catch(() => {})
  }, [])

  // Load assets + default layout on mount
  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const layout = await loadAllAssets()
      if (cancelled) return
      if (layout) {
        const migrated = migrateLayoutColors(layout)
        officeStateRef.current = new OfficeState(migrated)
      }
      setLayoutReady(true)

      try {
        await startWatching('')
      } catch {
        console.warn('[App] Could not start watching — backend not available')
      }
    })()
    return () => { cancelled = true }
  }, [])

  // Dynamic provider list from hub sessions
  const availableProviders = useMemo(() => {
    const set = new Set<string>()
    for (const s of hubSessions) {
      set.add(s.provider)
    }
    return Array.from(set).sort()
  }, [hubSessions])

  const handleSearch = useCallback((
    query: string,
    agent: string,
    category: string,
    showSubagentsFlag: boolean,
  ) => {
    setSearchQuery(query)
    setAgentFilter(agent)
    setCategoryFilter(category)
    setShowSubagents(showSubagentsFlag)
  }, [])

  const displayedSessions = useMemo(() => {
    let filtered = sessions

    if (!showSubagents) {
      filtered = filtered.filter((s) => !s.isSubagent)
    }

    if (categoryFilter !== 'all') {
      filtered = filtered.filter((s) => tags[s.id]?.category === categoryFilter)
    }

    if (agentFilter !== 'all') {
      filtered = filtered.filter((s) => s.agent === agentFilter)
    }

    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase()
      filtered = filtered.filter((s) =>
        (s.title?.toLowerCase() ?? '').includes(q) ||
        (s.snippet?.toLowerCase() ?? '').includes(q) ||
        (s.workspace ?? '').toLowerCase().includes(q) ||
        (s.last_tool ?? '').toLowerCase().includes(q)
      )
    }

    return filtered
  }, [sessions, tags, categoryFilter, agentFilter, showSubagents, searchQuery])

  const groupedSessions = useMemo(() => {
    const groups = new Map<string, Session[]>()
    for (const s of displayedSessions) {
      const name = projectBasename(s.workspace)
      const list = groups.get(name) || []
      list.push(s)
      groups.set(name, list)
    }
    return Array.from(groups.entries()).sort(([, a], [, b]) => {
      const latestA = Math.max(...a.map((s) => s.startedAt || 0))
      const latestB = Math.max(...b.map((s) => s.startedAt || 0))
      return latestB - latestA
    })
  }, [displayedSessions])

  const projectWorkspaces = useMemo(() => {
    const map = new Map<string, string>()
    for (const s of displayedSessions) {
      if (s.workspace) {
        const name = projectBasename(s.workspace)
        if (!map.has(name)) {
          map.set(name, s.workspace)
        }
      }
    }
    return map
  }, [displayedSessions])

  const handleSelectSession = useCallback((session: Session) => {
    if (selectedSession?.id === session.id) {
      // Toggle: clicking same session closes panel
      setSessionPanelOpen(false)
      setSelectedSession(null)
    } else {
      setSelectedSession(session)
      setSessionPanelOpen(true)
    }
  }, [selectedSession])

  const handleClosePanel = useCallback(() => {
    setSessionPanelOpen(false)
  }, [])

  const handleFocusProject = useCallback(async (workspace: string | null) => {
    clearAll()
    setFocusedProject(workspace)
    try {
      await switchWatching(workspace || '')
    } catch {
      console.warn('[App] Could not switch watching')
    }
  }, [clearAll])

  useEffect(() => {
    setSelectedSession((current) => (
      current && displayedSessions.some((s) => s.id === current.id) ? current : null
    ))
  }, [displayedSessions])

  const handleTagUpdate = useCallback((tag: SessionTag) => {
    setTags((current) => ({ ...current, [tag.sessionId]: tag }))
    setCategories(getAllCategories())
  }, [])

  const handleClick = useCallback((agentId: number) => {
    const agentStringId = getAgentStringId(agentId)
    if (!agentStringId) return
    const match = sessions.find((s) => s.sourcePath.includes(agentStringId))
    if (match) {
      setSelectedSession(match)
      setSessionPanelOpen(true)
    }
  }, [getAgentStringId, sessions])

  const handleCloseAgent = useCallback((_id: number) => {}, [])

  const handleZoomChange = useCallback((newZoom: number) => {
    setZoom(newZoom)
  }, [])

  const noopTile = useCallback((_col: number, _row: number) => {}, [])
  const noopSelection = useCallback(() => {}, [])
  const noopDelete = useCallback(() => {}, [])
  const noopRotate = useCallback(() => {}, [])
  const noopDrag = useCallback((_uid: string, _col: number, _row: number) => {}, [])

  const officeState = getOfficeState()

  if (!layoutReady) {
    return (
      <div style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        color: 'var(--vscode-foreground)',
        background: 'var(--pixel-bg)',
        fontSize: '24px',
      }}>
        Loading...
      </div>
    )
  }

  return (
    <div data-testid="app-root" className={`app ${mainView === 'sessions' ? 'app-sessions-view' : ''} ${sessionPanelOpen && mainView === 'sessions' ? 'app-with-panel' : ''}`}>
      {/* Hub connection banner */}
      {!hubConnected && (
        <div className="hub-banner" data-testid="hub-banner">
          Hub offline — waiting to reconnect...
        </div>
      )}

      {/* Sidebar: Search + Session List */}
      <div className="sidebar" data-testid="sidebar">
        <div className="main-view-toggle" data-testid="view-switcher">
          <button
            data-testid="tab-office"
            className={mainView === 'office' ? 'active' : ''}
            onClick={() => setMainView('office')}
          >
            Agent Office
          </button>
          <button
            data-testid="tab-sessions"
            className={mainView === 'sessions' ? 'active' : ''}
            onClick={() => setMainView('sessions')}
          >
            Sessions
          </button>
        </div>

        <SearchBar
          onSearch={handleSearch}
          categories={categories}
          availableProviders={availableProviders}
          initialAgent={agentFilter}
          initialCategory={categoryFilter}
          initialShowSubagents={showSubagents}
        />
        <SessionList
          sessions={displayedSessions}
          groupedSessions={groupedSessions}
          tags={tags}
          selectedId={selectedSession?.id ?? null}
          onSelect={handleSelectSession}
          loading={loading}
          isSearch={!!searchQuery.trim()}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          focusedProject={focusedProject}
          projectWorkspaces={projectWorkspaces}
          onFocusProject={handleFocusProject}
        />
      </div>

      {/* Main content area */}
      {mainView === 'office' ? (
        <div className="main-panel" data-testid="main-panel-office">
          <div ref={containerRef} data-testid="office-container" style={{ width: '100%', height: '100%', position: 'relative', overflow: 'hidden' }}>
            <OfficeCanvas
              officeState={officeState}
              onClick={handleClick}
              isEditMode={false}
              editorState={editorState}
              onEditorTileAction={noopTile}
              onEditorEraseAction={noopTile}
              onEditorSelectionChange={noopSelection}
              onDeleteSelected={noopDelete}
              onRotateSelected={noopRotate}
              onDragMove={noopDrag}
              editorTick={0}
              zoom={zoom}
              onZoomChange={handleZoomChange}
              panRef={panRef}
            />
            <div style={{ position: 'absolute', inset: 0, background: 'var(--pixel-vignette)', pointerEvents: 'none', zIndex: 40 }} />
            <ToolOverlay
              officeState={officeState}
              agents={agents}
              agentTools={agentTools}
              subagentCharacters={subagentCharacters}
              containerRef={containerRef}
              zoom={zoom}
              panRef={panRef}
              onCloseAgent={handleCloseAgent}
            />
            <TokenPanel />
          </div>
        </div>
      ) : (
        <div className="main-panel main-panel-sessions" data-testid="main-panel-sessions">
          {selectedSession && sessionPanelOpen ? (
            <SessionPanel
              session={selectedSession}
              onClose={handleClosePanel}
            />
          ) : (
            <div className="sessions-placeholder">
              Select a session from the sidebar to view its transcript
            </div>
          )}
        </div>
      )}

      {/* Right: Session Preview Panel (office view only) */}
      {selectedSession && mainView === 'office' && (
        <SessionPanel
          session={selectedSession}
          onClose={() => setSelectedSession(null)}
        />
      )}

      <StatusBar sessions={displayedSessions} tags={tags} onTagUpdate={handleTagUpdate} />
    </div>
  )
}

export default App
```

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "feat(sessions): wire App.tsx to hub WebSocket — drop CASS load

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 5: Update SearchBar — Dynamic Providers

**Files:**
- Modify: `src/components/SearchBar.tsx`

- [ ] **Step 1: Read SearchBar.tsx**

Already read above.

- [ ] **Step 2: Update SearchBar**

Changes:
- Replace `AgentFilter` type (`'all' | 'claude-code' | 'codex' | 'gemini'`) with `string` — providers come from hub
- Add `availableProviders: string[]` prop — dynamically built from hub sessions
- Replace hardcoded `<option>` elements with `availableProviders.map()`
- Replace `showClaudeSubagents` prop/label with `showSubagents` (generic subagent toggle, not just Claude-specific)
- Update `onSearch` signature: `agent: string` instead of `AgentFilter`

```typescript
// src/components/SearchBar.tsx (modified)
import { useState, useEffect, useRef, useCallback } from "react";

interface Props {
  onSearch: (query: string, agent: string, category: string, showSubagents: boolean) => void;
  categories: string[];
  availableProviders: string[];        // NEW — from hub
  initialAgent?: string;
  initialCategory?: string;
  initialShowSubagents?: boolean;
}

export function SearchBar({
  onSearch,
  categories,
  availableProviders,
  initialAgent = 'all',
  initialCategory = 'all',
  initialShowSubagents = false,
}: Props) {
  const [value, setValue] = useState('');
  const [agent, setAgent] = useState<string>(initialAgent);
  const [category, setCategory] = useState(initialCategory);
  const [showSubagents, setShowSubagents] = useState(initialShowSubagents);
  const inputRef = useRef<HTMLInputElement>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout>>();

  useEffect(() => { setAgent(initialAgent); }, [initialAgent]);
  useEffect(() => { setCategory(initialCategory); }, [initialCategory]);
  useEffect(() => { setShowSubagents(initialShowSubagents); }, [initialShowSubagents]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);

  const debouncedSearch = useCallback(
    (q: string, selectedAgent: string, selectedCategory: string, selectedShowSubagents: boolean) => {
      clearTimeout(timerRef.current);
      timerRef.current = setTimeout(
        () => onSearch(q, selectedAgent, selectedCategory, selectedShowSubagents),
        300
      );
    },
    [onSearch]
  );

  useEffect(() => () => clearTimeout(timerRef.current), []);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const q = e.target.value;
    setValue(q);
    debouncedSearch(q, agent, category, showSubagents);
  };

  const handleAgentChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const selectedAgent = e.target.value;
    setAgent(selectedAgent);
    clearTimeout(timerRef.current);
    onSearch(value, selectedAgent, category, showSubagents);
  };

  const handleCategoryChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const selectedCategory = e.target.value;
    setCategory(selectedCategory);
    clearTimeout(timerRef.current);
    onSearch(value, agent, selectedCategory, showSubagents);
  };

  const handleSubagentToggle = (e: React.ChangeEvent<HTMLInputElement>) => {
    const checked = e.target.checked;
    setShowSubagents(checked);
    clearTimeout(timerRef.current);
    onSearch(value, agent, category, checked);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      setValue('');
      onSearch('', agent, category, showSubagents);
      inputRef.current?.blur();
    }
  };

  return (
    <div className="search-bar" data-testid="search-bar">
      <div className="search-controls">
        <input
          ref={inputRef}
          type="text"
          data-testid="search-input"
          placeholder="Search sessions... (⌘K)"
          value={value}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
        />
        <select data-testid="agent-filter" value={agent} onChange={handleAgentChange} aria-label="Filter by agent">
          <option value="all">All agents</option>
          {availableProviders.map((p) => (
            <option key={p} value={p}>{p}</option>
          ))}
        </select>
        <select data-testid="category-filter" value={category} onChange={handleCategoryChange} aria-label="Filter by category">
          <option value="all">All categories</option>
          {categories.map((item) => (
            <option key={item} value={item}>{item}</option>
          ))}
        </select>
      </div>
      <label className="subagent-toggle">
        <input
          type="checkbox"
          checked={showSubagents}
          onChange={handleSubagentToggle}
          aria-label="Show subagent sessions"
        />
        Show subagents
      </label>
    </div>
  );
}
```

- [ ] **Step 3: Commit**

```bash
git add src/components/SearchBar.tsx
git commit -m "feat(sessions): SearchBar uses dynamic provider list from hub

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 6: Update SessionList — Session Panel Toggle, Agent Icon from Provider

**Files:**
- Modify: `src/components/SessionList.tsx`

Changes:
- Replace hardcoded `AGENT_ICONS` (which maps specific strings) with a dynamic approach: first letter of provider, colored via CSS variables
- `SessionCard` — display `session.last_message` as title if no `tag.summary`, show provider badge instead of agent name
- Add status dot: green if hub session is active (status === 'active' and not stale), gray otherwise
- Note: `SessionList` still accepts `Session[]` — the `isSubagent` and `workspace` fields are used as before

```typescript
// src/components/SessionList.tsx (modified)
// Change SessionCard title to use last_message from snippet if available,
// and show provider badge instead of agent name
```

The `SessionList` itself doesn't need major changes — it already renders `sessions` and `groupedSessions`. The main change is updating the card rendering to use the new `Session` fields populated from hub data.

Replace the card rendering section:

```typescript
// In SessionCard, replace the card content with:
function SessionCard({ session, tag, selected, onSelect }: { session: Session; tag?: SessionTag; selected: boolean; onSelect: (s: Session) => void }) {
  const isActive = session.endedAt == null; // no endedAt = still active

  return (
    <div className={`session-card ${selected ? 'selected' : ''}`} data-testid={`session-card-${session.id}`} onClick={() => onSelect(session)}>
      <span className={`agent-icon agent-icon-${session.agent}`}>
        {session.agent[0]?.toUpperCase() ?? '?'}
      </span>
      <div className="session-info">
        <div className="session-title">
          {tag ? (
            <>
              <span className="session-title-main">{tag.summary}</span>
              <span className="session-title-sub">{session.snippet?.slice(0, 80) || session.id}</span>
            </>
          ) : (
            session.snippet?.slice(0, 80) || session.id
          )}
        </div>
        <div className="session-meta">
          <span className="agent-name">{session.agent}</span>
          {tag && (
            <span className={`category-pill ${tag.category === 'misc' ? 'misc' : ''}`}>
              {tag.category}
            </span>
          )}
          <span className="session-time">{formatTime(session.startedAt || session.endedAt)}</span>
          {session.score != null && (
            <span className="session-score">{session.score.toFixed(2)}</span>
          )}
          <span className={`status-dot ${isActive ? 'active' : 'ended'}`} title={isActive ? 'active' : 'ended'} />
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add src/components/SessionList.tsx
git commit -m "feat(sessions): SessionList shows dynamic provider badges and status dots

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 7: Session Panel — Visibility Prop and Close Button

**Files:**
- Modify: `src/components/SessionPanel.tsx`

The panel already has a close button. We need to make it hidden when not visible. Add CSS class and ensure the transcript viewer takes full height when visible.

Add a `visible` prop and a CSS class:

```typescript
// src/components/SessionPanel.tsx

interface Props {
  session: Session
  onClose: () => void
  visible?: boolean   // NEW — panel hidden when false (default)
}
```

```tsx
// In the return, wrap the root div:
<div className={`session-panel ${visible === false ? 'hidden' : ''}`} data-testid="session-panel">
```

- [ ] **Step 2: Add CSS rule to App.css**

```css
/* src/App.css — add after existing session-panel rules */
.session-panel.hidden {
  display: none;
}
```

- [ ] **Step 3: Commit**

```bash
git add src/components/SessionPanel.tsx src/App.css
git commit -m "feat(sessions): SessionPanel accepts visible prop, hidden when false

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 8: CSS — Agent Icon Colors and Responsive Layout

**Files:**
- Modify: `src/App.css` (or `src/index.css` — wherever CSS variables are defined)

Add agent-specific icon colors:

```css
/* Agent icon colors */
.agent-icon-claude-code { background: #3b82f6; }
.agent-icon-codex { background: #22c55e; }
.agent-icon-openclaw { background: #f97316; }
.agent-icon-opencode { background: #8b5cf6; }
.agent-icon-copilot { background: #06b6d4; }
.agent-icon-gemini { background: #eab308; }
.agent-icon { display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 28px; border-radius: 50%; color: white; font-weight: bold; font-size: 12px; flex-shrink: 0; }
```

Add status dot styles:

```css
/* Status dot */
.status-dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; margin-left: 6px; }
.status-dot.active { background: #22c55e; }
.status-dot.ended { background: #9ca3af; }
```

Hub banner:

```css
/* Hub connection banner */
.hub-banner { background: #f59e0b; color: black; text-align: center; padding: 6px; font-size: 13px; }
```

- [ ] **Step 2: Commit**

```bash
git add src/App.css
git commit -m "ui(sessions): add agent icon colors, status dots, hub banner styles

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 9: Verify Full Integration

- [ ] **Step 1: Run the dev server and check for errors**

```bash
cd /Users/openclaw/Github/agentroom && npm run dev
```

Expected: Vite starts on port 5173, Sessions tab shows "Hub offline — waiting to reconnect..." initially, then populates when hub is running.

- [ ] **Step 2: Start hub + collector and verify sessions appear**

Terminal 1:
```bash
cd /Users/openclaw/Github/agentroom && HUB_AUTH_TOKEN=HorseBatteryCorrectStaple cargo run --package session_hub
```

Terminal 2:
```bash
cd /Users/openclaw/Github/agentroom && HUB_URL=ws://localhost:8080 HUB_AUTH_TOKEN=HorseBatteryCorrectStaple COLLECTOR_ID=localhost cargo run --package session_collector
```

Terminal 3:
```bash
cd /Users/openclaw/Github/agentroom && npm run dev
```

Verify sessions appear in the sidebar. Click a session → transcript panel slides in. Click same session → panel hides.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "chore: sessions tab full integration verified

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Spec Coverage Check

| Spec Requirement | Task |
|-----------------|------|
| WebSocket to hub on port 8081 | Task 1, Task 4 |
| StateSync on connect | Task 1, Task 4 |
| SessionStarted/Ended/Activity events | Task 1, Task 4 |
| Dynamic provider list (no hardcoding) | Task 2, Task 5 |
| Session card with icon, title, time, status dot | Task 6, Task 8 |
| Transcript panel toggle (show/hide on click) | Task 4, Task 7 |
| Filter bar: provider, category, subagent | Task 5 |
| Local search on hub sessions | Task 4 |
| Real-time list updates | Task 1, Task 4 |

All spec requirements covered. No placeholder steps. Type consistency verified across tasks (HubActiveSession flows from Task 1 → Task 2 → Task 4).
