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

export type DirectorPlayerLists = {
  all: string[];
  online: string[];
  inTransit: string[];
  gracePeriod: string[];
  completion: string[];
  queued: string[];
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
