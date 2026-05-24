const invoke = window.__TAURI__?.core?.invoke;
const windowApi = window.__TAURI__?.window;
const pages = ['home', 'nodes', 'subscriptions', 'settings'];
const navBtns = document.querySelectorAll('.nb');
let settings = null;
let status = null;
let currentGroup = 'PROXY';
let lastTraffic = null;
let chartPoints = Array.from({ length: 12 }, () => ({ up: 0, down: 0 }));
let chartHoverIndex = -1;
let proxySnapshot = null;
let proxyGroupsState = [];
let nodeSearchQuery = '';
let nodeSourceFilter = 'all';
let overallTesting = false;
let logExpanded = false;
let maintenanceInfo = null;
const expandedProxyGroups = new Set();
const nodeDelayOverrides = new Map();
const groupTestingState = new Set();
const nodeTestingState = new Set();
let lastCoreExitMessage = null;

const DEFAULT_SETTINGS = Object.freeze({
  localMixedPort: 7890,
  clashApiPort: 9090,
  clashApiSecret: '',
  tunEnabled: false,
  fakeDnsEnabled: true,
  proxyEnabled: false,
  mode: 'rule',
  fallback: 'direct',
  autoUpdateHours: 24,
  followSystemTheme: true,
  notifyOnFailure: true,
  autoLaunch: true,
  autoStartProxy: true,
  startHidden: false,
  hideToTray: true,
  autoSelectFastest: true,
  autoSwitchOnFailure: true,
  speedTestInterval: 'every1Hour',
  dnsGuardEnabled: true,
  ipv6Enabled: true,
  udpAccelerationEnabled: true,
  allowLan: true,
  experimentalQuic: false,
  customDnsServers: [],
  fakeIpV4Range: '198.18.0.0/15',
  fakeIpV6Range: 'fc00::/18',
  tunInterfaceName: 'singbox_tun',
  tunMtu: 1500,
  tunAutoRoute: true,
  tunStrictRoute: true,
  tunRouteExcludeAddress: [
    '10.0.0.0/8',
    '100.64.0.0/10',
    '127.0.0.0/8',
    '169.254.0.0/16',
    '172.16.0.0/12',
    '192.168.0.0/16',
    '::1/128',
    'fc00::/7',
    'fe80::/10'
  ],
  converterUrl: null,
  resumeAfterElevation: false,
  subscriptions: []
});

function normalizeSettings(next = {}){
  const source = next && typeof next === 'object' ? next : {};
  return {
    ...DEFAULT_SETTINGS,
    ...source,
    subscriptions: Array.isArray(source.subscriptions)
      ? source.subscriptions.map(item => ({
          ...item,
          enabled: item?.enabled !== false,
          tags: Array.isArray(item?.tags) ? item.tags : []
        }))
      : [],
    customDnsServers: Array.isArray(source.customDnsServers) ? source.customDnsServers : [],
    tunRouteExcludeAddress: Array.isArray(source.tunRouteExcludeAddress) ? source.tunRouteExcludeAddress : DEFAULT_SETTINGS.tunRouteExcludeAddress
  };
}

function isRemoteSubscription(subscription){
  return Boolean(subscription?.url && /^(https?:)\/\//.test(subscription.url));
}

function enabledSubscriptions(){
  return (settings?.subscriptions || []).filter(item => item.enabled !== false);
}
