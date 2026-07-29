export type LimitWindow = {
  remainingPercent: number | null;
  resetsAt: number | null;
};

export type QuotaStatus = {
  state: 'ready' | 'stale' | 'loading' | 'unavailable';
  message: string | null;
  fiveHour: LimitWindow | null;
  weekly: LimitWindow | null;
  credits: {
    balance: number | null;
    limit: number | null;
    resetCreditsAvailable: number | null;
    planType: string | null;
  } | null;
  updatedAt: number | null;
  lastSuccessAt: number | null;
  errorCode: string | null;
};

export type Preferences = {
  refreshIntervalSecs: number;
  visible: boolean;
  alwaysOnTop: boolean;
  autostart: boolean;
  windowX: number | null;
  windowY: number | null;
  windowWidth: number | null;
  windowHeight: number | null;
  primaryQuotaWindow: 'fiveHour' | 'weekly';
  uiScale: number;
  showCredits: boolean;
  autoShowOnCodex: boolean;
  autoHideOnCodexClose: boolean;
  startHiddenOnAutostart: boolean;
  lockWindowPosition: boolean;
  edgeDockEnabled: boolean;
  dockEdge: 'left' | 'right' | 'top' | null;
  dockMonitorId: string | null;
  dockOffset: number | null;
  dockAutoCollapseDelayMs: number;
};

export type DockState = {
  docked: boolean;
  expanded: boolean;
  edge: 'left' | 'right' | 'top' | null;
};

export type DiagnosticInfo = {
  appVersion: string;
  windowsVersion: string;
  codexFound: boolean;
  candidateSource: string | null;
  lastSuccessAt: number | null;
  lastErrorCode: string | null;
};
