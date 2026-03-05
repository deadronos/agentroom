import { invoke } from "@tauri-apps/api/core";
import type { Session, SessionSearchResult, SessionMessage } from "../types";

function normalizeAgent(agent: string): string {
  return agent.replace(/_/g, "-");
}

function parseBoolean(value: unknown): boolean | null {
  if (typeof value === "boolean") return value;
  if (typeof value === "number") return value !== 0;
  if (typeof value === "string") {
    const normalized = value.trim().toLowerCase();
    if (normalized === "true" || normalized === "1" || normalized === "yes") return true;
    if (normalized === "false" || normalized === "0" || normalized === "no") return false;
  }
  return null;
}

function detectSubagent(record: Record<string, unknown>, sourcePath: string): boolean {
  const explicit = parseBoolean(
    record.is_sidechain ?? record.isSidechain ?? record.is_subagent ?? record.isSubagent
  );
  if (explicit !== null) return explicit;
  return /\/subagents\/agent-[^/]+\.jsonl$/i.test(sourcePath);
}

function parseCassHit(hit: Record<string, unknown>): Session {
  const sourcePath = (hit.source_path as string) || "";
  return {
    id: sourcePath,
    agent: normalizeAgent((hit.agent as string) || "unknown"),
    isSubagent: detectSubagent(hit, sourcePath),
    workspace: (hit.workspace as string) || null,
    title: (hit.title as string) || null,
    sourcePath,
    startedAt: (hit.created_at as number) || (hit.started_at as number) || null,
    endedAt: (hit.ended_at as number) || null,
    score: hit.score as number | undefined,
    matchType: hit.match_type as string | undefined,
    snippet: hit.snippet as string | undefined,
  };
}

function extractWorkspaceFromSourcePath(sourcePath: string): string | null {
  // Claude Code: ~/.claude/projects/{encoded-workspace}/{sessionId}.jsonl
  const claudeMatch = sourcePath.match(/\.claude\/projects\/([^/]+)\//);
  if (claudeMatch) {
    let decoded = claudeMatch[1];
    if (decoded.startsWith("-")) decoded = decoded.slice(1);
    return "/" + decoded.replace(/-/g, "/");
  }
  // Codex: ~/.codex/ patterns
  if (sourcePath.includes("/.codex/")) return null;
  // Gemini: hash-based paths
  if (sourcePath.includes("/.gemini/")) return null;
  return null;
}

function parseTimelineSession(s: Record<string, unknown>): Session {
  const sourcePath = (s.source_path as string) || "";
  return {
    id: sourcePath,
    agent: normalizeAgent((s.agent as string) || "unknown"),
    isSubagent: detectSubagent(s, sourcePath),
    workspace: (s.workspace as string) || extractWorkspaceFromSourcePath(sourcePath),
    title: (s.title as string) || null,
    sourcePath,
    startedAt: (s.started_at as number) || null,
    endedAt: (s.ended_at as number) || null,
  };
}

// --- Transcript normalization (ported from electron/main.ts) ---

type TranscriptRole = "user" | "agent" | "tool" | "unknown";

function normalizeRole(rawRole: unknown, fallbackType?: unknown): TranscriptRole {
  const role = String(rawRole ?? fallbackType ?? "").toLowerCase();
  if (!role) return "unknown";
  if (role === "assistant" || role === "agent" || role === "gemini" || role === "model") return "agent";
  if (role === "user" || role === "human") return "user";
  if (role.includes("tool") || role.includes("function")) return "tool";
  return "unknown";
}

function isToolPayload(content: unknown): boolean {
  if (Array.isArray(content)) return content.some(isToolPayload);
  if (!content || typeof content !== "object") return false;

  const obj = content as Record<string, unknown>;
  const type = String(obj.type ?? "").toLowerCase();

  if (type.includes("tool") || type.includes("function")) return true;
  if (typeof obj.tool_use_id === "string" || typeof obj.toolUseId === "string") return true;
  if (obj.input && typeof obj.input === "object" && typeof obj.name === "string") return true;

  return false;
}

function extractTextFromContent(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) return content.map(extractTextFromContent).filter(Boolean).join("\n");
  if (!content || typeof content !== "object") return "";

  const obj = content as Record<string, unknown>;
  const type = String(obj.type ?? "").toLowerCase();

  // Drop tool call/result payloads from user/assistant plain-text transcript.
  if (type.includes("tool") || type.includes("function")) return "";
  if (typeof obj.tool_use_id === "string" || typeof obj.toolUseId === "string") return "";

  if (typeof obj.text === "string") return obj.text;
  if (typeof obj.message === "string") return obj.message;
  if (typeof obj.content === "string") return obj.content;
  if (Array.isArray(obj.content)) return obj.content.map(extractTextFromContent).filter(Boolean).join("\n");
  return "";
}

function isControlMessage(text: string): boolean {
  const trimmed = text.trim();
  return (
    trimmed.startsWith("# AGENTS.md instructions") ||
    trimmed.startsWith("<environment_context>") ||
    trimmed.startsWith("<collaboration_mode>") ||
    trimmed.startsWith("<permissions instructions>") ||
    trimmed.startsWith("<turn_aborted>")
  );
}

function normalizeExportEntries(raw: unknown): SessionMessage[] {
  const entries = Array.isArray(raw) ? raw : [];
  const messages: SessionMessage[] = [];
  const seen = new Set<string>();
  let codexResponseMessages = 0;

  const push = (role: TranscriptRole, content: string, timestamp: unknown) => {
    const cleaned = content.trim();
    if (!cleaned || role === "unknown" || isControlMessage(cleaned)) return;
    const ts = typeof timestamp === "string" ? timestamp : undefined;
    const key = `${ts || ""}|${role}|${cleaned}`;
    if (seen.has(key)) return;
    seen.add(key);
    messages.push({
      role: role as SessionMessage["role"],
      content: cleaned,
      createdAt: ts ? new Date(ts).getTime() : null,
    });
  };

  for (const entry of entries) {
    if (!entry || typeof entry !== "object") continue;
    const record = entry as Record<string, unknown>;

    if (record.message && typeof record.message === "object") {
      const msg = record.message as Record<string, unknown>;
      const text = extractTextFromContent(msg.content);
      if (!text.trim() && isToolPayload(msg.content)) continue;
      push(normalizeRole(msg.role, record.type), text, record.timestamp);
      continue;
    }

    if (record.type === "response_item") {
      const payload = record.payload as Record<string, unknown> | undefined;
      if (payload?.type === "message") {
        const payloadRole = payload.role as string;
        if (payloadRole === "assistant" || payloadRole === "user") {
          codexResponseMessages += 1;
          const text = extractTextFromContent(payload.content);
          if (!text.trim() && isToolPayload(payload.content)) continue;
          push(normalizeRole(payloadRole, record.type), text, record.timestamp);
        }
      }
    }
  }

  if (messages.length === 0 || codexResponseMessages === 0) {
    for (const entry of entries) {
      if (!entry || typeof entry !== "object") continue;
      const record = entry as Record<string, unknown>;
      if (record.type !== "event_msg") continue;
      const payload = record.payload as Record<string, unknown> | undefined;
      if (payload?.type === "user_message") {
        push("user", String(payload.message || ""), record.timestamp);
      }
      if (payload?.type === "agent_message") {
        push("agent", String(payload.message || ""), record.timestamp);
      }
    }
  }

  return messages;
}

// --- Public API ---

export async function searchSessions(
  query: string,
  options: { mode?: string; agent?: string; limit?: number; days?: number } = {}
): Promise<SessionSearchResult> {
  const { mode = "lexical", agent, limit = 50, days } = options;
  try {
    const raw = await invoke<string>("cass_search", { query, mode, agent, limit, days });
    const data = JSON.parse(raw);
    return {
      query: data.query || query,
      mode: data.mode || mode,
      totalHits: data.total_matches || data.count || 0,
      elapsedMs: data.elapsed_ms || 0,
      sessions: (data.hits || []).map(parseCassHit),
    };
  } catch (err) {
    console.error("CASS search error:", err);
    return { query, mode, totalHits: 0, elapsedMs: 0, sessions: [] };
  }
}

export async function listAllSessions(days = 90): Promise<Session[]> {
  try {
    const raw = await invoke<string>("cass_sessions", { days });
    const data = JSON.parse(raw);
    return (data.sessions || []).map(parseTimelineSession);
  } catch (err) {
    console.error("CASS list error:", err);
    return [];
  }
}

export async function getSessionTranscript(sourcePath: string): Promise<SessionMessage[]> {
  try {
    const raw = await invoke<string>("cass_transcript", { path: sourcePath });
    const data = JSON.parse(raw);

    // Try normalizeExportEntries first (handles all formats)
    if (Array.isArray(data)) {
      const normalized = normalizeExportEntries(data);
      if (normalized.length > 0) return normalized;
    }

    // If data has messages array, normalize that
    if (data.messages) {
      const normalized = normalizeExportEntries(data.messages);
      if (normalized.length > 0) return normalized;

      // Fallback: simple message extraction
      return (data.messages as Record<string, unknown>[]).map((m) => ({
        role: (normalizeRole(m.role, m.type) === "unknown" ? "user" : normalizeRole(m.role, m.type)) as SessionMessage["role"],
        content: extractTextFromContent(m.content),
        createdAt: m.timestamp ? new Date(m.timestamp as string).getTime() : null,
      })).filter((m) => m.content.trim());
    }

    return [];
  } catch (err) {
    console.error("CASS transcript error:", err);
    return [];
  }
}

export async function indexSessions(): Promise<{ success: boolean }> {
  try {
    await invoke<string>("cass_index");
    return { success: true };
  } catch (err) {
    console.error("CASS index error:", err);
    return { success: false };
  }
}

export async function getHealth(): Promise<{ healthy: boolean; error?: string }> {
  try {
    const raw = await invoke<string>("cass_health");
    return JSON.parse(raw);
  } catch (err) {
    console.error("CASS health error:", err);
    return { healthy: false, error: String(err) };
  }
}
