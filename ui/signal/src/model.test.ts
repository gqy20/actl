import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseSnapshot, summaryLines } from './model.ts';
import { mocks } from './mocks.ts';

test('unknown or malformed state never becomes a success snapshot', () => {
  for (const invalid of [null, {}, { ...mocks.completed, version: 2 }, { ...mocks.completed, status: null }, { ...mocks.completed, tone: 'success' }]) {
    assert.equal(parseSnapshot(invalid), null);
  }
  assert.equal(parseSnapshot(mocks.unknown)?.status, '状态未确认');
});
test('window titles and diagnostics remain opaque text', () => {
  const title = '<img src=x onerror=alert(1)>';
  assert.equal(parseSnapshot({ ...mocks.paused, title })?.title, title);
  assert.deepEqual(summaryLines('1 / 2 步完成\r\n\n写入纪要'), ['1 / 2 步完成', '写入纪要']);
});
test('completed preview preserves verification count and exposes no continue', () => {
  assert.equal(mocks.completed.primary, '');
  assert.match(mocks.completed.body, /1 步有核验记录/);
  assert.equal(mocks.stopped.primaryEnabled, false);
});
