import { describe, it, expect } from 'vitest';
import { layoutToTileMap } from './layoutSerializer';
import { OfficeLayout, TileType } from '../types';

describe('layoutToTileMap', () => {
  it('converts a flat layout array into a 2D tile map', () => {
    const layout: OfficeLayout = {
      version: 1,
      cols: 3,
      rows: 2,
      tiles: [
        TileType.WALL, TileType.FLOOR_1, TileType.WALL,
        TileType.FLOOR_2, TileType.WALL, TileType.FLOOR_3
      ],
      furniture: [],
    };

    const expectedMap = [
      [TileType.WALL, TileType.FLOOR_1, TileType.WALL],
      [TileType.FLOOR_2, TileType.WALL, TileType.FLOOR_3]
    ];

    const result = layoutToTileMap(layout);
    expect(result).toEqual(expectedMap);
  });

  it('handles an empty layout correctly', () => {
    const layout: OfficeLayout = {
      version: 1,
      cols: 0,
      rows: 0,
      tiles: [],
      furniture: [],
    };

    const expectedMap: any[][] = [];

    const result = layoutToTileMap(layout);
    expect(result).toEqual(expectedMap);
  });

  it('handles a 1x1 layout correctly', () => {
    const layout: OfficeLayout = {
      version: 1,
      cols: 1,
      rows: 1,
      tiles: [TileType.FLOOR_4],
      furniture: [],
    };

    const expectedMap = [
      [TileType.FLOOR_4]
    ];

    const result = layoutToTileMap(layout);
    expect(result).toEqual(expectedMap);
  });
});
