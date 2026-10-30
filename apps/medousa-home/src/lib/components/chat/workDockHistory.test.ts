/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from 'vitest';
import { readWorkDismissals, workHistoryKey, writeWorkDismissals } from './workDockHistory';

afterEach(() => { vi.restoreAllMocks(); localStorage.clear(); });
it('scopes history to workshop, authority and session without ambiguous concatenation', () => {
  const keys = [workHistoryKey('a', 'b', 'c'), workHistoryKey('x', 'b', 'c'), workHistoryKey('a', 'x', 'c'), workHistoryKey('a', 'b', 'x')];
  expect(new Set(keys).size).toBe(4);
  writeWorkDismissals(keys[0], ['worker:one']);
  expect(readWorkDismissals(keys[0])).toEqual(['worker:one']);
  expect(readWorkDismissals(keys[1])).toEqual([]);
});
it('ignores malformed storage and bounds persisted IDs', () => {
  localStorage.setItem('history', '{invalid'); expect(readWorkDismissals('history')).toEqual([]);
  localStorage.setItem('history', '{"not":"an array"}'); expect(readWorkDismissals('history')).toEqual([]);
  localStorage.setItem('history', '[null, 2, "worker:one"]'); expect(readWorkDismissals('history')).toEqual(['worker:one']);
  writeWorkDismissals('history', Array.from({ length: 600 }, (_, i) => `${i}`));
  expect(readWorkDismissals('history')).toHaveLength(500);
});
it('survives restricted or exhausted browser storage', () => {
  vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('restricted'); });
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('quota'); });
  expect(readWorkDismissals('history')).toEqual([]);
  expect(() => writeWorkDismissals('history', ['one'])).not.toThrow();
});
