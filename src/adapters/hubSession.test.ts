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
    expect(session.agent).toBe('claude-code');  // normalized
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
