import { describe, expect, it } from 'vitest';
import { activeLine, fmtBytes, fmtTime, plural } from './format';

describe('format', () => {
  it('formats time', () => {
    expect(fmtTime(0)).toBe('0:00');
    expect(fmtTime(65_000)).toBe('1:05');
    expect(fmtTime(3_725_000)).toBe('1:02:05');
    expect(fmtTime(NaN)).toBe('0:00');
  });
  it('formats bytes', () => {
    expect(fmtBytes(512)).toBe('512 B');
    expect(fmtBytes(1024 * 1024 * 5)).toBe('5.0 MB');
  });
  it('pluralises', () => {
    expect(plural(1, 'song')).toBe('1 song');
    expect(plural(2, 'song')).toBe('2 songs');
  });
  it('finds active lyric line', () => {
    const t = [1000, 3000, 6000];
    expect(activeLine(t, 500)).toBe(-1);
    expect(activeLine(t, 1000)).toBe(0);
    expect(activeLine(t, 5999)).toBe(1);
    expect(activeLine(t, 99999)).toBe(2);
  });
});
