import { useState } from "react";
import type { Session, SessionTag } from "../types";

type ViewMode = "grouped" | "flat";

interface Props {
  sessions: Session[];
  groupedSessions: [string, Session[]][] | null;
  tags: Record<string, SessionTag>;
  selectedId: string | null;
  onSelect: (session: Session) => void;
  loading: boolean;
  isSearch: boolean;
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
  focusedProject: string | null;
  projectWorkspaces: Map<string, string>;
  onFocusProject: (workspace: string | null) => void;
}

const AGENT_ICONS: Record<string, string> = {
  "claude-code": "C",
  codex: "X",
  gemini: "G",
};

function formatTime(ts: number | null): string {
  if (!ts) return "";
  const d = new Date(ts);
  const now = new Date();
  const diffMs = now.getTime() - d.getTime();
  const diffH = diffMs / (1000 * 60 * 60);

  if (diffH < 1) return `${Math.floor(diffMs / 60000)}m ago`;
  if (diffH < 24) return `${Math.floor(diffH)}h ago`;
  if (diffH < 48) return "yesterday";
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function sessionTitle(s: Session): string {
  if (s.title) return s.title;
  if (s.snippet) return s.snippet.slice(0, 80);
  const parts = s.sourcePath.split("/");
  return parts[parts.length - 1] || s.id;
}

function SessionCard({
  session,
  tag,
  selected,
  onSelect,
}: {
  session: Session;
  tag?: SessionTag;
  selected: boolean;
  onSelect: (s: Session) => void;
}) {
  return (
    <div
      className={`session-card ${selected ? "selected" : ""}`}
      onClick={() => onSelect(session)}
    >
      <span className="agent-icon">
        {AGENT_ICONS[session.agent] || session.agent[0]?.toUpperCase() || "?"}
      </span>
      <div className="session-info">
        <div className="session-title">
          {tag ? (
            <>
              <span className="session-title-main">{tag.summary}</span>
              <span className="session-title-sub">{sessionTitle(session)}</span>
            </>
          ) : (
            sessionTitle(session)
          )}
        </div>
        <div className="session-meta">
          <span className="agent-name">{session.agent}</span>
          {tag && (
            <span className={`category-pill ${tag.category === "misc" ? "misc" : ""}`}>
              {tag.category}
            </span>
          )}
          <span className="session-time">{formatTime(session.startedAt)}</span>
          {session.score != null && (
            <span className="session-score">{session.score.toFixed(2)}</span>
          )}
        </div>
      </div>
    </div>
  );
}

export function SessionList({
  sessions,
  groupedSessions,
  tags,
  selectedId,
  onSelect,
  loading,
  isSearch,
  viewMode,
  onViewModeChange,
  focusedProject,
  projectWorkspaces,
  onFocusProject,
}: Props) {
  // Track which groups the user has explicitly expanded (default: all collapsed)
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const toggleGroup = (name: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(name)) {
        next.delete(name);
      } else {
        next.add(name);
      }
      return next;
    });
  };

  if (loading) {
    return <div className="session-list loading">Loading sessions...</div>;
  }

  if (sessions.length === 0) {
    return (
      <div className="session-list empty">
        {isSearch ? "No results found" : "No sessions available"}
      </div>
    );
  }

  /** Get the workspace path for a project name, for the focus feature */
  const getWorkspace = (projectName: string): string | null => {
    return projectWorkspaces.get(projectName) ?? null;
  };

  const isFocused = (projectName: string): boolean => {
    const ws = getWorkspace(projectName);
    return focusedProject !== null && ws !== null && focusedProject === ws;
  };

  const handleFocusClick = (e: React.MouseEvent, projectName: string) => {
    e.stopPropagation();
    const ws = getWorkspace(projectName);
    if (isFocused(projectName)) {
      onFocusProject(null);
    } else if (ws) {
      onFocusProject(ws);
    }
  };

  // Derive the focused project basename for the indicator
  const focusedBasename = focusedProject
    ? (() => {
        const parts = focusedProject.split("/").filter(Boolean);
        return parts[parts.length - 1] || focusedProject;
      })()
    : null;

  return (
    <div className="session-list">
      {/* Focus indicator bar */}
      {focusedBasename && (
        <div className="focus-indicator">
          <span>Watching: {focusedBasename}</span>
          <button onClick={() => onFocusProject(null)} title="Unfocus">&times;</button>
        </div>
      )}

      <div className="view-toggle">
        <button
          className={viewMode === "grouped" ? "active" : ""}
          onClick={() => onViewModeChange("grouped")}
        >
          Grouped
        </button>
        <button
          className={viewMode === "flat" ? "active" : ""}
          onClick={() => onViewModeChange("flat")}
        >
          Flat
        </button>
      </div>

      {groupedSessions && viewMode === "grouped" ? (
        groupedSessions.map(([projectName, groupSessions]) => (
          <div key={projectName} className="session-group">
            <div className="group-header" onClick={() => {
              toggleGroup(projectName);
              // Also focus watcher on this project
              const ws = getWorkspace(projectName);
              if (ws && focusedProject !== ws) {
                onFocusProject(ws);
              }
            }}>
              <span className={`group-chevron${expanded.has(projectName) ? "" : " collapsed"}`}>
                {"\u25BC"}
              </span>
              <span className="group-name">{projectName}</span>
              <span className="group-count">{groupSessions.length}</span>
              {getWorkspace(projectName) && (
                <button
                  className={`focus-btn${isFocused(projectName) ? " active" : ""}`}
                  onClick={(e) => handleFocusClick(e, projectName)}
                  title={isFocused(projectName) ? "Unfocus" : "Focus on this project"}
                >
                  {isFocused(projectName) ? "\u2715" : "\u25CE"}
                </button>
              )}
            </div>
            {expanded.has(projectName) &&
              groupSessions.map((s) => (
                <SessionCard
                  key={s.id}
                  session={s}
                  tag={tags[s.id]}
                  selected={s.id === selectedId}
                  onSelect={onSelect}
                />
              ))}
          </div>
        ))
      ) : (
        sessions.map((s) => (
          <SessionCard
            key={s.id}
            session={s}
            tag={tags[s.id]}
            selected={s.id === selectedId}
            onSelect={onSelect}
          />
        ))
      )}
    </div>
  );
}
