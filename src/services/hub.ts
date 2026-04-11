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
  | { type: 'state_sync'; data: { sessions: HubActiveSession[] } }
  | { type: 'session_started'; data: { session_id: string; provider: string; project: string | null; model: string; timestamp: number; last_tool: string | null; last_message: string | null; agent_id: string | null; agent_type: string } }
  | { type: 'activity'; data: { session_id: string; provider: string; timestamp: number; tool: string | null; message_preview: string | null } }
  | { type: 'session_ended'; data: { session_id: string; provider: string; timestamp: number } }
  | { type: 'ack'; fingerprint: string }
  | { type: 'error'; message: string };

export type HubConnectionState = 'connecting' | 'connected' | 'disconnected' | 'reconnecting';

export interface HubClientOptions {
  url: string;
  onStateSync?: (sessions: HubActiveSession[]) => void;
  onSessionStarted?: (session: HubActiveSession) => void;
  onSessionEnded?: (sessionId: string) => void;
  onActivity?: (sessionId: string, tool: string | null, messagePreview: string | null) => void;
  onConnectionStateChange?: (state: HubConnectionState) => void;
}

export class HubClient {
  private ws: WebSocket | null = null;
  public opts: HubClientOptions;
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
        this.opts.onStateSync?.(msg.data.sessions);
        break;
      case 'session_started':
        this.opts.onSessionStarted?.({
          session_id: msg.data.session_id,
          provider: msg.data.provider,
          agent_id: msg.data.agent_id,
          agent_type: msg.data.agent_type,
          model: msg.data.model,
          status: 'active',
          last_activity: msg.data.timestamp,
          project: msg.data.project,
          last_message: msg.data.last_message,
          last_tool: msg.data.last_tool,
          last_tool_input: null,
          parent_session_id: null,
        });
        break;
      case 'session_ended':
        this.opts.onSessionEnded?.(msg.data.session_id);
        break;
      case 'activity':
        this.opts.onActivity?.(msg.data.session_id, msg.data.tool, msg.data.message_preview);
        break;
      case 'ack':
      case 'error':
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
