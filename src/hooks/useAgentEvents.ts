/**
 * useAgentEvents — Replaces useExtensionMessages.ts
 * Listens to Tauri 'agent-state-changed' events and drives OfficeState.
 */
import { useState, useEffect, useCallback, useRef } from 'react'
import type { OfficeState } from '../office/engine/officeState.js'
import type { ToolActivity } from '../office/types.js'
import { extractToolName } from '../office/toolUtils.js'
import { listenAgentEvents, type AgentEventPayload } from '../bridge.js'
import { playDoneSound } from '../office/notificationSound.js'

export interface SubagentCharacter {
  id: number
  parentAgentId: number
  parentToolId: string
  label: string
}

export interface AgentEventState {
  agents: number[]
  agentTools: Record<number, ToolActivity[]>
  agentStatuses: Record<number, string>
  subagentCharacters: SubagentCharacter[]
  /** Remove all agents from office state and reset ID maps */
  clearAll: () => void
  /** Reverse lookup: numeric character ID → original agent_id string (file stem / UUID) */
  getAgentStringId: (numId: number) => string | null
}

// Map string agent_id (session UUID) to numeric ID for the game engine
let nextAgentNumericId = 1
const agentIdMap = new Map<string, number>()
// Map string agent_id to agent type (claude-code, codex, gemini)
const agentTypeMap = new Map<string, string>()

function getOrCreateNumericId(agentId: string): number {
  let numId = agentIdMap.get(agentId)
  if (numId === undefined) {
    numId = nextAgentNumericId++
    agentIdMap.set(agentId, numId)
  }
  return numId
}

/** Reset all module-level ID maps. Call when switching watched project. */
export function resetAgentIdMaps(): void {
  agentIdMap.clear()
  agentTypeMap.clear()
  nextAgentNumericId = 1
}

export function useAgentEvents(
  getOfficeState: () => OfficeState,
): AgentEventState {
  const [agents, setAgents] = useState<number[]>([])
  const [agentTools, setAgentTools] = useState<Record<number, ToolActivity[]>>({})
  const [agentStatuses, setAgentStatuses] = useState<Record<number, string>>({})
  const [subagentCharacters, setSubagentCharacters] = useState<SubagentCharacter[]>([])

  // Track active tool counts per agent for tool_done logic
  const activeToolCountRef = useRef<Map<number, Set<string>>>(new Map())

  // Initial scan flag: during the first ~2s, all agents are created idle.
  // Only real-time events after the initial scan activate agents.
  const initializingRef = useRef(true)
  useEffect(() => {
    const timer = setTimeout(() => { initializingRef.current = false }, 2000)
    return () => clearTimeout(timer)
  }, [])

  /** Reverse lookup: numeric ID → original agent_id string */
  const getAgentStringId = useCallback((numId: number): string | null => {
    for (const [strId, nId] of agentIdMap) {
      if (nId === numId) return strId
    }
    return null
  }, [])

  /** Remove all agents from office state and reset ID maps */
  const clearAll = useCallback(() => {
    const os = getOfficeState()
    // Remove all characters
    for (const id of Array.from(os.characters.keys())) {
      const ch = os.characters.get(id)
      if (ch?.seatId) {
        const seat = os.seats.get(ch.seatId)
        if (seat) seat.assigned = false
      }
      if (ch?.idleSeatId) {
        const idleSeat = os.seats.get(ch.idleSeatId)
        if (idleSeat) idleSeat.assigned = false
      }
      os.characters.delete(id)
    }
    os.subagentIdMap.clear()
    os.subagentMeta.clear()
    os.selectedAgentId = null
    os.cameraFollowId = null
    // Reset module-level ID maps
    resetAgentIdMaps()
    activeToolCountRef.current.clear()
    // Reset React state
    setAgents([])
    setAgentTools({})
    setAgentStatuses({})
    setSubagentCharacters([])
  }, [getOfficeState])

  const ensureAgent = useCallback((numId: number, agentType?: string) => {
    const os = getOfficeState()
    if (!os.characters.has(numId)) {
      os.addAgent(numId, undefined, undefined, undefined, undefined, undefined, agentType)
      setAgents((prev) => prev.includes(numId) ? prev : [...prev, numId])
    }
  }, [getOfficeState])

  useEffect(() => {
    const unlistenPromise = listenAgentEvents((event: AgentEventPayload) => {
      const os = getOfficeState()
      const numId = getOrCreateNumericId(event.agent_id)

      // Track agent type from first event that carries it
      if (event.agent_type && !agentTypeMap.has(event.agent_id)) {
        agentTypeMap.set(event.agent_id, event.agent_type)
      }
      const agentType = agentTypeMap.get(event.agent_id)

      // During initial scan (~first 2s), only create agents as idle.
      // Skip activation, sounds, and bubbles — treat all sessions as idle.
      if (initializingRef.current) {
        ensureAgent(numId, agentType)
        return
      }

      switch (event.status) {
        case 'tool_start': {
          ensureAgent(numId, agentType)
          const toolId = event.tool_id || ''
          const status = event.tool_status || `Using ${event.tool_name || 'tool'}`

          // Track active tools
          let toolSet = activeToolCountRef.current.get(numId)
          if (!toolSet) {
            toolSet = new Set()
            activeToolCountRef.current.set(numId, toolSet)
          }
          toolSet.add(toolId)

          setAgentTools((prev) => {
            const list = prev[numId] || []
            if (list.some((t) => t.toolId === toolId)) return prev
            return { ...prev, [numId]: [...list, { toolId, status, done: false }] }
          })

          const toolName = extractToolName(status)
          os.setAgentTool(numId, toolName)
          os.setAgentActive(numId, true)
          os.clearPermissionBubble(numId)

          // Sub-agent creation for Task tool
          if (event.is_subagent && event.parent_tool_id) {
            const label = status.startsWith('Subtask:') ? status.slice('Subtask:'.length).trim() : status
            const subId = os.addSubagent(numId, event.parent_tool_id)
            setSubagentCharacters((prev) => {
              if (prev.some((s) => s.id === subId)) return prev
              return [...prev, { id: subId, parentAgentId: numId, parentToolId: event.parent_tool_id!, label }]
            })
          }

          setAgentStatuses((prev) => {
            if (!(numId in prev)) return prev
            const next = { ...prev }
            delete next[numId]
            return next
          })
          break
        }

        case 'tool_done': {
          const toolId = event.tool_id || ''

          // Remove from active tools
          const toolSet = activeToolCountRef.current.get(numId)
          if (toolSet) {
            toolSet.delete(toolId)
            // If no more active tools, clear tool animation
            if (toolSet.size === 0) {
              os.setAgentTool(numId, null)
            }
          }

          setAgentTools((prev) => {
            const list = prev[numId]
            if (!list) return prev
            return {
              ...prev,
              [numId]: list.map((t) => (t.toolId === toolId ? { ...t, done: true } : t)),
            }
          })

          // Handle sub-agent tool done
          if (event.is_subagent && event.parent_tool_id) {
            const subId = os.getSubagentId(numId, event.parent_tool_id)
            if (subId !== null) {
              os.setAgentTool(subId, null)
            }
          }
          break
        }

        case 'turn_end': {
          ensureAgent(numId, agentType)

          // Clear all tools
          activeToolCountRef.current.delete(numId)
          setAgentTools((prev) => {
            if (!(numId in prev)) return prev
            const next = { ...prev }
            delete next[numId]
            return next
          })

          // Remove sub-agents
          os.removeAllSubagents(numId)
          setSubagentCharacters((prev) => prev.filter((s) => s.parentAgentId !== numId))

          os.setAgentTool(numId, null)
          os.setAgentActive(numId, false)
          os.clearPermissionBubble(numId)

          setAgentStatuses((prev) => ({ ...prev, [numId]: 'waiting' }))
          os.showWaitingBubble(numId)
          playDoneSound()
          break
        }

        case 'waiting': {
          ensureAgent(numId, agentType)
          os.setAgentActive(numId, false)
          os.showWaitingBubble(numId)
          setAgentStatuses((prev) => ({ ...prev, [numId]: 'waiting' }))
          playDoneSound()
          break
        }

        case 'active': {
          ensureAgent(numId, agentType)
          os.setAgentActive(numId, true)
          setAgentStatuses((prev) => {
            if (!(numId in prev)) return prev
            const next = { ...prev }
            delete next[numId]
            return next
          })
          break
        }

        case 'permission': {
          ensureAgent(numId, agentType)
          os.showPermissionBubble(numId)
          setAgentTools((prev) => {
            const list = prev[numId]
            if (!list) return prev
            return {
              ...prev,
              [numId]: list.map((t) => (t.done ? t : { ...t, permissionWait: true })),
            }
          })
          break
        }

        case 'permission_clear': {
          os.clearPermissionBubble(numId)
          // Also clear sub-agent permission bubbles
          for (const [subId, meta] of os.subagentMeta) {
            if (meta.parentAgentId === numId) {
              os.clearPermissionBubble(subId)
            }
          }
          setAgentTools((prev) => {
            const list = prev[numId]
            if (!list) return prev
            const hasPermission = list.some((t) => t.permissionWait)
            if (!hasPermission) return prev
            return {
              ...prev,
              [numId]: list.map((t) => (t.permissionWait ? { ...t, permissionWait: false } : t)),
            }
          })
          break
        }

        case 'text_idle': {
          ensureAgent(numId, agentType)
          os.setAgentActive(numId, false)
          os.showWaitingBubble(numId)
          setAgentStatuses((prev) => ({ ...prev, [numId]: 'waiting' }))
          playDoneSound()
          break
        }
      }
    })

    return () => {
      unlistenPromise.then((fn) => fn())
    }
  }, [getOfficeState, ensureAgent])

  return { agents, agentTools, agentStatuses, subagentCharacters, clearAll, getAgentStringId }
}
