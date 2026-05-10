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
  containers: string[];
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
  apiVersion: string;
  namespace: string;
  authEnabled: boolean;
  directorConfigured: boolean;
  battlegroups: number;
  pods: number;
  services: number;
};

export type ManagerLogResponse = {
  pod: string;
  container?: string | null;
  lines: string[];
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
