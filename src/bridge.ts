/**
 * Bridge — Replaces VS Code's vscodeApi.ts
 * Connects the game engine to Tauri backend via events and invoke.
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface AgentEventPayload {
  agent_id: string
  status: 'tool_start' | 'tool_done' | 'turn_end' | 'permission' | 'permission_clear' | 'active' | 'waiting' | 'text_idle'
  tool_name?: string
  tool_id?: string
  tool_status?: string
  is_subagent?: boolean
  parent_tool_id?: string
  agent_type?: string
}

export function listenAgentEvents(
  callback: (event: AgentEventPayload) => void,
): Promise<UnlistenFn> {
  return listen<AgentEventPayload>('agent-state-changed', (e) => callback(e.payload))
}

export async function startWatching(projectDir: string): Promise<string> {
  return invoke<string>('start_watching', { projectDir })
}

export async function stopWatching(): Promise<string> {
  return invoke<string>('stop_watching')
}

/** Stop current watcher and start a new one for a different project */
export async function switchWatching(projectDir: string): Promise<string> {
  await stopWatching()
  return startWatching(projectDir)
}

export async function getActiveAgents(): Promise<string> {
  return invoke<string>('get_active_agents')
}

// Layout persistence via local file (~/.agentroom/layouts/)
export async function saveLayout(projectId: string, layout: unknown): Promise<void> {
  await invoke('save_visual_layout', { projectId, layout: JSON.stringify(layout) })
}

export async function loadLayout(projectId: string): Promise<unknown | null> {
  try {
    const result = await invoke<string>('load_visual_layout', { projectId })
    return JSON.parse(result)
  } catch {
    return null
  }
}

export async function readCodexBarSnapshot(): Promise<unknown | null> {
  try {
    const raw = await invoke<string>('read_codexbar_snapshot')
    return JSON.parse(raw)
  } catch {
    return null
  }
}
