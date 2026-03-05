/**
 * TilesetManager — loads SkyOffice-style PNG tilesets and draws individual tiles by GID.
 * Each tileset has a firstGid; tiles are identified globally by their GID.
 */

interface TilesetDef {
  firstGid: number
  image: HTMLImageElement
  tileWidth: number
  tileHeight: number
  columns: number
  tileCount: number
}

class TilesetManager {
  private tilesets: TilesetDef[] = []

  /** Load a tileset PNG and register it with the given firstGid. */
  async loadTileset(src: string, firstGid: number, tileW: number, tileH: number): Promise<void> {
    const image = await new Promise<HTMLImageElement>((resolve, reject) => {
      const img = new Image()
      img.onload = () => resolve(img)
      img.onerror = () => reject(new Error(`Failed to load tileset: ${src}`))
      img.src = src
    })
    const columns = Math.floor(image.width / tileW)
    const rows = Math.floor(image.height / tileH)
    const tileCount = columns * rows
    this.tilesets.push({ firstGid, image, tileWidth: tileW, tileHeight: tileH, columns, tileCount })
    // Keep sorted by firstGid descending for lookup
    this.tilesets.sort((a, b) => b.firstGid - a.firstGid)
    console.log(`[TilesetManager] Loaded ${src} (${columns}×${rows} = ${tileCount} tiles, firstGid=${firstGid})`)
  }

  /** Find the tileset and source rect for a given GID. Returns null for GID 0 (empty). */
  getTileSource(gid: number): { tileset: TilesetDef; sx: number; sy: number } | null {
    if (gid <= 0) return null
    for (const ts of this.tilesets) {
      if (gid >= ts.firstGid && gid < ts.firstGid + ts.tileCount) {
        const localId = gid - ts.firstGid
        const col = localId % ts.columns
        const row = Math.floor(localId / ts.columns)
        return {
          tileset: ts,
          sx: col * ts.tileWidth,
          sy: row * ts.tileHeight,
        }
      }
    }
    return null
  }

  /** Draw a single tile at pixel position (x, y) scaled by zoom. */
  drawTile(ctx: CanvasRenderingContext2D, gid: number, x: number, y: number, zoom: number): void {
    const src = this.getTileSource(gid)
    if (!src) return
    const { tileset, sx, sy } = src
    ctx.drawImage(
      tileset.image,
      sx, sy, tileset.tileWidth, tileset.tileHeight,
      x, y, tileset.tileWidth * zoom, tileset.tileHeight * zoom,
    )
  }

  /** Whether any tilesets have been loaded */
  get loaded(): boolean {
    return this.tilesets.length > 0
  }
}

/** Singleton tileset manager instance */
export const tilesetManager = new TilesetManager()
