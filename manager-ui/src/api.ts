export type Session = {
  authenticated: boolean;
  apiVersion: string;
  namespace: string;
  authEnabled: boolean;
};

export type BattlegroupSummary = {
  namespace: string;
  name: string;
  title: string;
  phase: string;
  stop: boolean;
  serverSets: number;
  serverImage: string;
};

export type PodSummary = {
  name: string;
  phase: string;
  ready: boolean;
  restarts: number;
  containers: string[];
  containerResources: ContainerResource[];
  nodeName?: string;
  createdAt?: string;
};

export type ContainerResource = {
  name: string;
  image?: string;
  cpuRequest?: string;
  cpuLimit?: string;
  memoryRequest?: string;
  memoryLimit?: string;
};

export type ServicePortSummary = {
  name?: string;
  port: number;
  targetPort?: string;
  nodePort?: number;
  protocol?: string;
};

export type ServiceSummary = {
  name: string;
  serviceType?: string;
  clusterIp?: string;
  externalIps: string[];
  ports: ServicePortSummary[];
};

export type Workloads = {
  pods: PodSummary[];
  services: ServiceSummary[];
};

export type EventSummary = {
  name: string;
  eventType: string;
  reason: string;
  message: string;
  involvedKind: string;
  involvedName: string;
  count: number;
  firstSeen?: string;
  lastSeen?: string;
};

export type EventsResponse = {
  namespace: string;
  events: EventSummary[];
};

export type PersistentVolumeClaimSummary = {
  name: string;
  phase: string;
  requestedStorage?: string;
  capacityStorage?: string;
  storageClass?: string;
  volumeName?: string;
  accessModes: string[];
  createdAt?: string;
};

export type StorageResponse = {
  namespace: string;
  claims: PersistentVolumeClaimSummary[];
};

export type PlayerSummary = {
  active: number;
  online: number;
  inTransit: number;
  gracePeriod: number;
  completion: number;
  queued: number;
  loginRequestsTotal: number;
  travelRequestsTotal: number;
};

export type DirectorPlayerLists = {
  all: string[];
  online: string[];
  inTransit: string[];
  gracePeriod: string[];
  completion: string[];
  queued: string[];
};

export type DirectorPathCapability = {
  method: string;
  path: string;
};

export type DirectorCapabilities = {
  configured: boolean;
  apiPaths: DirectorPathCapability[];
  uiProxyPath: string;
};

export type DirectorMap = {
  name: string;
  kind: string;
  players: number;
  online: number;
  queued: number;
  servers: DirectorServer[];
  hasOverride: boolean;
};

export type DirectorServer = {
  label: string;
  serverId: string;
  partitionId?: number;
  dimensionIndex?: number;
  players: number;
  online: number;
  queued?: number;
  status: string;
  heartbeatSecondsAgo?: number;
  hasOverride: boolean;
};

export type DirectorMapConfigDetail = {
  name: string;
  kind: string;
  configKey: string;
  effectiveConfig: unknown;
  webOverrideConfig: unknown;
  updatePayloadTemplate: unknown;
  servers: DirectorServer[];
  hasOverride: boolean;
};

export type Overview = {
  status: {
    apiVersion: string;
    namespace: string;
    authEnabled: boolean;
    directorConfigured: boolean;
    battlegroups: number;
    pods: number;
    services: number;
  };
  battlegroups: BattlegroupSummary[];
  workloads: Workloads;
  directorAvailable: boolean;
  players: PlayerSummary | null;
  maps: DirectorMap[];
};

export type ManagerSelf = {
  apiVersion: string;
  startedUnixMs: number;
  uptimeSeconds: number;
  pid: number;
  namespace: string;
  port: number;
  authEnabled: boolean;
  directorConfigured: boolean;
  currentExe: string;
  serviceName: string;
  binaryPath: string;
  envPath: string;
  logPath: string;
};

export type ManagerLogResponse = {
  path: string;
  available: boolean;
  truncated: boolean;
  tailLines: number;
  lines: string[];
};

export type WorldLayout = {
  haggaBasinInstances: number;
  socialHubsEnabled: boolean;
  deepDesertPveInstances: number;
  deepDesertPvpInstances: number;
  deepDesertTotalInstances: number;
  deepDesertPartitionIds: number[];
  restartRequired: boolean;
  warnings: string[];
};

export type WorldLayoutUpdateResponse = {
  layout: WorldLayout;
  battlegroupPatched: boolean;
  pvpConfigUpdated: boolean;
  restartRequired: boolean;
  warnings: string[];
};

export type LogsResponse = {
  pod: string;
  container?: string;
  lines: string[];
};

export type LogExportResponse = {
  namespace: string;
  generatedAtUnixMs: number;
  tailLines: number;
  pods: Array<{
    name: string;
    phase: string;
    containers: Array<{ name: string; lines: string[] }>;
  }>;
  errors: Array<{ pod: string; container?: string; message: string }>;
};

export type UserSettingsCatalog = {
  files: UserSettingsFileSummary[];
};

export type UserSettingsFileSummary = {
  id: string;
  fileName: string;
  description: string;
};

export type UserSettingsFile = {
  id: string;
  fileName: string;
  path: string;
  content: string;
  sizeBytes: number;
  sections: IniSection[];
};

export type IniSection = {
  name: string;
  entries: Array<{ key: string; value: string; line: number }>;
};

export type UserSettingsUpdateResponse = {
  file: UserSettingsFile;
  restartRecommended: boolean;
};

export type UserSettingsPreviewResponse = {
  file: string;
  changed: boolean;
  addedLines: number;
  removedLines: number;
  hunks: UserSettingsDiffHunk[];
};

export type UserSettingsDiffHunk = {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: UserSettingsDiffLine[];
};

export type UserSettingsDiffLine = {
  kind: "equal" | "insert" | "delete";
  oldLine?: number;
  newLine?: number;
  text: string;
};

export type UserSettingsBackupSummary = {
  id: string;
  fileName: string;
  sizeBytes: number;
  modifiedAt?: string;
};

export type UserSettingsBackupsResponse = {
  file: string;
  backups: UserSettingsBackupSummary[];
};

export type UserSettingsBackupCreateResponse = {
  backup: UserSettingsBackupSummary;
};

export type UserSettingsRestoreResponse = {
  file: UserSettingsFile;
  restoredFrom: string;
  restartRecommended: boolean;
};

export type TelemetryEnvelope = {
  eventType: "snapshot" | "error";
  timeUnixMs: number;
  payload: TelemetrySnapshot | { message?: string };
};

export type TelemetrySnapshot = {
  namespace: string;
  battlegroups: BattlegroupSummary[];
  pods: PodSummary[];
  services: ServiceSummary[];
};

export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(path, {
    ...init,
    credentials: "include",
    headers: {
      "Content-Type": "application/json",
      ...(init.headers || {}),
    },
  });
  if (response.status === 401) {
    throw new ApiError("unauthorized", 401);
  }
  const text = await response.text();
  const value = text ? JSON.parse(text) : {};
  if (!response.ok) {
    throw new ApiError(value.error || response.statusText, response.status);
  }
  return value as T;
}

export class ApiError extends Error {
  constructor(message: string, public status: number) {
    super(message);
  }
}
