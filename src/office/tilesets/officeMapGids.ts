/**
 * Background GID map for the 30×22 office layout (2-room version).
 * References FloorAndGround.png tileset (firstGid=1).
 * GID 0 = no tileset tile (use fallback floor/wall rendering).
 *
 * Room zones:
 *   Work Room (left):   rows 1-20, cols 1-12  — GID 415 (office floor)
 *   Idle Room (right):  rows 1-20, cols 16-28 — GID 668 (warm floor)
 *   Corridor:           cols 13-15, rows 1-20 — GID 1607 (corridor)
 *   Walls:              perimeter              — GID 0 (fallback)
 */

// Floor GIDs from the FloorAndGround tileset
const WORK = 415     // light office floor
const IDLE = 668     // warm floor
const HALL = 1607    // corridor/hallway
const _0 = 0         // wall / no tileset tile (use fallback)

/** 30 columns × 22 rows background GID map */
export const OFFICE_MAP_GIDS: number[] = generateGidMap()

function generateGidMap(): number[] {
  const COLS = 30
  const ROWS = 22
  const gids: number[] = new Array(COLS * ROWS).fill(_0)

  const set = (r: number, c: number, gid: number) => {
    if (r >= 0 && r < ROWS && c >= 0 && c < COLS) {
      gids[r * COLS + c] = gid
    }
  }

  // Work Room: rows 1-20, cols 1-12
  for (let r = 1; r <= 20; r++) {
    for (let c = 1; c <= 12; c++) {
      set(r, c, WORK)
    }
  }

  // Idle Room: rows 1-20, cols 16-28
  for (let r = 1; r <= 20; r++) {
    for (let c = 16; c <= 28; c++) {
      set(r, c, IDLE)
    }
  }

  // Corridor: cols 13-15, rows 1-20
  for (let r = 1; r <= 20; r++) {
    for (let c = 13; c <= 15; c++) {
      set(r, c, HALL)
    }
  }

  return gids
}
