export interface Session {
  id: string
  agent: string
  isSubagent?: boolean
  workspace: string | null
  title: string | null
  sourcePath: string
  startedAt: number | null
  endedAt: number | null
  messageCount?: number
  score?: number
  matchType?: string
  snippet?: string
  last_tool?: string | null
}

export interface SessionSearchResult {
  query: string
  mode: string
  totalHits: number
  elapsedMs: number
  sessions: Session[]
}

export interface SessionMessage {
  role: 'user' | 'agent' | 'tool' | 'system'
  content: string
  createdAt: number | null
}

export interface SessionTranscript {
  session: Session
  messages: SessionMessage[]
}

export interface SessionTag {
  sessionId: string
  summary: string
  category: string
  taggedAt: number
  model?: string
}

export interface SessionTagStore {
  version: number
  tags: Record<string, SessionTag>
}

export interface ResumeConfig {
  provider: string
  command: string
  resumeArgs: string[]
  sessionIdFields: string[]
}

export const RESUME_CONFIGS: Record<string, ResumeConfig> = {
  'claude-code': {
    provider: 'claude-code',
    command: 'claude',
    resumeArgs: ['--resume', '{sessionId}', '--dangerously-skip-permissions'],
    sessionIdFields: ['session_id', 'sessionId'],
  },
  codex: {
    provider: 'codex',
    command: 'codex',
    resumeArgs: ['resume', '{sessionId}', '--dangerously-bypass-approvals-and-sandbox'],
    sessionIdFields: ['thread_id'],
  },
  gemini: {
    provider: 'gemini',
    command: 'gemini',
    resumeArgs: ['--resume', '{sessionId}', '--yolo'],
    sessionIdFields: ['sessionId'],
  },
}

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
