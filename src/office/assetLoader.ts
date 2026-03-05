/**
 * Browser-side asset loader — replaces the Node.js extension-side assetLoader.ts
 * Loads character sprites, wall tiles, tilesets, and default layout directly from
 * public/ directory using Canvas API for PNG → SpriteData conversion.
 */
import { setCharacterTemplates } from './sprites/spriteData.js'
import { setWallSprites } from './wallTiles.js'
import { tilesetManager } from './tilesets/tilesetManager.js'
import type { OfficeLayout } from './types.js'

// Constants matching Pixel Agents extension
const PNG_ALPHA_THRESHOLD = 128
const CHAR_FRAME_W = 16
const CHAR_FRAME_H = 32
const CHAR_FRAMES_PER_ROW = 7
const CHAR_COUNT = 6
const WALL_PIECE_WIDTH = 16
const WALL_PIECE_HEIGHT = 32
const WALL_GRID_COLS = 4
const WALL_BITMASK_COUNT = 16

type SpriteData = string[][]

interface CharacterDirectionSprites {
  down: SpriteData[]
  up: SpriteData[]
  right: SpriteData[]
}

/**
 * Load a PNG image and return its RGBA pixel data via Canvas API
 */
function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image()
    img.onload = () => resolve(img)
    img.onerror = () => reject(new Error(`Failed to load image: ${src}`))
    img.src = src
  })
}

function imageToPixelData(img: HTMLImageElement): ImageData {
  const canvas = document.createElement('canvas')
  canvas.width = img.width
  canvas.height = img.height
  const ctx = canvas.getContext('2d')!
  ctx.drawImage(img, 0, 0)
  return ctx.getImageData(0, 0, img.width, img.height)
}

function rgbaToHex(r: number, g: number, b: number): string {
  return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`.toUpperCase()
}

/**
 * Extract a sub-region from ImageData as SpriteData
 */
function extractSprite(
  data: ImageData,
  x: number,
  y: number,
  w: number,
  h: number,
): SpriteData {
  const sprite: SpriteData = []
  for (let row = 0; row < h; row++) {
    const line: string[] = []
    for (let col = 0; col < w; col++) {
      const idx = ((y + row) * data.width + (x + col)) * 4
      const r = data.data[idx]
      const g = data.data[idx + 1]
      const b = data.data[idx + 2]
      const a = data.data[idx + 3]
      line.push(a < PNG_ALPHA_THRESHOLD ? '' : rgbaToHex(r, g, b))
    }
    sprite.push(line)
  }
  return sprite
}

/**
 * 2× upscale a SpriteData: each pixel becomes a factor×factor block.
 * Used to scale 16×24 character sprites to 32×48 for the new 32px tile size.
 */
export function upscaleSprite(sprite: SpriteData, factor: number): SpriteData {
  const result: SpriteData = []
  for (const row of sprite) {
    const scaledRow: string[] = []
    for (const pixel of row) {
      for (let i = 0; i < factor; i++) scaledRow.push(pixel)
    }
    for (let i = 0; i < factor; i++) result.push([...scaledRow])
  }
  return result
}

/**
 * Load all 6 character sprite sheets from /assets/characters/char_0.png .. char_5.png
 * Each PNG is 112×96: 7 frames × 16px wide, 3 direction rows × 32px tall
 * Row 0 = down, Row 1 = up, Row 2 = right
 * After extraction, each frame is 2× upscaled to 32×64 for the 32px tile system.
 */
export async function loadCharacterSprites(): Promise<void> {
  const directions = ['down', 'up', 'right'] as const
  const characters: CharacterDirectionSprites[] = []

  for (let ci = 0; ci < CHAR_COUNT; ci++) {
    const img = await loadImage(`/assets/characters/char_${ci}.png`)
    const pixels = imageToPixelData(img)

    const charData: CharacterDirectionSprites = { down: [], up: [], right: [] }

    for (let dirIdx = 0; dirIdx < directions.length; dirIdx++) {
      const dir = directions[dirIdx]
      const rowOffsetY = dirIdx * CHAR_FRAME_H
      const frames: SpriteData[] = []

      for (let f = 0; f < CHAR_FRAMES_PER_ROW; f++) {
        const raw = extractSprite(pixels, f * CHAR_FRAME_W, rowOffsetY, CHAR_FRAME_W, CHAR_FRAME_H)
        frames.push(upscaleSprite(raw, 2))
      }
      charData[dir] = frames
    }
    characters.push(charData)
  }

  console.log(`[AssetLoader] Loaded ${characters.length} character sprites (2× upscaled to 32×64)`)
  setCharacterTemplates(characters)
}

/**
 * Load wall tiles from /assets/walls.png (64×128, 4×4 grid of 16×32 pieces)
 * Each piece is 2× upscaled to 32×64 for the 32px tile system.
 * Piece at bitmask M: col = M % 4, row = floor(M / 4)
 */
export async function loadWallTiles(): Promise<void> {
  try {
    const img = await loadImage('/assets/walls.png')
    const pixels = imageToPixelData(img)

    const sprites: SpriteData[] = []
    for (let mask = 0; mask < WALL_BITMASK_COUNT; mask++) {
      const ox = (mask % WALL_GRID_COLS) * WALL_PIECE_WIDTH
      const oy = Math.floor(mask / WALL_GRID_COLS) * WALL_PIECE_HEIGHT
      const raw = extractSprite(pixels, ox, oy, WALL_PIECE_WIDTH, WALL_PIECE_HEIGHT)
      sprites.push(upscaleSprite(raw, 2))
    }

    console.log(`[AssetLoader] Loaded ${sprites.length} wall tile pieces (2× upscaled to 32×64)`)
    setWallSprites(sprites)
  } catch (err) {
    console.warn('[AssetLoader] No wall tiles found, using defaults:', err)
  }
}

/**
 * Load SkyOffice tileset PNGs via TilesetManager.
 * FloorAndGround.png (firstGid=1) is the primary tileset for floor/wall background.
 */
export async function loadTilesets(): Promise<void> {
  try {
    await tilesetManager.loadTileset('/assets/tilesets/FloorAndGround.png', 1, 32, 32)
  } catch (err) {
    console.warn('[AssetLoader] Failed to load FloorAndGround tileset:', err)
  }
  try {
    await tilesetManager.loadTileset('/assets/tilesets/Modern_Office_Black_Shadow.png', 2561, 32, 32)
  } catch (err) {
    console.warn('[AssetLoader] Failed to load Modern_Office tileset:', err)
  }
  try {
    await tilesetManager.loadTileset('/assets/tilesets/Generic.png', 3409, 32, 32)
  } catch (err) {
    console.warn('[AssetLoader] Failed to load Generic tileset:', err)
  }
  try {
    await tilesetManager.loadTileset('/assets/tilesets/Basement.png', 4657, 32, 32)
  } catch (err) {
    console.warn('[AssetLoader] Failed to load Basement tileset:', err)
  }
}

/**
 * Load default office layout from /assets/default-layout.json
 */
export async function loadDefaultLayout(): Promise<OfficeLayout | null> {
  try {
    const resp = await fetch('/assets/default-layout.json')
    if (!resp.ok) return null
    const layout = await resp.json() as OfficeLayout
    console.log(`[AssetLoader] Loaded default layout (${layout.cols}×${layout.rows})`)
    return layout
  } catch (err) {
    console.warn('[AssetLoader] No default layout found:', err)
    return null
  }
}

/**
 * Load all assets in the correct order
 */
export async function loadAllAssets(): Promise<OfficeLayout | null> {
  await loadTilesets()
  await loadCharacterSprites()
  await loadWallTiles()
  const layout = await loadDefaultLayout()
  return layout
}
