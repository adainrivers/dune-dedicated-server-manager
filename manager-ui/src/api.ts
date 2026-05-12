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
  nodeName?: string;
  createdAt?: string;
};

export type Workloads = {
  pods: PodSummary[];
  services: unknown[];
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

export type LogsResponse = {
  pod: string;
  container?: string;
  lines: string[];
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

export type TelemetryEnvelope = {
  eventType: "snapshot" | "error";
  timeUnixMs: number;
  payload: TelemetrySnapshot | { message?: string };
};

export type TelemetrySnapshot = {
  namespace: string;
  battlegroups: BattlegroupSummary[];
  pods: PodSummary[];
  services: unknown[];
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
