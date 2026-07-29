import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  displayCredit,
  displayPercent,
  dockMiniLabel,
  quotaLevel,
  resetDistance,
  resetDue,
  selectQuotaWindows,
  statusPresentation
} from './quotaDisplay';
import type {
  DiagnosticInfo,
  DockState,
  LimitWindow,
  Preferences,
  QuotaStatus
} from './types';

const unavailable: QuotaStatus = {
  state: 'loading',
  message: '正在读取 Codex 额度…',
  fiveHour: null,
  weekly: null,
  credits: null,
  updatedAt: null,
  lastSuccessAt: null,
  errorCode: null
};

const initialDockState: DockState = {
  docked: false,
  expanded: true,
  edge: null
};

function Gauge({
  label,
  limit,
  stale
}: {
  label: string;
  limit: LimitWindow | null;
  stale: boolean;
}) {
  const percent = limit?.remainingPercent ?? null;
  const level = quotaLevel(percent);
  const progress = typeof percent === 'number'
    ? Math.max(0.012, Math.min(percent, 100) / 100)
    : 0;
  const radius = 48;
  const length = 2 * Math.PI * radius;

  return (
    <div
      className={`gauge ${level}${stale ? ' stale-gauge' : ''}`}
      aria-label={`${label} ${displayPercent(percent)}`}
    >
      <svg viewBox="0 0 120 120" role="img" aria-hidden="true">
        <circle className="gauge-track" cx="60" cy="60" r={radius} />
        <circle
          className="gauge-value"
          cx="60"
          cy="60"
          r={radius}
          strokeDasharray={`${length * progress} ${length}`}
        />
      </svg>
      <span>{displayPercent(percent)}</span>
    </div>
  );
}

function MiniGauge({ limit, stale }: { limit: LimitWindow | null; stale: boolean }) {
  const percent = limit?.remainingPercent ?? null;
  const radius = 10;
  const length = 2 * Math.PI * radius;
  const progress = typeof percent === 'number'
    ? Math.max(0.02, Math.min(percent, 100) / 100)
    : 0;
  return (
    <svg
      className={`mini-gauge ${quotaLevel(percent)}${stale ? ' stale-gauge' : ''}`}
      viewBox="0 0 28 28"
      aria-hidden="true"
    >
      <circle className="mini-track" cx="14" cy="14" r={radius} />
      <circle
        className="mini-value"
        cx="14"
        cy="14"
        r={radius}
        strokeDasharray={`${length * progress} ${length}`}
      />
    </svg>
  );
}

function BrandMark() {
  return (
    <div className="brand-mark" title="Codex Quota Ring" aria-label="Codex Quota Ring">
      <svg viewBox="0 0 32 32" aria-hidden="true">
        <circle className="brand-track" cx="16" cy="16" r="11" />
        <path className="brand-ring" d="M16 5a11 11 0 1 1-8.9 4.5" />
        <circle className="brand-dot" cx="16" cy="5" r="2.2" />
      </svg>
    </div>
  );
}

function formattedTime(timestamp: number | null) {
  return timestamp
    ? new Date(timestamp).toLocaleTimeString('zh-CN', {
      hour: '2-digit',
      minute: '2-digit'
    })
    : '尚无';
}

function diagnosticText(info: DiagnosticInfo) {
  return [
    `Codex Quota Ring: ${info.appVersion}`,
    `系统: ${info.windowsVersion}`,
    `Codex: ${info.codexFound ? '已找到' : '未找到'}`,
    `候选来源: ${info.candidateSource ?? '无'}`,
    `最后成功刷新: ${formattedTime(info.lastSuccessAt)}`,
    `最近错误: ${info.lastErrorCode ?? '无'}`
  ].join('\n');
}

function App() {
  const [status, setStatus] = useState<QuotaStatus>(unavailable);
  const [preferences, setPreferences] = useState<Preferences | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [clock, setClock] = useState(Date.now());
  const [diagnostics, setDiagnostics] = useState<DiagnosticInfo | null>(null);
  const [copied, setCopied] = useState(false);
  const [dockState, setDockState] = useState<DockState>(initialDockState);
  const statusRef = useRef(status);
  const lastTickRef = useRef(Date.now());
  const lastResetRefreshRef = useRef<string | null>(null);
  const dockExpandTimerRef = useRef<number | null>(null);
  const dockCollapseTimerRef = useRef<number | null>(null);

  useEffect(() => {
    statusRef.current = status;
  }, [status]);

  const refresh = async (force = true) => {
    if (refreshing && force) return;
    if (force) setRefreshing(true);
    try {
      setStatus(await invoke<QuotaStatus>('refresh_quota', { force }));
    } finally {
      if (force) setRefreshing(false);
    }
  };

  const loadDiagnostics = async () => {
    setDiagnostics(await invoke<DiagnosticInfo>('get_diagnostics'));
  };

  useEffect(() => {
    void invoke<QuotaStatus>('get_quota_status').then(setStatus);
    void invoke<Preferences>('get_preferences').then(async value => {
      setPreferences(value);
      await getCurrentWebview().setZoom(value.uiScale);
    });
    void invoke<DockState>('get_dock_state').then(setDockState);

    const quotaListener = listen<QuotaStatus>('quota-updated', event => {
      setStatus(event.payload);
    });
    const preferenceListener = listen<Preferences>('preferences-updated', async event => {
      setPreferences(event.payload);
      await getCurrentWebview().setZoom(event.payload.uiScale);
    });
    const settingsListener = listen('settings-open-requested', () => {
      setSettingsOpen(true);
      void loadDiagnostics();
    });
    const dockListener = listen<DockState>('dock-state-updated', event => {
      setDockState(event.payload);
    });

    const timer = window.setInterval(() => {
      const now = Date.now();
      setClock(now);
      if (now - lastTickRef.current > 90_000) {
        void refresh(false);
      }
      lastTickRef.current = now;

      const current = statusRef.current;
      const due = [current.fiveHour?.resetsAt, current.weekly?.resetsAt]
        .filter((value): value is number => resetDue(value, now));
      if (due.length > 0) {
        const resetKey = due.sort().join(':');
        if (lastResetRefreshRef.current !== resetKey) {
          lastResetRefreshRef.current = resetKey;
          void refresh(false);
        }
      }
    }, 30_000);

    return () => {
      window.clearInterval(timer);
      void quotaListener.then(unlisten => unlisten());
      void preferenceListener.then(unlisten => unlisten());
      void settingsListener.then(unlisten => unlisten());
      void dockListener.then(unlisten => unlisten());
      if (dockExpandTimerRef.current != null) {
        window.clearTimeout(dockExpandTimerRef.current);
      }
      if (dockCollapseTimerRef.current != null) {
        window.clearTimeout(dockCollapseTimerRef.current);
      }
    };
  }, []);

  const updatePreferences = async (patch: Partial<Preferences>) => {
    const next = await invoke<Preferences>('update_preferences', { patch });
    setPreferences(next);
    if (patch.uiScale != null) {
      await getCurrentWebview().setZoom(next.uiScale);
    }
  };

  const presentation = statusPresentation(status.state);
  const statusText = status.state === 'ready'
    ? `更新于 ${formattedTime(status.lastSuccessAt)}`
    : status.state === 'stale'
      ? `数据可能已过期 · 上次成功 ${formattedTime(status.lastSuccessAt)}`
      : status.message ?? '额度暂不可用';

  const toggleSettings = async () => {
    const next = !settingsOpen;
    if (next && dockCollapseTimerRef.current != null) {
      window.clearTimeout(dockCollapseTimerRef.current);
      dockCollapseTimerRef.current = null;
    }
    await invoke('set_settings_open', { open: next });
    setSettingsOpen(next);
    if (next) void loadDiagnostics();
  };

  const beginDrag = (event: MouseEvent<HTMLElement>) => {
    if (event.button !== 0 || preferences?.lockWindowPosition) return;
    const target = event.target as HTMLElement;
    if (target.closest('button, input, select, label, .settings, .resize-grip')) return;
    void getCurrentWindow().startDragging();
  };

  const beginResize = (event: MouseEvent<HTMLButtonElement>) => {
    if (event.button !== 0 || preferences?.lockWindowPosition) return;
    event.stopPropagation();
    void getCurrentWindow().startResizeDragging('SouthEast');
  };

  const copyDiagnostics = async () => {
    if (!diagnostics) return;
    await navigator.clipboard.writeText(diagnosticText(diagnostics));
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  const cancelDockTimers = () => {
    if (dockExpandTimerRef.current != null) {
      window.clearTimeout(dockExpandTimerRef.current);
      dockExpandTimerRef.current = null;
    }
    if (dockCollapseTimerRef.current != null) {
      window.clearTimeout(dockCollapseTimerRef.current);
      dockCollapseTimerRef.current = null;
    }
  };

  const scheduleDockExpand = () => {
    if (!dockState.docked || dockState.expanded || dockExpandTimerRef.current != null) return;
    dockExpandTimerRef.current = window.setTimeout(() => {
      dockExpandTimerRef.current = null;
      void invoke<DockState>('set_dock_expanded', { expanded: true }).then(setDockState);
    }, 150);
  };

  const scheduleDockCollapse = () => {
    if (!dockState.docked || !dockState.expanded || settingsOpen) return;
    if (dockCollapseTimerRef.current != null) {
      window.clearTimeout(dockCollapseTimerRef.current);
    }
    dockCollapseTimerRef.current = window.setTimeout(() => {
      dockCollapseTimerRef.current = null;
      void invoke<DockState>('set_dock_expanded', { expanded: false }).then(setDockState);
    }, preferences?.dockAutoCollapseDelayMs ?? 800);
  };

  const {
    primaryLimit,
    secondaryLimit,
    primaryLabel,
    secondaryLabel
  } = selectQuotaWindows(status, preferences?.primaryQuotaWindow ?? 'fiveHour');
  const creditBalance = displayCredit(status.credits?.balance);
  const creditLimit = displayCredit(status.credits?.limit);
  const resetCredits = status.credits?.resetCreditsAvailable;
  const showCredits = Boolean(
    preferences?.showCredits
      && status.credits
      && (creditBalance != null || creditLimit != null || resetCredits != null || status.credits.planType)
  );
  const compactDock = dockState.docked && !dockState.expanded;
  const compactLabel = dockMiniLabel(preferences?.primaryQuotaWindow ?? 'fiveHour');

  if (compactDock) {
    return (
      <main
        className={`app-shell dock-compact edge-${dockState.edge ?? 'top'}${status.state === 'stale' ? ' stale' : ''}`}
        onMouseEnter={scheduleDockExpand}
        onMouseLeave={cancelDockTimers}
        title={`${primaryLabel} ${displayPercent(primaryLimit?.remainingPercent)}`}
      >
        <section className="mini-rail">
          <MiniGauge limit={primaryLimit} stale={status.state === 'stale'} />
          <strong className={quotaLevel(primaryLimit?.remainingPercent)}>
            {displayPercent(primaryLimit?.remainingPercent)}
          </strong>
          <span>{compactLabel}</span>
        </section>
      </main>
    );
  }

  return (
    <main
      className={`app-shell ${presentation.className}${settingsOpen ? ' settings-open' : ''}`}
      onMouseDown={beginDrag}
      onMouseEnter={cancelDockTimers}
      onMouseLeave={scheduleDockCollapse}
    >
      <section className="rail">
        <section className="overview">
          <Gauge
            label={primaryLabel}
            limit={primaryLimit}
            stale={status.state === 'stale'}
          />
          <div className="primary-copy">
            <p className="eyebrow">CODEX · {primaryLabel}</p>
            <h1>{primaryLimit?.remainingPercent == null ? '额度不可用' : '剩余额度'}</h1>
            <p className="reset">{resetDistance(primaryLimit?.resetsAt, clock)}</p>
            {status.state === 'stale' && <span className="stale-badge">数据可能已过期</span>}
          </div>
        </section>

        <div className="divider" />

        <section className="secondary">
          <div className="secondary-header">
            <span>{secondaryLabel}</span>
            <strong className={quotaLevel(secondaryLimit?.remainingPercent)}>
              {displayPercent(secondaryLimit?.remainingPercent)}
            </strong>
          </div>
          <p>{resetDistance(secondaryLimit?.resetsAt, clock)}</p>
          {showCredits && (
            <div className="credits-line" title="只读账户额度信息">
              {creditBalance != null && (
                <span>Credits {creditBalance}{creditLimit != null ? ` / ${creditLimit}` : ''}</span>
              )}
              {resetCredits != null && <span>重置券 {resetCredits}</span>}
              {status.credits?.planType && <span>{status.credits.planType}</span>}
            </div>
          )}
          <small title={`${presentation.label}。${status.message ?? statusText}`}>{statusText}</small>
        </section>

        <div className="actions">
          <BrandMark />
          <button
            className={`icon-button refresh-button${refreshing ? ' spinning' : ''}`}
            onClick={() => void refresh(true)}
            aria-label="立即刷新"
            title="立即刷新"
            disabled={refreshing}
          >
            ↻
          </button>
          <button
            className={`icon-button${settingsOpen ? ' active' : ''}`}
            onClick={() => void toggleSettings()}
            aria-label="设置"
            title="设置"
          >
            ⚙
          </button>
        </div>
      </section>

      {settingsOpen && preferences && (
        <section className="settings">
          <div className="settings-title">
            <span>v0.3.1 设置</span>
            <small>{preferences.lockWindowPosition ? '窗口位置已锁定' : '拖动空白处可移动窗口'}</small>
          </div>

          <div className="settings-grid">
            <label>
              <span>主环显示</span>
              <select
                value={preferences.primaryQuotaWindow}
                onChange={event => void updatePreferences({
                  primaryQuotaWindow: event.target.value as Preferences['primaryQuotaWindow']
                })}
              >
                <option value="fiveHour">5 小时额度</option>
                <option value="weekly">周额度</option>
              </select>
            </label>

            <label>
              <span>界面缩放</span>
              <select
                value={preferences.uiScale}
                onChange={event => void updatePreferences({
                  uiScale: Number(event.target.value)
                })}
              >
                <option value={0.8}>80%</option>
                <option value={1}>100%</option>
                <option value={1.25}>125%</option>
                <option value={1.5}>150%</option>
              </select>
            </label>

            <label>
              <span>刷新间隔</span>
              <select
                value={preferences.refreshIntervalSecs}
                onChange={event => void updatePreferences({
                  refreshIntervalSecs: Number(event.target.value)
                })}
              >
                <option value={60}>1 分钟</option>
                <option value={300}>5 分钟</option>
                <option value={900}>15 分钟</option>
              </select>
            </label>

            <label className="check-setting">
              <input
                type="checkbox"
                checked={preferences.showCredits}
                onChange={event => void updatePreferences({ showCredits: event.target.checked })}
              />
              显示 Credits
            </label>

            <label className="check-setting">
              <input
                type="checkbox"
                checked={preferences.alwaysOnTop}
                onChange={event => void updatePreferences({ alwaysOnTop: event.target.checked })}
              />
              始终置顶
            </label>

            <label className="check-setting">
              <input
                type="checkbox"
                checked={preferences.lockWindowPosition}
                onChange={event => void updatePreferences({
                  lockWindowPosition: event.target.checked
                })}
              />
              锁定窗口位置
            </label>

            <label className="check-setting">
              <input
                type="checkbox"
                checked={preferences.autostart}
                onChange={event => void updatePreferences({ autostart: event.target.checked })}
              />
              登录时启动
            </label>

            <label className="check-setting">
              <input
                type="checkbox"
                checked={preferences.startHiddenOnAutostart}
                onChange={event => void updatePreferences({
                  startHiddenOnAutostart: event.target.checked
                })}
              />
              开机启动后隐藏
            </label>

            <label className="check-setting">
              <input
                type="checkbox"
                checked={preferences.autoShowOnCodex}
                onChange={event => void updatePreferences({
                  autoShowOnCodex: event.target.checked
                })}
              />
              Codex 启动时显示
            </label>

            <label className="check-setting">
              <input
                type="checkbox"
                checked={preferences.autoHideOnCodexClose}
                onChange={event => void updatePreferences({
                  autoHideOnCodexClose: event.target.checked
                })}
              />
              Codex 关闭后隐藏
            </label>

            <label className="check-setting">
              <input
                type="checkbox"
                checked={preferences.edgeDockEnabled}
                onChange={event => void updatePreferences({
                  edgeDockEnabled: event.target.checked
                })}
              />
              启用贴边收起
            </label>
          </div>

          <section className="diagnostics">
            <div>
              <strong>诊断信息</strong>
              <span>
                {diagnostics
                  ? `${diagnostics.codexFound ? '已找到 Codex' : '未找到 Codex'} · ${diagnostics.lastErrorCode ?? '无错误'}`
                  : '正在读取…'}
              </span>
            </div>
            <button onClick={() => void copyDiagnostics()}>
              {copied ? '已复制' : '复制诊断信息'}
            </button>
            {status.message && <p title={status.message}>{status.message}</p>}
          </section>
        </section>
      )}

      {!preferences?.lockWindowPosition && (
        <button
          className="resize-grip"
          onMouseDown={beginResize}
          aria-label="调整窗口大小"
          title="拖动调整窗口大小"
        />
      )}
    </main>
  );
}

export default App;
