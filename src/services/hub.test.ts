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
