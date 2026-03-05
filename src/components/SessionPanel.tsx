import { useState, useEffect, useRef } from 'react'
import { getSessionTranscript } from '../services/cass.js'
import { resumeSession, buildResumeCommand } from '../services/resume.js'
import type { Session, SessionMessage } from '../types.js'

interface Props {
  session: Session
  onClose: () => void
}

function formatTime(ts: number | null): string {
  if (!ts) return ''
  const d = new Date(ts)
  const hh = d.getHours().toString().padStart(2, '0')
  const mm = d.getMinutes().toString().padStart(2, '0')
  return `${hh}:${mm}`
}

export function SessionPanel({ session, onClose }: Props) {
  const [messages, setMessages] = useState<SessionMessage[]>([])
  const [loading, setLoading] = useState(true)
  const [resuming, setResuming] = useState(false)
  const [canResume, setCanResume] = useState(false)
  const transcriptRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setMessages([])

    getSessionTranscript(session.sourcePath).then((msgs) => {
      if (!cancelled) {
        setMessages(msgs)
        setLoading(false)
      }
    })

    return () => { cancelled = true }
  }, [session.sourcePath])

  useEffect(() => {
    let cancelled = false
    buildResumeCommand(session).then((command) => {
      if (!cancelled) setCanResume(!!command)
    })
    return () => { cancelled = true }
  }, [session.id, session.sourcePath, session.agent, session.workspace])

  // Auto-scroll to bottom when messages load
  useEffect(() => {
    if (transcriptRef.current && messages.length > 0) {
      transcriptRef.current.scrollTop = transcriptRef.current.scrollHeight
    }
  }, [messages])

  const handleResume = async () => {
    setResuming(true)
    await resumeSession(session)
    setResuming(false)
  }

  const agentLabel = session.agent === 'claude-code' ? 'Claude' :
    session.agent === 'codex' ? 'Codex' :
    session.agent === 'gemini' ? 'Gemini' : session.agent

  const filename = session.sourcePath.split('/').pop() || session.sourcePath

  return (
    <div className="session-panel">
      <div className="sp-header">
        <div className="sp-title-row">
          <span className="sp-agent-badge">{agentLabel}</span>
          <h3 className="sp-title">{session.title || filename}</h3>
          <button className="sp-close" onClick={onClose} title="Close panel">x</button>
        </div>
        {session.workspace && (
          <div className="sp-workspace">{session.workspace}</div>
        )}
        <div className="sp-actions">
          {canResume && (
            <button
              className="sp-resume-btn"
              onClick={handleResume}
              disabled={resuming}
            >
              {resuming ? 'Opening...' : 'Open in iTerm2'}
            </button>
          )}
          <span className="sp-msg-count">
            {loading ? '...' : `${messages.length} messages`}
          </span>
        </div>
      </div>

      <div className="sp-transcript" ref={transcriptRef}>
        {loading ? (
          <div className="sp-loading">Loading transcript...</div>
        ) : messages.length === 0 ? (
          <div className="sp-empty">No messages in this session</div>
        ) : (
          messages.map((msg, i) => (
            <div key={i} className={`sp-message sp-message-${msg.role}`}>
              <div className="sp-message-header">
                <span className="sp-message-role">{msg.role}</span>
                {msg.createdAt && (
                  <span className="sp-message-time">{formatTime(msg.createdAt)}</span>
                )}
              </div>
              <div className="sp-message-content">{msg.content}</div>
            </div>
          ))
        )}
      </div>
    </div>
  )
}
