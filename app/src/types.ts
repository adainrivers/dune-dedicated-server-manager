import type { LucideIcon } from "lucide-react";

export type CommandFailure = {
  message: string;
  stdout?: string;
  stderr?: string;
  code?: number;
};

export type HostStatus = {
  user: string;
  isElevated: boolean;
  hypervAvailable: boolean;
  vmmsStatus?: string | null;
  sshAvailable: boolean;
  defaultInstallPathExists: boolean;
  defaultInstallPath: string;
};

export type AppConfig = {
  installPath: string;
  vmName: string;
  vmIp: string;
  sshUser: string;
  sshPath: string;
  managerApiUrl: string;
  managerApiToken: string;
  managerApiNamespace: string;
  managerApiImage: string;
  managerApiBinaryPath: string;
  managerApiDirectorUrl: string;
};

export type VmStatus = {
  name: string;
  state: string;
  status: string;
  memoryAssignedBytes: number;
  uptime: string;
  path: string;
  configurationLocation: string;
  ipAddresses: string[];
};

export type GuestConnection = {
  ip: string;
  sshUser: string;
  keyPath: string;
  connected: boolean;
  sudo: boolean;
  hostname: string;
  kernel: string;
  kubectl: boolean;
};

export type BattleGroupSummary = {
  namespace: string;
  name: string;
  title: string;
  phase: string;
  stop: boolean;
  serverImage: string;
  fileBrowserUrl?: string | null;
  directorUrl?: string | null;
  serverSets: number;
};

export type ServerSetSummary = {
  map: string;
  replicas: number;
  memoryLimit: string;
  dedicatedScaling: boolean;
  image: string;
};

export type BattleGroupDetail = {
  namespace: string;
  name: string;
  title: string;
  phase: string;
  stop: boolean;
  databasePhase: string;
  serverGroupPhase: string;
  gatewayPhase: string;
  directorPhase: string;
  serverImage: string;
  utilityImages: string[];
  serverSets: ServerSetSummary[];
};

export type KubeItem = {
  metadata?: {
    name?: string;
    namespace?: string;
    creationTimestamp?: string;
  };
  status?: Record<string, unknown>;
  spec?: Record<string, unknown>;
};

export type Workloads = {
  pods: {
    items?: KubeItem[];
  };
  services: {
    items?: KubeItem[];
  };
};

export type ManagerPodSummary = {
  name: string;
  phase: string;
  ready: boolean;
  restarts: number;
  nodeName?: string | null;
  createdAt?: string | null;
};

export type ManagerServicePortSummary = {
  name?: string | null;
  port: number;
  targetPort?: string | null;
  nodePort?: number | null;
  protocol?: string | null;
};

export type ManagerServiceSummary = {
  name: string;
  serviceType?: string | null;
  clusterIp?: string | null;
  externalIps: string[];
  ports: ManagerServicePortSummary[];
};

export type ManagerWorkloads = {
  pods: ManagerPodSummary[];
  services: ManagerServiceSummary[];
};

export type ManagerApiStatus = {
  namespace: string;
  authEnabled: boolean;
  directorConfigured: boolean;
  battlegroups: number;
  pods: number;
  services: number;
};

export type TelemetryEnvelope = {
  eventType: string;
  timeUnixMs: number;
  payload?: {
    battlegroups?: unknown[];
    pods?: unknown[];
    services?: unknown[];
  };
};

export type ManagerApiInstallResult = {
  namespace: string;
  deployment: string;
  service: string;
  binaryPath: string;
  url: string;
};

export type DirectorPlayerSummary = {
  active: number;
  online: number;
  inTransit: number;
  gracePeriod: number;
  completion: number;
  queued: number;
  loginRequestsTotal: number;
  travelRequestsTotal: number;
};

export type DirectorServerSummary = {
  label: string;
  serverId: string;
  partitionId?: number | null;
  dimensionIndex?: number | null;
  players: number;
  online: number;
  queued?: number | null;
  status: string;
  heartbeatSecondsAgo?: number | null;
  hasOverride: boolean;
};

export type DirectorMapSummary = {
  name: string;
  kind: string;
  players: number;
  online: number;
  queued: number;
  servers: DirectorServerSummary[];
  hasOverride: boolean;
};

export type FlsDraft = {
  heartbeatSeconds: string;
  settingsSeconds: string;
};

export type TransferDraft = {
  deleteOrigin: boolean;
  incoming: string;
  outgoing: boolean;
  exportTimeout: string;
  importTimeout: string;
  freeFrom: boolean;
  freeTo: boolean;
  validateTimeout: string;
  worldClosed: boolean;
  worldClosingSoon: boolean;
};

export type MapOverrideDraft = {
  playerHardCap: string;
  updatePlayerCountOnFls: boolean;
  enforceSameHomeDimension: boolean;
  automaticScaling: boolean;
  throttlingSeconds: string;
  minServers: string;
  extraServers: string;
};

export type ViewKey =
  | "overview"
  | "host"
  | "manager"
  | "players"
  | "battlegroups"
  | "workloads"
  | "director"
  | "config"
  | "logs";

export type NavItem = {
  key: ViewKey;
  label: string;
  icon: LucideIcon;
  disabled?: boolean;
};
