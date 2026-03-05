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
import { listAllSessions, searchSessions } from './services/cass.js'
import { getAllCategories, loadAllTags } from './services/tags.js'
import type { Session, SessionTag } from './types.js'

type AgentFilter = 'all' | 'claude-code' | 'codex' | 'gemini'
type ViewMode = 'grouped' | 'flat'
type MainView = 'office' | 'sessions'

function isClaudeSubagent(session: Session): boolean {
  return session.agent === 'claude-code' && !!session.isSubagent
}

function projectBasename(workspace: string | null): string {
  if (!workspace) return 'Other'
  const parts = workspace.split('/').filter(Boolean)
  return parts[parts.length - 1] || 'Other'
}

// Game state lives outside React — updated imperatively by event handlers
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

  // Session search/filter state
  const [sessions, setSessions] = useState<Session[]>([])
  const [tags, setTags] = useState<Record<string, SessionTag>>({})
  const [categories, setCategories] = useState<string[]>([])
  const [selectedSession, setSelectedSession] = useState<Session | null>(null)
  const [loading, setLoading] = useState(true)
  const [searchQuery, setSearchQuery] = useState('')
  const [agentFilter, setAgentFilter] = useState<AgentFilter>('all')
  const [categoryFilter, setCategoryFilter] = useState('all')
  const [showClaudeSubagents, setShowClaudeSubagents] = useState(false)
  const [viewMode, setViewMode] = useState<ViewMode>('grouped')
  const [focusedProject, setFocusedProject] = useState<string | null>(null)
  const [mainView, setMainView] = useState<MainView>('office')

  const { agents, agentTools, subagentCharacters, clearAll, getAgentStringId } = useAgentEvents(getOfficeState)

  // Load sessions from CASS
  const loadSessions = useCallback(async (agent: AgentFilter) => {
    setLoading(true)
    try {
      const allSessions = await listAllSessions(90)
      const filtered = agent === 'all' ? allSessions : allSessions.filter((s) => s.agent === agent)
      setSessions(filtered)
      setSelectedSession((current) => (current && filtered.some((s) => s.id === current.id) ? current : null))
    } catch {
      console.warn('[App] Could not load sessions — CASS may not be available')
    }
    setLoading(false)
  }, [])

  // Load tags
  useEffect(() => {
    loadAllTags().then((loaded) => {
      setTags(loaded)
      setCategories(getAllCategories())
    }).catch(() => {
      // Tags service may not be available
    })
  }, [])

  // Load sessions on mount
  useEffect(() => {
    loadSessions('all')
  }, [loadSessions])

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

  const handleSearch = useCallback(async (
    query: string,
    agent: AgentFilter,
    category: string,
    showSubagents: boolean,
  ) => {
    setSearchQuery(query)
    setAgentFilter(agent)
    setCategoryFilter(category)
    setShowClaudeSubagents(showSubagents)
    if (!query.trim()) {
      loadSessions(agent)
      return
    }
    setLoading(true)
    try {
      const result = await searchSessions(query, { limit: 50, agent: agent === 'all' ? undefined : agent })
      setSessions(result.sessions)
    } catch {
      console.warn('[App] Search failed')
    }
    setLoading(false)
  }, [loadSessions])

  const displayedSessions = useMemo(() => {
    let filtered = showClaudeSubagents
      ? sessions
      : sessions.filter((s) => !isClaudeSubagent(s))

    if (categoryFilter !== 'all') {
      filtered = filtered.filter((s) => tags[s.id]?.category === categoryFilter)
    }

    return filtered
  }, [sessions, tags, categoryFilter, showClaudeSubagents])

  // Group sessions by workspace/project
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

  // Map project basename → full workspace path (for focus button)
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

  const handleFocusProject = useCallback(async (workspace: string | null) => {
    clearAll()
    setFocusedProject(workspace)
    try {
      await switchWatching(workspace || '')
    } catch {
      console.warn('[App] Could not switch watching')
    }
  }, [clearAll])

  // When a session is clicked, also focus the watcher on its workspace
  const handleSelectSession = useCallback((session: Session) => {
    setSelectedSession(session)
    if (session.workspace) {
      // Only switch watcher if it's a different project than currently focused
      if (focusedProject !== session.workspace) {
        handleFocusProject(session.workspace)
      }
    }
  }, [focusedProject, handleFocusProject])

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
    // Find the matching session for this agent character
    const agentStringId = getAgentStringId(agentId)
    if (!agentStringId) return
    // Match session by checking if sourcePath contains the agent_id (UUID file stem)
    const match = sessions.find((s) => s.sourcePath.includes(agentStringId))
    if (match) {
      setSelectedSession(match)
    }
  }, [getAgentStringId, sessions])

  const handleCloseAgent = useCallback((_id: number) => {
    // No-op in standalone
  }, [])

  const handleZoomChange = useCallback((newZoom: number) => {
    setZoom(newZoom)
  }, [])

  // Editor no-ops for MVP
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
    <div className={`app ${mainView === 'sessions' ? 'app-sessions-view' : ''} ${selectedSession && mainView === 'office' ? 'app-with-panel' : ''}`}>
      {/* Sidebar: Search + Session List */}
      <div className="sidebar">
        {/* View Switcher */}
        <div className="main-view-toggle">
          <button
            className={mainView === 'office' ? 'active' : ''}
            onClick={() => setMainView('office')}
          >
            Agent Office
          </button>
          <button
            className={mainView === 'sessions' ? 'active' : ''}
            onClick={() => setMainView('sessions')}
          >
            Sessions
          </button>
        </div>

        <SearchBar
          onSearch={handleSearch}
          categories={categories}
          initialAgent={agentFilter}
          initialCategory={categoryFilter}
          initialShowClaudeSubagents={showClaudeSubagents}
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

      {/* Main content area: Office or Sessions view */}
      {mainView === 'office' ? (
        <div className="main-panel">
          <div ref={containerRef} style={{ width: '100%', height: '100%', position: 'relative', overflow: 'hidden' }}>
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

            {/* Vignette overlay */}
            <div
              style={{
                position: 'absolute',
                inset: 0,
                background: 'var(--pixel-vignette)',
                pointerEvents: 'none',
                zIndex: 40,
              }}
            />

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
        /* Sessions full view — transcript fills the main area */
        <div className="main-panel main-panel-sessions">
          {selectedSession ? (
            <SessionPanel
              session={selectedSession}
              onClose={() => setSelectedSession(null)}
            />
          ) : (
            <div className="sessions-placeholder">
              Select a session from the sidebar to view its transcript
            </div>
          )}
        </div>
      )}

      {/* Right: Session Preview Panel (only in office view) */}
      {selectedSession && mainView === 'office' && (
        <SessionPanel
          session={selectedSession}
          onClose={() => setSelectedSession(null)}
        />
      )}

      {/* Bottom: Status Bar */}
      <StatusBar sessions={displayedSessions} tags={tags} onTagUpdate={handleTagUpdate} />
    </div>
  )
}

export default App
