import test from 'node:test';
import assert from 'node:assert/strict';
import {
  displayCredit,
  displayPercent,
  dockMiniDimensions,
  dockMiniLabel,
  quotaLevel,
  rectanglesIntersect,
  resetDistance,
  resetDue,
  settingsExpansionDirection,
  statusPresentation,
  selectQuotaWindows
} from '../src/quotaDisplay.js';

test('quota level follows the agreed boundaries', () => {
  assert.equal(quotaLevel(50), 'healthy');
  assert.equal(quotaLevel(49.9), 'attention');
  assert.equal(quotaLevel(10), 'attention');
  assert.equal(quotaLevel(9.9), 'critical');
  assert.equal(quotaLevel(null), 'unavailable');
});

test('formats values without exposing invalid percentages', () => {
  assert.equal(displayPercent(54.4), '54%');
  assert.equal(displayPercent(999), '100%');
  assert.equal(displayPercent(-1), '0%');
  assert.equal(displayPercent(null), '—');
});

test('formats short and long reset countdowns', () => {
  const now = Date.UTC(2026, 0, 1, 0, 0, 0);
  assert.equal(resetDistance(now + 89 * 60000, now), '1:29 后重置');
  assert.equal(resetDistance(now + 2 * 86400000 + 3 * 3600000, now), '2天3小时后重置');
  assert.equal(resetDistance(null, now), '重置时间未知');
});

test('switches the primary and compact quota windows', () => {
  const status = {
    fiveHour: { remainingPercent: 70, resetsAt: 1 },
    weekly: { remainingPercent: 40, resetsAt: 2 }
  };
  assert.deepEqual(selectQuotaWindows(status, 'fiveHour'), {
    primaryLimit: status.fiveHour,
    secondaryLimit: status.weekly,
    primaryLabel: '5 小时额度',
    secondaryLabel: '周额度'
  });
  assert.deepEqual(selectQuotaWindows(status, 'weekly'), {
    primaryLimit: status.weekly,
    secondaryLimit: status.fiveHour,
    primaryLabel: '周额度',
    secondaryLabel: '5 小时额度'
  });
});

test('presents ready, stale, loading and unavailable states', () => {
  assert.deepEqual(statusPresentation('ready'), { label: '数据正常', className: 'ready' });
  assert.deepEqual(statusPresentation('stale'), { label: '数据可能已过期', className: 'stale' });
  assert.equal(statusPresentation('loading').className, 'loading');
  assert.equal(statusPresentation('unavailable').className, 'unavailable');
});

test('does not turn missing or invalid credits into zero', () => {
  assert.equal(displayCredit(null), null);
  assert.equal(displayCredit(Number.NaN), null);
  assert.equal(displayCredit(-1), null);
  assert.equal(displayCredit(0), '0');
  assert.equal(displayCredit(12.5), '12.5');
});

test('detects reset boundaries without issuing a read itself', () => {
  const now = Date.UTC(2026, 0, 1);
  assert.equal(resetDue(now, now), true);
  assert.equal(resetDue(now + 1, now), false);
  assert.equal(resetDue(null, now), false);
});

test('chooses upward settings expansion near the screen bottom', () => {
  assert.equal(settingsExpansionDirection(100, 160, 520, 0, 1080), 'down');
  assert.equal(settingsExpansionDirection(700, 160, 520, 0, 1080), 'up');
});

test('detects offscreen and intersecting window rectangles', () => {
  const monitor = { x: 0, y: 0, width: 1920, height: 1080 };
  assert.equal(rectanglesIntersect(
    { x: 1900, y: 100, width: 100, height: 100 },
    monitor
  ), true);
  assert.equal(rectanglesIntersect(
    { x: 2000, y: 100, width: 100, height: 100 },
    monitor
  ), false);
});

test('uses the agreed compact label and edge layout', () => {
  assert.equal(dockMiniLabel('fiveHour'), '5h');
  assert.equal(dockMiniLabel('weekly'), '周');
  assert.deepEqual(dockMiniDimensions('top'), {
    width: 108,
    height: 36,
    orientation: 'horizontal'
  });
  assert.deepEqual(dockMiniDimensions('left'), {
    width: 42,
    height: 112,
    orientation: 'vertical'
  });
  assert.deepEqual(dockMiniDimensions('right'), {
    width: 42,
    height: 112,
    orientation: 'vertical'
  });
});
