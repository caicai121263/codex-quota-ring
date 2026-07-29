export function quotaLevel(percent) {
  if (typeof percent !== 'number' || !Number.isFinite(percent)) return 'unavailable';
  if (percent < 10) return 'critical';
  if (percent < 50) return 'attention';
  return 'healthy';
}

export function displayPercent(percent) {
  if (typeof percent !== 'number' || !Number.isFinite(percent)) return '—';
  return `${Math.round(Math.min(100, Math.max(0, percent)))}%`;
}

export function resetDistance(timestampMs, now = Date.now()) {
  if (typeof timestampMs !== 'number' || !Number.isFinite(timestampMs)) return '重置时间未知';
  const minutes = Math.max(0, Math.ceil((timestampMs - now) / 60000));
  const days = Math.floor(minutes / 1440);
  const hours = Math.floor((minutes % 1440) / 60);
  const mins = minutes % 60;
  if (days) return `${days}天${hours}小时后重置`;
  if (hours) return `${hours}:${String(mins).padStart(2, '0')} 后重置`;
  return `${mins}分钟后重置`;
}

export function resetDue(timestampMs, now = Date.now()) {
  return typeof timestampMs === 'number'
    && Number.isFinite(timestampMs)
    && timestampMs <= now;
}

export function displayCredit(value) {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) return null;
  return Number.isInteger(value)
    ? String(value)
    : value.toLocaleString('zh-CN', { maximumFractionDigits: 2 });
}

export function statusPresentation(state) {
  if (state === 'ready') return { label: '数据正常', className: 'ready' };
  if (state === 'stale') return { label: '数据可能已过期', className: 'stale' };
  if (state === 'loading') return { label: '正在读取', className: 'loading' };
  return { label: '额度不可用', className: 'unavailable' };
}

export function settingsExpansionDirection(currentY, currentHeight, targetHeight, monitorTop, monitorHeight) {
  const bottom = monitorTop + monitorHeight;
  return currentY + targetHeight <= bottom || targetHeight <= currentHeight ? 'down' : 'up';
}

export function rectanglesIntersect(left, right) {
  return left.x < right.x + right.width
    && left.x + left.width > right.x
    && left.y < right.y + right.height
    && left.y + left.height > right.y;
}

export function dockMiniLabel(primaryQuotaWindow) {
  return primaryQuotaWindow === 'weekly' ? '周' : '5h';
}

export function dockMiniDimensions(edge) {
  return edge === 'top'
    ? { width: 108, height: 36, orientation: 'horizontal' }
    : { width: 42, height: 112, orientation: 'vertical' };
}

export function selectQuotaWindows(status, primaryQuotaWindow) {
  const weeklyIsPrimary = primaryQuotaWindow === 'weekly';
  return {
    primaryLimit: weeklyIsPrimary ? status.weekly : status.fiveHour,
    secondaryLimit: weeklyIsPrimary ? status.fiveHour : status.weekly,
    primaryLabel: weeklyIsPrimary ? '周额度' : '5 小时额度',
    secondaryLabel: weeklyIsPrimary ? '5 小时额度' : '周额度'
  };
}
