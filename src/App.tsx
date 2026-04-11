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
import type { Session, SessionTag, HubActiveSession } from './types.js'
import { getHubClient } from './services/hub.js'
import { hubSessionToSession, normalizeProvider } from './adapters/hubSession.js'

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
  const handleStateSync = useCallback((incomingHubSessions: HubActiveSession[]) => {
    setHubSessions(incomingHubSessions)
    setSessions(incomingHubSessions.map(hubSessionToSession))
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
    if (typeof window !== 'undefined' && '__TAURI__' in window) {
      loadAllTags().then((loaded) => {
        setTags(loaded)
        setCategories(getAllCategories())
      }).catch(() => {})
    } else {
      setLoading(false)
    }
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
    if (!hubSessions || hubSessions.length === 0) return []
    const set = new Set<string>()
    for (const s of hubSessions) {
      set.add(normalizeProvider(s.provider))
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
    if (!sessions || sessions.length === 0) return []
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
  const handleZoomChange = useCallback((newZoom: number) => { setZoom(newZoom) }, [])

  const noopTile = useCallback((_col: number, _row: number) => {}, [])
  const noopSelection = useCallback(() => {}, [])
  const noopDelete = useCallback(() => {}, [])
  const noopRotate = useCallback(() => {}, [])
  const noopDrag = useCallback((_uid: string, _col: number, _row: number) => {}, [])

  const officeState = getOfficeState()

  if (!layoutReady) {
    return (
      <div style={{
        width: '100%', height: '100%', display: 'flex', alignItems: 'center',
        justifyContent: 'center', color: 'var(--vscode-foreground)',
        background: 'var(--pixel-bg)', fontSize: '24px',
      }}>
        Loading...
      </div>
    )
  }

  return (
    <div data-testid="app-root" className={`app ${mainView === 'sessions' ? 'app-sessions-view' : ''} ${sessionPanelOpen && mainView === 'sessions' ? 'app-with-panel' : ''}`}>
      {!hubConnected && (
        <div className="hub-banner" data-testid="hub-banner">
          Hub offline — waiting to reconnect...
        </div>
      )}

      <div className="sidebar" data-testid="sidebar">
        <div className="main-view-toggle" data-testid="view-switcher">
          <button data-testid="tab-office" className={mainView === 'office' ? 'active' : ''} onClick={() => setMainView('office')}>Agent Office</button>
          <button data-testid="tab-sessions" className={mainView === 'sessions' ? 'active' : ''} onClick={() => setMainView('sessions')}>Sessions</button>
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
            <ToolOverlay officeState={officeState} agents={agents} agentTools={agentTools} subagentCharacters={subagentCharacters} containerRef={containerRef} zoom={zoom} panRef={panRef} onCloseAgent={handleCloseAgent} />
            <TokenPanel />
          </div>
        </div>
      ) : (
        <div className="main-panel main-panel-sessions" data-testid="main-panel-sessions">
          {selectedSession && sessionPanelOpen ? (
            <SessionPanel session={selectedSession} onClose={handleClosePanel} />
          ) : (
            <div className="sessions-placeholder">Select a session from the sidebar to view its transcript</div>
          )}
        </div>
      )}

      {selectedSession && mainView === 'office' && (
        <SessionPanel session={selectedSession} onClose={() => setSelectedSession(null)} />
      )}

      <StatusBar sessions={displayedSessions} tags={tags} onTagUpdate={handleTagUpdate} />
    </div>
  )
}

export default App
