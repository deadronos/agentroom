import type { Session } from '../types';
import type { HubActiveSession } from '../services/hub';

export function hubSessionToSession(hub: HubActiveSession): Session {
  const isSubagent = hub.agent_type === 'subagent';

  // Title is the last message preview, truncated to 80 chars
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
    startedAt: null,
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
