import { CharacterState, Direction, TILE_SIZE } from '../types.js'
import type { Character, Seat, SpriteData, TileType as TileTypeVal } from '../types.js'
import type { CharacterSprites } from '../sprites/spriteData.js'
import { findPath } from '../layout/tileMap.js'
import {
  WALK_SPEED_PX_PER_SEC,
  WALK_FRAME_DURATION_SEC,
  TYPE_FRAME_DURATION_SEC,
} from '../constants.js'

/** Tools that show reading animation instead of typing */
const READING_TOOLS = new Set(['Read', 'Grep', 'Glob', 'WebFetch', 'WebSearch'])

export function isReadingTool(tool: string | null): boolean {
  if (!tool) return false
  return READING_TOOLS.has(tool)
}

/** Pixel center of a tile */
function tileCenter(col: number, row: number): { x: number; y: number } {
  return {
    x: col * TILE_SIZE + TILE_SIZE / 2,
    y: row * TILE_SIZE + TILE_SIZE / 2,
  }
}

/** Direction from one tile to an adjacent tile */
function directionBetween(fromCol: number, fromRow: number, toCol: number, toRow: number): Direction {
  const dc = toCol - fromCol
  const dr = toRow - fromRow
  if (dc > 0) return Direction.RIGHT
  if (dc < 0) return Direction.LEFT
  if (dr > 0) return Direction.DOWN
  return Direction.UP
}

export function createCharacter(
  id: number,
  palette: number,
  seatId: string | null,
  seat: Seat | null,
  hueShift = 0,
): Character {
  const col = seat ? seat.seatCol : 1
  const row = seat ? seat.seatRow : 1
  const center = tileCenter(col, row)
  return {
    id,
    state: CharacterState.TYPE,
    dir: seat ? seat.facingDir : Direction.DOWN,
    x: center.x,
    y: center.y,
    tileCol: col,
    tileRow: row,
    path: [],
    moveProgress: 0,
    currentTool: null,
    palette,
    hueShift,
    frame: 0,
    frameTimer: 0,
    wanderTimer: 0,
    wanderCount: 0,
    wanderLimit: 0,
    isActive: false,
    seatId,
    idleSeatId: null,
    bubbleType: null,
    bubbleTimer: 0,
    seatTimer: 0,
    isSubagent: false,
    parentAgentId: null,
    matrixEffect: null,
    matrixEffectTimer: 0,
    matrixEffectSeeds: [],
  }
}

export function updateCharacter(
  ch: Character,
  dt: number,
  _walkableTiles: Array<{ col: number; row: number }>,
  seats: Map<string, Seat>,
  tileMap: TileTypeVal[][],
  blockedTiles: Set<string>,
): void {
  ch.frameTimer += dt

  switch (ch.state) {
    case CharacterState.TYPE: {
      if (ch.frameTimer >= TYPE_FRAME_DURATION_SEC) {
        ch.frameTimer -= TYPE_FRAME_DURATION_SEC
        ch.frame = (ch.frame + 1) % 2
      }

      if (ch.isActive) {
        // Active but not at work seat? Walk to work seat.
        if (ch.seatId) {
          const workSeat = seats.get(ch.seatId)
          if (workSeat && (ch.tileCol !== workSeat.seatCol || ch.tileRow !== workSeat.seatRow)) {
            const path = findPath(ch.tileCol, ch.tileRow, workSeat.seatCol, workSeat.seatRow, tileMap, blockedTiles)
            if (path.length > 0) {
              ch.path = path
              ch.moveProgress = 0
              ch.state = CharacterState.WALK
              ch.frame = 0
              ch.frameTimer = 0
              break
            }
          }
        }
        break // at work seat or no path — stay typing
      }

      // If no longer active, stand up and go to idle seat (after seatTimer expires)
      if (ch.seatTimer > 0) {
        ch.seatTimer -= dt
        break
      }
      ch.seatTimer = 0 // clear sentinel
      // Pathfind to idle seat if available
      if (ch.idleSeatId) {
        const idleSeat = seats.get(ch.idleSeatId)
        if (idleSeat) {
          // Already at idle seat? Just sit
          if (ch.tileCol === idleSeat.seatCol && ch.tileRow === idleSeat.seatRow) {
            ch.dir = idleSeat.facingDir
            break // stay in TYPE state at idle seat
          }
          const path = findPath(ch.tileCol, ch.tileRow, idleSeat.seatCol, idleSeat.seatRow, tileMap, blockedTiles)
          if (path.length > 0) {
            ch.path = path
            ch.moveProgress = 0
            ch.state = CharacterState.WALK
            ch.frame = 0
            ch.frameTimer = 0
            break
          }
        }
      }
      // No idle seat or no path — just stand idle in place
      ch.state = CharacterState.IDLE
      ch.frame = 0
      ch.frameTimer = 0
      break
    }

    case CharacterState.IDLE: {
      // No idle animation — static pose
      ch.frame = 0
      if (ch.seatTimer < 0) ch.seatTimer = 0 // clear turn-end sentinel
      // If became active, pathfind to work seat
      if (ch.isActive) {
        if (!ch.seatId) {
          // No seat assigned — type in place
          ch.state = CharacterState.TYPE
          ch.frame = 0
          ch.frameTimer = 0
          break
        }
        const seat = seats.get(ch.seatId)
        if (seat) {
          const path = findPath(ch.tileCol, ch.tileRow, seat.seatCol, seat.seatRow, tileMap, blockedTiles)
          if (path.length > 0) {
            ch.path = path
            ch.moveProgress = 0
            ch.state = CharacterState.WALK
            ch.frame = 0
            ch.frameTimer = 0
          } else {
            // Already at seat or no path — sit down
            ch.state = CharacterState.TYPE
            ch.dir = seat.facingDir
            ch.frame = 0
            ch.frameTimer = 0
          }
        }
        break
      }
      // Inactive and idle — periodically retry pathfinding to idle seat
      if (ch.idleSeatId) {
        ch.wanderTimer -= dt
        if (ch.wanderTimer <= 0) {
          ch.wanderTimer = 2.0 // retry every 2 seconds
          const idleSeat = seats.get(ch.idleSeatId)
          if (idleSeat) {
            if (ch.tileCol === idleSeat.seatCol && ch.tileRow === idleSeat.seatRow) {
              // Already at idle seat — sit down
              ch.state = CharacterState.TYPE
              ch.dir = idleSeat.facingDir
              ch.frame = 0
              ch.frameTimer = 0
              break
            }
            const path = findPath(ch.tileCol, ch.tileRow, idleSeat.seatCol, idleSeat.seatRow, tileMap, blockedTiles)
            if (path.length > 0) {
              ch.path = path
              ch.moveProgress = 0
              ch.state = CharacterState.WALK
              ch.frame = 0
              ch.frameTimer = 0
            }
          }
        }
      }
      break
    }

    case CharacterState.WALK: {
      // Walk animation
      if (ch.frameTimer >= WALK_FRAME_DURATION_SEC) {
        ch.frameTimer -= WALK_FRAME_DURATION_SEC
        ch.frame = (ch.frame + 1) % 4
      }

      if (ch.path.length === 0) {
        // Path complete — snap to tile center and transition
        const center = tileCenter(ch.tileCol, ch.tileRow)
        ch.x = center.x
        ch.y = center.y

        if (ch.isActive) {
          if (!ch.seatId) {
            // No seat — type in place
            ch.state = CharacterState.TYPE
          } else {
            const seat = seats.get(ch.seatId)
            if (seat && ch.tileCol === seat.seatCol && ch.tileRow === seat.seatRow) {
              ch.state = CharacterState.TYPE
              ch.dir = seat.facingDir
            } else {
              ch.state = CharacterState.IDLE
            }
          }
        } else {
          // Inactive — check if arrived at idle seat
          if (ch.idleSeatId) {
            const idleSeat = seats.get(ch.idleSeatId)
            if (idleSeat && ch.tileCol === idleSeat.seatCol && ch.tileRow === idleSeat.seatRow) {
              // Arrived at idle seat — sit down facing idle seat direction
              ch.state = CharacterState.TYPE
              ch.dir = idleSeat.facingDir
              ch.frame = 0
              ch.frameTimer = 0
              break
            }
          }
          // Check if arrived at work seat (legacy path or reassignment)
          if (ch.seatId) {
            const seat = seats.get(ch.seatId)
            if (seat && ch.tileCol === seat.seatCol && ch.tileRow === seat.seatRow) {
              ch.state = CharacterState.TYPE
              ch.dir = seat.facingDir
              if (ch.seatTimer < 0) {
                ch.seatTimer = 0
              }
              ch.frame = 0
              ch.frameTimer = 0
              break
            }
          }
          // Not at any seat — pathfind to idle seat or just stand idle
          if (ch.idleSeatId) {
            const idleSeat = seats.get(ch.idleSeatId)
            if (idleSeat) {
              const path = findPath(ch.tileCol, ch.tileRow, idleSeat.seatCol, idleSeat.seatRow, tileMap, blockedTiles)
              if (path.length > 0) {
                ch.path = path
                ch.moveProgress = 0
                ch.frame = 0
                ch.frameTimer = 0
                break
              }
            }
          }
          ch.state = CharacterState.IDLE
        }
        ch.frame = 0
        ch.frameTimer = 0
        break
      }

      // Move toward next tile in path
      const nextTile = ch.path[0]
      ch.dir = directionBetween(ch.tileCol, ch.tileRow, nextTile.col, nextTile.row)

      ch.moveProgress += (WALK_SPEED_PX_PER_SEC / TILE_SIZE) * dt

      const fromCenter = tileCenter(ch.tileCol, ch.tileRow)
      const toCenter = tileCenter(nextTile.col, nextTile.row)
      const t = Math.min(ch.moveProgress, 1)
      ch.x = fromCenter.x + (toCenter.x - fromCenter.x) * t
      ch.y = fromCenter.y + (toCenter.y - fromCenter.y) * t

      if (ch.moveProgress >= 1) {
        // Arrived at next tile
        ch.tileCol = nextTile.col
        ch.tileRow = nextTile.row
        ch.x = toCenter.x
        ch.y = toCenter.y
        ch.path.shift()
        ch.moveProgress = 0
      }

      // If became active while walking to idle seat, repath to work seat
      if (ch.isActive && ch.seatId) {
        const seat = seats.get(ch.seatId)
        if (seat) {
          const lastStep = ch.path[ch.path.length - 1]
          if (!lastStep || lastStep.col !== seat.seatCol || lastStep.row !== seat.seatRow) {
            const newPath = findPath(ch.tileCol, ch.tileRow, seat.seatCol, seat.seatRow, tileMap, blockedTiles)
            if (newPath.length > 0) {
              ch.path = newPath
              ch.moveProgress = 0
            }
          }
        }
      }
      break
    }
  }
}

/** Get the correct sprite frame for a character's current state and direction */
export function getCharacterSprite(ch: Character, sprites: CharacterSprites): SpriteData {
  switch (ch.state) {
    case CharacterState.TYPE:
      if (isReadingTool(ch.currentTool)) {
        return sprites.reading[ch.dir][ch.frame % 2]
      }
      return sprites.typing[ch.dir][ch.frame % 2]
    case CharacterState.WALK:
      return sprites.walk[ch.dir][ch.frame % 4]
    case CharacterState.IDLE:
      return sprites.walk[ch.dir][1]
    default:
      return sprites.walk[ch.dir][1]
  }
}

