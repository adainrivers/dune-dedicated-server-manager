import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  CheckCircle2,
  Database,
  Download,
  ExternalLink,
  HardDrive,
  Map,
  MinusCircle,
  PackagePlus,
  RadioTower,
  Play,
  RefreshCw,
  RotateCcw,
  Server,
  ShieldCheck,
  SlidersHorizontal,
  Square,
  Terminal,
  Users,
  XCircle,
  Wifi,
  type LucideIcon
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

type CommandFailure = {
  message: string;
  stdout?: string;
  stderr?: string;
  code?: number;
};

type HostStatus = {
  user: string;
  isElevated: boolean;
  hypervAvailable: boolean;
  vmmsStatus?: string | null;
  sshAvailable: boolean;
  defaultInstallPathExists: boolean;
  defaultInstallPath: string;
};

type AppConfig = {
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

type VmStatus = {
  name: string;
  state: string;
  status: string;
  memoryAssignedBytes: number;
  uptime: string;
  path: string;
  configurationLocation: string;
  ipAddresses: string[];
};

type GuestConnection = {
  ip: string;
  sshUser: string;
  keyPath: string;
  connected: boolean;
  sudo: boolean;
  hostname: string;
  kernel: string;
  kubectl: boolean;
};

type BattleGroupSummary = {
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

type ServerSetSummary = {
  map: string;
  replicas: number;
  memoryLimit: string;
  dedicatedScaling: boolean;
  image: string;
};

type BattleGroupDetail = {
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

type KubeItem = {
  metadata?: {
    name?: string;
    namespace?: string;
    creationTimestamp?: string;
  };
  status?: Record<string, unknown>;
  spec?: Record<string, unknown>;
};

type Workloads = {
  pods: {
    items?: KubeItem[];
  };
  services: {
    items?: KubeItem[];
  };
};

type ManagerPodSummary = {
  name: string;
  phase: string;
  ready: boolean;
  restarts: number;
  nodeName?: string | null;
  createdAt?: string | null;
};

type ManagerServicePortSummary = {
  name?: string | null;
  port: number;
  targetPort?: string | null;
  nodePort?: number | null;
  protocol?: string | null;
};

type ManagerServiceSummary = {
  name: string;
  serviceType?: string | null;
  clusterIp?: string | null;
  externalIps: string[];
  ports: ManagerServicePortSummary[];
};

type ManagerWorkloads = {
  pods: ManagerPodSummary[];
  services: ManagerServiceSummary[];
};

type ManagerApiStatus = {
  namespace: string;
  authEnabled: boolean;
  directorConfigured: boolean;
  battlegroups: number;
  pods: number;
  services: number;
};

type TelemetryEnvelope = {
  eventType: string;
  timeUnixMs: number;
  payload?: {
    battlegroups?: unknown[];
    pods?: unknown[];
    services?: unknown[];
  };
};

type ManagerApiInstallResult = {
  namespace: string;
  deployment: string;
  service: string;
  binaryPath: string;
  url: string;
};

type DirectorPlayerSummary = {
  active: number;
  online: number;
  inTransit: number;
  gracePeriod: number;
  completion: number;
  queued: number;
  loginRequestsTotal: number;
  travelRequestsTotal: number;
};

type DirectorServerSummary = {
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

type DirectorMapSummary = {
  name: string;
  kind: string;
  players: number;
  online: number;
  queued: number;
  servers: DirectorServerSummary[];
  hasOverride: boolean;
};

type FlsDraft = {
  heartbeatSeconds: string;
  settingsSeconds: string;
};

type TransferDraft = {
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

type MapOverrideDraft = {
  playerHardCap: string;
  updatePlayerCountOnFls: boolean;
  enforceSameHomeDimension: boolean;
  automaticScaling: boolean;
  throttlingSeconds: string;
  minServers: string;
  extraServers: string;
};

type ViewKey =
  | "overview"
  | "host"
  | "manager"
  | "players"
  | "battlegroups"
  | "workloads"
  | "director"
  | "config"
  | "logs";

const defaultConfig: AppConfig = {
  installPath: "",
  vmName: "",
  vmIp: "",
  sshUser: "",
  sshPath: "",
  managerApiUrl: "",
  managerApiToken: "",
  managerApiNamespace: "",
  managerApiImage: "",
  managerApiBinaryPath: "",
  managerApiDirectorUrl: ""
};

function formatBytes(bytes: number) {
  if (!bytes) return "0 GB";
  return `${Math.round((bytes / 1024 ** 3) * 10) / 10} GB`;
}

function asError(error: unknown): CommandFailure {
  if (typeof error === "object" && error !== null && "message" in error) {
    return error as CommandFailure;
  }
  return { message: String(error) };
}

function statusTone(value?: string | boolean | null) {
  const text = String(value ?? "").toLowerCase();
  if (
    value === true ||
    [
      "running",
      "ready",
      "healthy",
      "available",
      "connected",
      "online",
      "operating normally",
      "active",
      "succeeded",
      "ok"
    ].includes(text)
  ) {
    return "good";
  }
  if (value === false || ["stopped", "suspended", "disabled", "offline", "error", "failed"].includes(text)) {
    return "bad";
  }
  return "warn";
}

function StatusPill({ value }: { value?: string | boolean | null }) {
  const label = typeof value === "boolean" ? (value ? "Yes" : "No") : value || "Unknown";
  return <span className={`pill ${statusTone(value)}`}>{label}</span>;
}

function StatusLamp({ value, label }: { value?: string | boolean | null; label: string }) {
  const tone = statusTone(value);
  const Icon = tone === "good" ? CheckCircle2 : tone === "bad" ? XCircle : MinusCircle;
  const text = typeof value === "boolean" ? (value ? "Ready" : "Unavailable") : value || "Unknown";
  return (
    <span className={`status-lamp ${tone}`} title={`${label}: ${text}`} aria-label={`${label}: ${text}`}>
      <Icon size={18} />
    </span>
  );
}

function InfoRow({ label, value }: { label: string; value?: string | number | null }) {
  return (
    <div className="info-row">
      <span>{label}</span>
      <strong>{value || "Unknown"}</strong>
    </div>
  );
}

function StatusInfoRow({ label, value }: { label: string; value?: string | boolean | null }) {
  return (
    <div className="info-row">
      <span>{label}</span>
      <strong>
        <StatusPill value={value} />
      </strong>
    </div>
  );
}

function EmptyState({ text }: { text: string }) {
  return <div className="empty-state">{text}</div>;
}

function Metric({ label, value }: { label: string; value?: string | number | null }) {
  return (
    <div className="metric">
      <strong>{value ?? "Unknown"}</strong>
      <span>{label}</span>
    </div>
  );
}

function valueAt(value: unknown, path: string[]) {
  let current = value;
  for (const key of path) {
    if (!current || typeof current !== "object" || !(key in current)) return null;
    current = (current as Record<string, unknown>)[key];
  }
  if (current === null || current === undefined) return null;
  if (typeof current === "boolean") return current ? "true" : "false";
  if (typeof current === "number" || typeof current === "string") return current;
  return JSON.stringify(current);
}

function numberAt(value: unknown, path: string[], fallback = "") {
  const found = valueAt(value, path);
  return found === null ? fallback : String(found);
}

function boolAt(value: unknown, path: string[], fallback = false) {
  let current = value;
  for (const key of path) {
    if (!current || typeof current !== "object" || !(key in current)) return fallback;
    current = (current as Record<string, unknown>)[key];
  }
  return typeof current === "boolean" ? current : fallback;
}

function nullableNumber(value: string) {
  const trimmed = value.trim();
  return trimmed ? Number(trimmed) : null;
}

function delay(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function generateToken() {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export default function App() {
  const [config, setConfig] = useState<AppConfig>(defaultConfig);
  const [host, setHost] = useState<HostStatus | null>(null);
  const [vm, setVm] = useState<VmStatus | null>(null);
  const [guest, setGuest] = useState<GuestConnection | null>(null);
  const [battleGroups, setBattleGroups] = useState<BattleGroupSummary[]>([]);
  const [battleGroupDetail, setBattleGroupDetail] = useState<BattleGroupDetail | null>(null);
  const [selectedNamespace, setSelectedNamespace] = useState<string>("");
  const [workloads, setWorkloads] = useState<Workloads | null>(null);
  const [errors, setErrors] = useState<CommandFailure[]>([]);
  const [busy, setBusy] = useState(false);
  const [snapshotPath, setSnapshotPath] = useState<string>("");
  const [configSaved, setConfigSaved] = useState(false);
  const [configLoaded, setConfigLoaded] = useState(false);
  const [managerStatus, setManagerStatus] = useState<ManagerApiStatus | null>(null);
  const [managerTelemetry, setManagerTelemetry] = useState<TelemetryEnvelope | null>(null);
  const [managerSocketState, setManagerSocketState] = useState<"disabled" | "connecting" | "connected" | "error">(
    "disabled"
  );
  const [managerError, setManagerError] = useState("");
  const [managerInstall, setManagerInstall] = useState<ManagerApiInstallResult | null>(null);
  const [directorPlayers, setDirectorPlayers] = useState<DirectorPlayerSummary | null>(null);
  const [directorMaps, setDirectorMaps] = useState<DirectorMapSummary[]>([]);
  const [directorFlsConfig, setDirectorFlsConfig] = useState<Record<string, unknown> | null>(null);
  const [directorTransferConfig, setDirectorTransferConfig] = useState<Record<string, unknown> | null>(null);
  const [selectedDirectorMap, setSelectedDirectorMap] = useState("");
  const [flsDraft, setFlsDraft] = useState<FlsDraft>({ heartbeatSeconds: "", settingsSeconds: "" });
  const [transferDraft, setTransferDraft] = useState<TransferDraft>({
    deleteOrigin: true,
    incoming: "0",
    outgoing: false,
    exportTimeout: "",
    importTimeout: "",
    freeFrom: false,
    freeTo: false,
    validateTimeout: "",
    worldClosed: false,
    worldClosingSoon: false
  });
  const [mapOverrideDraft, setMapOverrideDraft] = useState<MapOverrideDraft>({
    playerHardCap: "",
    updatePlayerCountOnFls: false,
    enforceSameHomeDimension: false,
    automaticScaling: false,
    throttlingSeconds: "",
    minServers: "",
    extraServers: ""
  });
  const [activeView, setActiveView] = useState<ViewKey>("overview");

  const selectedBattleGroup = useMemo(
    () => battleGroups.find((group) => group.namespace === selectedNamespace) ?? battleGroups[0],
    [battleGroups, selectedNamespace]
  );
  const vmState = vm?.state.toLowerCase() ?? "";
  const vmIsRunning = vmState === "running";
  const vmIsStarting = vmState === "starting";
  const vmIsChanging = ["starting", "stopping", "pausing", "resuming", "resetting", "saving"].includes(vmState);
  const canControlVm = Boolean(host?.isElevated && host?.hypervAvailable && vm);
  const startVmDisabledReason = busy
    ? "An operation is already running"
    : !host?.isElevated
      ? "VM controls require the app to run elevated"
      : !host?.hypervAvailable
        ? "Hyper-V is unavailable"
        : !vm
          ? "VM was not detected"
          : vmIsRunning
            ? "VM is already running"
            : vmIsChanging
              ? "VM is changing state"
              : "Start VM";
  const stopVmDisabledReason = busy
    ? "An operation is already running"
    : !host?.isElevated
      ? "VM controls require the app to run elevated"
      : !host?.hypervAvailable
        ? "Hyper-V is unavailable"
        : !vm
          ? "VM was not detected"
          : !vmIsRunning
            ? "VM is not running"
            : vmIsChanging
              ? "VM is changing state"
              : "Stop VM";
  const battleGroupIsStopped =
    selectedBattleGroup?.stop === true || selectedBattleGroup?.phase.toLowerCase() === "stopped";
  const battleGroupIsRunning =
    selectedBattleGroup?.stop === false &&
    ["running", "ready", "starting"].includes(selectedBattleGroup?.phase.toLowerCase() ?? "");
  const canUseGuest = Boolean(vmIsRunning && guest?.connected && guest?.sudo && guest?.kubectl);
  const managerApiConfigured = config.managerApiUrl.trim().length > 0;
  const managerReadiness = managerStatus ? "Ready" : managerApiConfigured ? "Offline" : "Disabled";
  const managerTelemetryState = managerApiConfigured ? managerSocketState : "disabled";
  const canUseManager = managerApiConfigured && Boolean(managerStatus);
  const managerToolsInstalled = canUseManager;
  const directorAvailable = Boolean(managerToolsInstalled && managerStatus?.directorConfigured);
  const managerInstallNamespace = config.managerApiNamespace.trim() || selectedBattleGroup?.namespace || "";
  const canInstallManagerApi = Boolean(canUseGuest && managerInstallNamespace && config.managerApiBinaryPath.trim());
  const managerRequiredViews = ["battlegroups", "workloads", "config", "logs", "players", "director"];
  const directorRequiredViews = ["players", "director"];
  const activeViewRequiresManager = managerRequiredViews.includes(activeView);
  const activeViewRequiresDirector = directorRequiredViews.includes(activeView);
  const viewLabels: Record<ViewKey, string> = {
    overview: "Overview",
    host: "Host & VM",
    manager: "Manager API",
    players: "Players",
    battlegroups: "BattleGroups",
    workloads: "Pods & Services",
    director: "Director",
    config: "Config",
    logs: "Logs"
  };
  const pageTitle = activeView === "overview" ? selectedBattleGroup?.title || "Dune Awakening" : viewLabels[activeView];
  const pageSubtitle =
    activeView === "overview"
      ? selectedBattleGroup?.name || "No battlegroup detected"
      : selectedBattleGroup?.title || selectedBattleGroup?.name || "No battlegroup selected";
  const navItems: { key: ViewKey; label: string; icon: LucideIcon; disabled?: boolean }[] = [
    { key: "overview", label: "Overview", icon: Server },
    { key: "host", label: "Host & VM", icon: HardDrive },
    { key: "manager", label: "Manager API", icon: RadioTower },
    { key: "players", label: "Players", icon: Users, disabled: !directorAvailable },
    { key: "battlegroups", label: "BattleGroups", icon: Activity, disabled: !managerToolsInstalled },
    { key: "workloads", label: "Pods & Services", icon: Database, disabled: !managerToolsInstalled },
    { key: "director", label: "Director", icon: Map, disabled: !directorAvailable },
    { key: "config", label: "Config", icon: SlidersHorizontal, disabled: !managerToolsInstalled },
    { key: "logs", label: "Logs", icon: Terminal, disabled: !managerToolsInstalled }
  ];
  const selectedDirectorMapSummary =
    directorMaps.find((map) => map.name === selectedDirectorMap) ?? directorMaps[0] ?? null;

  async function capture<T>(label: string, fn: () => Promise<T>): Promise<T | null> {
    try {
      return await fn();
    } catch (error) {
      const commandError = asError(error);
      setErrors((current) => [{ ...commandError, message: `${label}: ${commandError.message}` }, ...current]);
      return null;
    }
  }

  async function managerRequest<T>(path: string, init?: RequestInit): Promise<T> {
    const baseUrl = config.managerApiUrl.trim().replace(/\/$/, "");
    if (!baseUrl) throw new Error("Manager API URL is not configured");
    const headers = new Headers(init?.headers);
    if (config.managerApiToken) {
      headers.set("Authorization", `Bearer ${config.managerApiToken}`);
    }
    const response = await fetch(`${baseUrl}${path}`, { ...init, headers });
    if (!response.ok) {
      const body = await response.text();
      throw new Error(body || `Manager API returned ${response.status}`);
    }
    return (await response.json()) as T;
  }

  function managerWorkloadsToUi(value: ManagerWorkloads): Workloads {
    return {
      pods: {
        items: value.pods.map((pod) => ({
          metadata: { name: pod.name, creationTimestamp: pod.createdAt ?? undefined },
          status: { phase: pod.phase, ready: pod.ready, restarts: pod.restarts }
        }))
      },
      services: {
        items: value.services.map((service) => ({
          metadata: { name: service.name },
          spec: {
            type: service.serviceType,
            clusterIP: service.clusterIp,
            externalIPs: service.externalIps,
            ports: service.ports
          }
        }))
      }
    };
  }

  async function refresh() {
    setBusy(true);
    setErrors([]);
    setSnapshotPath("");

    const nextHost = await capture("Host status", () => invoke<HostStatus>("get_host_status"));
    setHost(nextHost);

    const nextVm = await capture("VM status", () => invoke<VmStatus>("get_vm_status", { vmName: config.vmName }));
    setVm(nextVm);

    const nextVmState = nextVm?.state.toLowerCase() ?? "";
    if (nextVmState !== "running") {
      setGuest(null);
      setManagerStatus(null);
      setManagerTelemetry(null);
      setBattleGroups([]);
      setBattleGroupDetail(null);
      setWorkloads(null);
      setDirectorPlayers(null);
      setDirectorMaps([]);
      setDirectorFlsConfig(null);
      setDirectorTransferConfig(null);
      setBusy(false);
      return;
    }

    const ip = nextVm?.ipAddresses?.[0] ?? guest?.ip ?? config.vmIp;
    if (!ip) {
      setGuest(null);
      setManagerStatus(null);
      setBusy(false);
      return;
    }

    const nextGuest = await capture("Guest connection", () =>
      invoke<GuestConnection>("connect_guest", { installPath: config.installPath, ip, sshUser: config.sshUser })
    );
    setGuest(nextGuest);
    if (nextGuest?.ip && nextGuest.ip !== config.vmIp) {
      const updatedConfig = { ...config, vmIp: nextGuest.ip };
      setConfig(updatedConfig);
      void invoke<AppConfig>("save_app_config", { config: updatedConfig });
    }

    const nextManagerStatus = managerApiConfigured
      ? await capture("Manager API status", () => managerRequest<ManagerApiStatus>("/api/status"))
      : null;
    setManagerStatus(nextManagerStatus);

    let nextBattleGroups = managerApiConfigured
      ? await capture("Manager BattleGroups", () => managerRequest<BattleGroupSummary[]>("/api/battlegroups"))
      : null;
    if (!nextBattleGroups && nextGuest?.kubectl && nextGuest.sudo) {
      nextBattleGroups = await capture("Initial BattleGroup discovery", () =>
        invoke<BattleGroupSummary[]>("get_battlegroups", {
          installPath: config.installPath,
          ip: nextGuest.ip ?? ip,
          sshUser: config.sshUser
        })
      );
    }
    if (nextBattleGroups) {
      setBattleGroups(nextBattleGroups);
      const nextSelected = nextBattleGroups.some((group) => group.namespace === selectedNamespace)
        ? selectedNamespace
        : nextBattleGroups[0]?.namespace ?? "";
      setSelectedNamespace(nextSelected);
      const group = nextBattleGroups.find((candidate) => candidate.namespace === nextSelected);
      if (group) {
        await Promise.all([loadWorkloads(group.namespace), loadBattleGroupDetail(group)]);
      }
    }
    if (managerApiConfigured) {
      await loadDirectorData();
    }
    setBusy(false);
  }

  async function loadWorkloads(_namespace: string) {
    const nextWorkloads = await capture("Manager workloads", () => managerRequest<ManagerWorkloads>("/api/workloads"));
    setWorkloads(nextWorkloads ? managerWorkloadsToUi(nextWorkloads) : null);
  }

  async function loadBattleGroupDetail(group: BattleGroupSummary) {
    const detail = await capture("BattleGroup detail", () =>
      managerRequest<BattleGroupDetail>(
        `/api/battlegroups/${encodeURIComponent(group.namespace)}/${encodeURIComponent(group.name)}`
      )
    );
    setBattleGroupDetail(detail);
  }

  async function loadDirectorData() {
    const [players, maps, flsConfig, transferConfig] = await Promise.all([
      capture("Director players", () => managerRequest<DirectorPlayerSummary>("/api/director/players/summary")),
      capture("Director maps", () => managerRequest<DirectorMapSummary[]>("/api/director/maps")),
      capture("Director FLS config", () => managerRequest<Record<string, unknown>>("/api/director/config/fls")),
      capture("Director character transfer config", () =>
        managerRequest<Record<string, unknown>>("/api/director/config/character-transfer")
      )
    ]);
    if (players) setDirectorPlayers(players);
    if (maps) setDirectorMaps(maps);
    if (flsConfig) setDirectorFlsConfig(flsConfig);
    if (transferConfig) setDirectorTransferConfig(transferConfig);
  }

  async function saveFlsConfig() {
    setBusy(true);
    await capture("Update Director FLS config", () =>
      managerRequest<Record<string, unknown>>("/api/director/config/fls", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          FlsServerHeartbeatUpdateFrequencySeconds: Number(flsDraft.heartbeatSeconds),
          FlsServerSettingsUpdateFrequencySeconds: Number(flsDraft.settingsSeconds)
        })
      })
    );
    await loadDirectorData();
    setBusy(false);
  }

  async function clearFlsConfig() {
    setBusy(true);
    await capture("Clear Director FLS overrides", () =>
      managerRequest<Record<string, unknown>>("/api/director/config/fls", { method: "DELETE" })
    );
    await loadDirectorData();
    setBusy(false);
  }

  async function saveTransferConfig() {
    setBusy(true);
    await capture("Update Director character transfer config", () =>
      managerRequest<Record<string, unknown>>("/api/director/config/character-transfer", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          ShouldDeleteOriginCharactersDuringTransfers: transferDraft.deleteOrigin,
          IncomingCharacterTransfers: Number(transferDraft.incoming),
          AcceptOutgoingCharacterTransfers: transferDraft.outgoing,
          ExportCharacterTimeout: Number(transferDraft.exportTimeout),
          ImportCharacterTimeout: Number(transferDraft.importTimeout),
          FreeToTransferCharactersFrom: transferDraft.freeFrom,
          FreeToTransferCharactersTo: transferDraft.freeTo,
          ValidateBeforeImportCharacterTimeout: Number(transferDraft.validateTimeout),
          ForceIsWorldClosed: transferDraft.worldClosed,
          ForceIsWorldClosingSoon: transferDraft.worldClosingSoon
        })
      })
    );
    await loadDirectorData();
    setBusy(false);
  }

  async function clearTransferConfig() {
    setBusy(true);
    await capture("Clear Director character transfer overrides", () =>
      managerRequest<Record<string, unknown>>("/api/director/config/character-transfer", { method: "DELETE" })
    );
    await loadDirectorData();
    setBusy(false);
  }

  async function saveMapOverride() {
    if (!selectedDirectorMapSummary) return;
    const mapName = selectedDirectorMapSummary.name;
    const config =
      selectedDirectorMapSummary.kind === "Dimension"
        ? {
            MapName: mapName,
            DimensionServerGroupConfig: {
              EnforceSameHomeDimensionForAll: mapOverrideDraft.enforceSameHomeDimension,
              PlayerHardCap: nullableNumber(mapOverrideDraft.playerHardCap),
              ShouldUpdatePlayerCountOnFls: mapOverrideDraft.updatePlayerCountOnFls,
              DimensionOverrides: null
            }
          }
        : selectedDirectorMapSummary.kind === "Instanced"
          ? {
              MapName: mapName,
              ClassicalInstancingGroupConfig: {
                PlayerHardCap: nullableNumber(mapOverrideDraft.playerHardCap),
                ShouldUpdatePlayerCountOnFls: mapOverrideDraft.updatePlayerCountOnFls,
                EnableAutomaticInstanceScaling: mapOverrideDraft.automaticScaling,
                InstanceScalingThrottlingSeconds: nullableNumber(mapOverrideDraft.throttlingSeconds),
                MinServers: nullableNumber(mapOverrideDraft.minServers),
                NumExtraServers: nullableNumber(mapOverrideDraft.extraServers)
              }
            }
          : {
              MapName: mapName,
              SingleServerConfig: {
                PlayerHardCap: nullableNumber(mapOverrideDraft.playerHardCap),
                ShouldUpdatePlayerCountOnFls: mapOverrideDraft.updatePlayerCountOnFls
              }
            };

    setBusy(true);
    await capture("Update Director map override", () =>
      managerRequest<Record<string, unknown>>(
        `/api/director/config/maps/${encodeURIComponent(mapName)}/override`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(config)
        }
      )
    );
    await loadDirectorData();
    setBusy(false);
  }

  async function clearMapOverride(mapName = selectedDirectorMapSummary?.name) {
    if (!mapName) return;
    setBusy(true);
    await capture("Clear Director map override", () =>
      managerRequest<Record<string, unknown>>(
        `/api/director/config/maps/${encodeURIComponent(mapName)}/override`,
        { method: "DELETE" }
      )
    );
    await loadDirectorData();
    setBusy(false);
  }

  async function startVm() {
    setBusy(true);
    if (vm) {
      setVm({ ...vm, state: "Starting", status: "Starting" });
    }
    const nextVm = await capture("Start VM", () => invoke<VmStatus>("start_vm", { vmName: config.vmName }));
    if (nextVm) setVm(nextVm);
    await pollVmLifecycle("running");
    setBusy(false);
  }

  async function stopVm() {
    setBusy(true);
    if (vm) {
      setVm({ ...vm, state: "Stopping", status: "Stopping" });
    }
    const nextVm = await capture("Stop VM", () => invoke<VmStatus>("stop_vm", { vmName: config.vmName }));
    if (nextVm) setVm(nextVm);
    await pollVmLifecycle("off");
    setBusy(false);
  }

  async function pollVmLifecycle(targetState: string) {
    for (let index = 0; index < 20; index += 1) {
      await delay(1500);
      const nextVm = await capture("VM lifecycle", () => invoke<VmStatus>("get_vm_status", { vmName: config.vmName }));
      if (!nextVm) return;
      setVm(nextVm);
      if (nextVm.state.toLowerCase() === targetState) return;
    }
  }

  async function setBattleGroupRunning(running: boolean) {
    if (!selectedBattleGroup) return;
    setBusy(true);
    await capture(running ? "Start battlegroup" : "Stop battlegroup", () =>
      managerRequest<BattleGroupDetail>(
        `/api/battlegroups/${encodeURIComponent(selectedBattleGroup.namespace)}/${encodeURIComponent(
          selectedBattleGroup.name
        )}/${running ? "start" : "stop"}`,
        { method: "POST" }
      )
    );
    await refresh();
    setBusy(false);
  }

  async function restartBattleGroup() {
    if (!selectedBattleGroup) return;
    setBusy(true);
    await capture("Restart battlegroup", () =>
      managerRequest<BattleGroupDetail>(
        `/api/battlegroups/${encodeURIComponent(selectedBattleGroup.namespace)}/${encodeURIComponent(
          selectedBattleGroup.name
        )}/restart`,
        { method: "POST" }
      )
    );
    await refresh();
    setBusy(false);
  }

  async function exportLiveConfig() {
    if (!selectedBattleGroup) return;
    setBusy(true);
    const snapshot = await capture("Export live config", () =>
      managerRequest<Record<string, unknown>>(
        `/api/battlegroups/${encodeURIComponent(selectedBattleGroup.namespace)}/${encodeURIComponent(
          selectedBattleGroup.name
        )}/raw`
      )
    );
    if (snapshot) {
      const blob = new Blob([JSON.stringify(snapshot, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `${selectedBattleGroup.name}-live.json`;
      anchor.click();
      URL.revokeObjectURL(url);
      setSnapshotPath(`${selectedBattleGroup.name}-live.json`);
    }
    setBusy(false);
  }

  async function saveConfig(nextConfig = config) {
    setBusy(true);
    const saved = await capture("Save config", () => invoke<AppConfig>("save_app_config", { config: nextConfig }));
    if (saved) {
      setConfig(saved);
      setConfigSaved(true);
      window.setTimeout(() => setConfigSaved(false), 2200);
    }
    setBusy(false);
  }

  async function detectEnvironment() {
    setBusy(true);
    const detected = await capture("Detect environment", () => invoke<AppConfig>("detect_app_config"));
    if (detected) {
      setConfig(detected);
    }
    setBusy(false);
  }

  async function installManagerApi() {
    const namespace = managerInstallNamespace;
    const token = config.managerApiToken || generateToken();
    const nextConfig = {
      ...config,
      managerApiNamespace: namespace,
      managerApiToken: token
    };

    setBusy(true);
    setConfig(nextConfig);
    const result = await capture("Install Manager API", () =>
      invoke<ManagerApiInstallResult>("install_manager_api", {
        namespace,
        binaryPath: nextConfig.managerApiBinaryPath,
        token,
        directorBaseUrl: nextConfig.managerApiDirectorUrl,
        installPath: nextConfig.installPath,
        ip: guest?.ip ?? nextConfig.vmIp,
        sshUser: nextConfig.sshUser
      })
    );
    if (result) {
      setManagerInstall(result);
      const savedConfig = { ...nextConfig, managerApiUrl: result.url };
      setConfig(savedConfig);
      await capture("Save Manager API config", () => invoke<AppConfig>("save_app_config", { config: savedConfig }));
    }
    setBusy(false);
  }

  useEffect(() => {
    void (async () => {
      const loaded = await capture("Detect environment", () => invoke<AppConfig>("detect_app_config"));
      if (loaded) {
        setConfig(loaded);
      }
      setConfigLoaded(true);
    })();
  }, []);

  useEffect(() => {
    if (configLoaded) {
      void refresh();
    }
  }, [configLoaded, config.vmName, config.installPath, config.sshUser]);

  useEffect(() => {
    const baseUrl = config.managerApiUrl.trim().replace(/\/$/, "");
    if (!configLoaded || !baseUrl) {
      setManagerStatus(null);
      setManagerTelemetry(null);
      setManagerSocketState("disabled");
      setManagerError("");
      return;
    }

    let closed = false;
    const headers: HeadersInit = config.managerApiToken
      ? { Authorization: `Bearer ${config.managerApiToken}` }
      : {};

    async function loadManagerStatus() {
      try {
        const response = await fetch(`${baseUrl}/api/status`, { headers });
        if (!response.ok) throw new Error(`Manager API returned ${response.status}`);
        const nextStatus = (await response.json()) as ManagerApiStatus;
        if (!closed) {
          setManagerStatus(nextStatus);
          setManagerError("");
        }
      } catch (error) {
        if (!closed) {
          setManagerStatus(null);
          setManagerError(String(error));
        }
      }
    }

    void loadManagerStatus();
    setManagerSocketState("connecting");
    const websocketUrl = `${baseUrl.replace(/^http/i, "ws")}/api/telemetry${
      config.managerApiToken ? `?token=${encodeURIComponent(config.managerApiToken)}` : ""
    }`;
    const socket = new WebSocket(websocketUrl);

    socket.onopen = () => {
      if (!closed) setManagerSocketState("connected");
    };
    socket.onmessage = (event) => {
      if (closed) return;
      try {
        const envelope = JSON.parse(event.data) as TelemetryEnvelope;
        setManagerTelemetry(envelope);
        setManagerError("");
      } catch {
        setManagerError("Manager API sent an unreadable telemetry event");
      }
    };
    socket.onerror = () => {
      if (!closed) setManagerSocketState("error");
    };
    socket.onclose = () => {
      if (!closed) setManagerSocketState("error");
    };

    return () => {
      closed = true;
      socket.close();
    };
  }, [configLoaded, config.managerApiUrl, config.managerApiToken]);

  useEffect(() => {
    if (activeViewRequiresManager && !managerToolsInstalled) {
      setActiveView("manager");
    } else if (activeViewRequiresDirector && !directorAvailable) {
      setActiveView("manager");
    }
  }, [activeViewRequiresDirector, activeViewRequiresManager, directorAvailable, managerToolsInstalled]);

  useEffect(() => {
    if (!directorFlsConfig) return;
    setFlsDraft({
      heartbeatSeconds: numberAt(directorFlsConfig, ["config", "flsServerHeartbeatUpdateFrequencySeconds"]),
      settingsSeconds: numberAt(directorFlsConfig, ["config", "flsServerSettingsUpdateFrequencySeconds"])
    });
  }, [directorFlsConfig]);

  useEffect(() => {
    if (!directorTransferConfig) return;
    setTransferDraft({
      deleteOrigin: boolAt(directorTransferConfig, ["config", "shouldDeleteOriginCharactersDuringTransfers"], true),
      incoming: numberAt(directorTransferConfig, ["config", "incomingCharacterTransfers"], "0"),
      outgoing: boolAt(directorTransferConfig, ["config", "acceptOutgoingCharacterTransfers"]),
      exportTimeout: numberAt(directorTransferConfig, ["config", "exportCharacterTimeout"]),
      importTimeout: numberAt(directorTransferConfig, ["config", "importCharacterTimeout"]),
      freeFrom: boolAt(directorTransferConfig, ["config", "freeToTransferCharactersFrom"]),
      freeTo: boolAt(directorTransferConfig, ["config", "freeToTransferCharactersTo"]),
      validateTimeout: numberAt(directorTransferConfig, ["config", "validateBeforeImportCharacterTimeout"]),
      worldClosed: boolAt(directorTransferConfig, ["config", "forceIsWorldClosed"]),
      worldClosingSoon: boolAt(directorTransferConfig, ["config", "forceIsWorldClosingSoon"])
    });
  }, [directorTransferConfig]);

  useEffect(() => {
    if (!selectedDirectorMap && directorMaps.length > 0) {
      setSelectedDirectorMap(directorMaps[0].name);
    }
  }, [directorMaps, selectedDirectorMap]);

  const pods = workloads?.pods.items ?? [];
  const services = workloads?.services.items ?? [];

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <Server size={26} />
          <div>
            <strong>Dune Dedicated</strong>
            <span>Server Manager</span>
          </div>
        </div>
        <nav>
          {navItems.map((item) => {
            const Icon = item.icon;
            return (
              <button
                key={item.key}
                className={`${activeView === item.key ? "active" : ""} ${item.disabled ? "disabled" : ""}`}
                disabled={item.disabled}
                onClick={() => setActiveView(item.key)}
              >
                <Icon size={16} />
                <span>{item.label}</span>
              </button>
            );
          })}
        </nav>
      </aside>

      <section className="content">
        <header className="topbar">
          <div>
            <h1>{pageTitle}</h1>
            <p>{pageSubtitle}</p>
          </div>
          <button className="primary" onClick={refresh} disabled={busy}>
            <RefreshCw size={17} />
            Refresh
          </button>
        </header>

        <section className="status-strip">
          <div>
            <ShieldCheck size={18} />
            <span>Admin</span>
            <StatusLamp label="Admin" value={host?.isElevated ?? false} />
          </div>
          <div>
            <HardDrive size={18} />
            <span>VM</span>
            <StatusLamp label="VM" value={vm?.state} />
          </div>
          <div>
            <Terminal size={18} />
            <span>SSH</span>
            <StatusLamp label="SSH" value={guest?.connected ?? false} />
          </div>
          <div>
            <Database size={18} />
            <span>k3s</span>
            <StatusLamp label="k3s" value={guest?.kubectl ?? false} />
          </div>
          <div>
            <Activity size={18} />
            <span>BattleGroup</span>
            <StatusLamp label="BattleGroup" value={selectedBattleGroup?.phase} />
          </div>
          <div>
            <RadioTower size={18} />
            <span>Manager API</span>
            <StatusLamp label="Manager API" value={managerReadiness} />
          </div>
        </section>

        {(activeView === "overview" || activeView === "config") && (
          <section className="settings-band">
            <div className="panel-title">
              <h2>Detected Environment</h2>
              <button onClick={detectEnvironment} disabled={busy}>
                <RefreshCw size={16} />
                Detect
              </button>
            </div>
            <div className="detected-grid">
              <InfoRow label="Server install path" value={config.installPath || "Not found"} />
              <InfoRow label="VM name" value={config.vmName || "Not found"} />
              <InfoRow label="VM IP" value={config.vmIp || vm?.ipAddresses?.[0] || "Not found"} />
              <InfoRow label="SSH user" value={config.sshUser || "Not found"} />
              <InfoRow label="SSH path" value={config.sshPath || "Not found"} />
              <InfoRow label="Manager API URL" value={config.managerApiUrl || "Not installed"} />
              <InfoRow label="Manager namespace" value={managerInstallNamespace || "Not detected"} />
              <InfoRow label="Manager binary" value={config.managerApiBinaryPath || "Not found"} />
              <InfoRow label="Director internal URL" value={config.managerApiDirectorUrl || "Not detected"} />
              <InfoRow label="Manager token" value={config.managerApiToken ? "Stored" : "Will be generated on install"} />
            </div>
            {configSaved && <p className="success-line">Saved to app config.json</p>}
          </section>
        )}

        {errors.length > 0 && (
          <section className="error-list">
            {errors.map((error, index) => (
              <div key={`${error.message}-${index}`}>
                <strong>{error.message}</strong>
                {error.stderr && <pre>{error.stderr}</pre>}
              </div>
            ))}
          </section>
        )}

        {vm && !vmIsRunning && (
          <section className="tool-required panel">
            <div>
              <HardDrive size={24} />
              <h2>{vmIsChanging ? `VM is ${vm.state}` : "VM must be running"}</h2>
            </div>
            <p>
              {vmIsStarting
                ? "Hyper-V is starting the VM. Guest SSH, Manager API, BattleGroups, Director telemetry, and logs will load once the VM reports Running and has an IP address."
                : "Guest SSH, Manager API, BattleGroups, Director telemetry, and logs are skipped until Hyper-V reports the VM state as Running and an IP address is available."}
            </p>
            <button onClick={startVm} disabled={busy || !canControlVm || vmIsRunning || vmIsChanging}>
              <Play size={16} />
              Start VM
            </button>
          </section>
        )}

        {(activeView === "overview" || activeView === "host") && (
          <section className="grid two">
            <article className="panel">
              <div className="panel-title">
                <h2>Host & VM</h2>
                <div className="button-row">
                  <button
                    onClick={startVm}
                    disabled={busy || !canControlVm || vmIsRunning || vmIsChanging}
                    title={startVmDisabledReason}
                  >
                    <Play size={16} />
                    Start VM
                  </button>
                  <button
                    onClick={stopVm}
                    disabled={busy || !canControlVm || !vmIsRunning || vmIsChanging}
                    title={stopVmDisabledReason}
                  >
                    <Square size={16} />
                    Stop VM
                  </button>
                </div>
              </div>
              <InfoRow label="Hyper-V" value={host?.hypervAvailable ? "Available" : "Unavailable"} />
              <InfoRow label="vmms service" value={host?.vmmsStatus} />
              <StatusInfoRow label="VM state" value={vm?.state} />
              <InfoRow label="VM health" value={vm?.status} />
              <InfoRow label="Memory" value={vm ? formatBytes(vm.memoryAssignedBytes) : null} />
              <InfoRow label="Uptime" value={vm?.uptime} />
              <InfoRow label="VM path" value={vm?.path} />
            </article>

            <article className="panel">
              <div className="panel-title">
                <h2>Guest Connection</h2>
                <Wifi size={19} />
              </div>
              <InfoRow label="IP" value={guest?.ip ?? vm?.ipAddresses?.[0]} />
              <InfoRow label="SSH user" value={guest?.sshUser} />
              <InfoRow label="Hostname" value={guest?.hostname} />
              <InfoRow label="Kernel" value={guest?.kernel} />
              <InfoRow label="Passwordless sudo" value={guest?.sudo ? "Ready" : "Unavailable"} />
              <InfoRow label="kubectl" value={guest?.kubectl ? "Ready" : "Unavailable"} />
            </article>
          </section>
        )}

        {(activeView === "overview" || activeView === "manager") && (
          <section className="panel">
            <div className="panel-title">
              <h2>Manager API</h2>
              <div className="button-row">
                <button onClick={installManagerApi} disabled={busy || !canInstallManagerApi}>
                  <PackagePlus size={16} />
                  Install Tool
                </button>
                <RadioTower size={19} />
              </div>
            </div>
            <section className="config-summary">
              <InfoRow label="URL" value={config.managerApiUrl || "Not configured"} />
              <InfoRow label="Install namespace" value={managerInstallNamespace || "Not configured"} />
              <InfoRow label="Binary" value={config.managerApiBinaryPath || "Not configured"} />
              <InfoRow label="API" value={managerReadiness} />
              <InfoRow label="Telemetry socket" value={managerTelemetryState} />
              <InfoRow label="Namespace" value={managerStatus?.namespace} />
              <InfoRow label="Director bridge" value={managerStatus?.directorConfigured ? "Configured" : "Unavailable"} />
              <InfoRow
                label="Telemetry"
                value={
                  managerTelemetry?.payload
                    ? `${managerTelemetry.payload.pods?.length ?? 0} pods, ${
                        managerTelemetry.payload.services?.length ?? 0
                      } services`
                    : "No events yet"
                }
              />
              <InfoRow
                label="Snapshot counts"
                value={
                  managerStatus
                    ? `${managerStatus.battlegroups} battlegroups, ${managerStatus.pods} pods, ${managerStatus.services} services`
                    : "Unknown"
                }
              />
            </section>
            {managerInstall && (
              <p className="success-line">
                Installed {managerInstall.deployment} in {managerInstall.namespace}
              </p>
            )}
            {managerError && <p className="subtle-line">{managerError}</p>}
          </section>
        )}

        {!managerToolsInstalled && (activeView === "overview" || activeView === "manager" || activeViewRequiresManager) && (
          <section className="tool-required panel">
            <div>
              <RadioTower size={24} />
              <h2>Manager tools must be installed</h2>
            </div>
            <p>
              BattleGroups, live config, pods, services, logs, and server actions are hidden until the Manager API is
              installed and reachable.
            </p>
            <button onClick={installManagerApi} disabled={busy || !canInstallManagerApi}>
              <PackagePlus size={16} />
              Install Tool
            </button>
          </section>
        )}

        {managerToolsInstalled && !directorAvailable && (activeView === "overview" || activeView === "manager") && (
          <section className="tool-required panel">
            <div>
              <Map size={24} />
              <h2>Director bridge is unavailable</h2>
            </div>
            <p>
              Native player telemetry, map runtime state, and the advanced Director console need the Manager API to
              detect and reach the internal Director service.
            </p>
            <button onClick={refresh} disabled={busy}>
              <RefreshCw size={16} />
              Refresh
            </button>
          </section>
        )}

        {directorAvailable && (activeView === "overview" || activeView === "players") && (
          <section className="panel">
            <div className="panel-title">
              <h2>Players</h2>
              <Users size={19} />
            </div>
            {!directorPlayers ? (
              <EmptyState text="No Director player telemetry loaded." />
            ) : (
              <div className="metric-grid">
                <Metric label="Active" value={directorPlayers.active} />
                <Metric label="Online" value={directorPlayers.online} />
                <Metric label="In Transit" value={directorPlayers.inTransit} />
                <Metric label="Grace Period" value={directorPlayers.gracePeriod} />
                <Metric label="Completion" value={directorPlayers.completion} />
                <Metric label="Queued" value={directorPlayers.queued} />
                <Metric label="Login Requests" value={directorPlayers.loginRequestsTotal} />
                <Metric label="Travel Requests" value={directorPlayers.travelRequestsTotal} />
              </div>
            )}
          </section>
        )}

        {managerToolsInstalled && (activeView === "overview" || activeView === "battlegroups") && (
            <section className="panel">
              <div className="panel-title">
                <h2>BattleGroups</h2>
                <div className="button-row">
                  <button
                    onClick={() => setBattleGroupRunning(true)}
                    disabled={busy || !selectedBattleGroup || !canUseManager || !battleGroupIsStopped}
                  >
                    <Play size={16} />
                    Start
                  </button>
                  <button
                    onClick={() => setBattleGroupRunning(false)}
                    disabled={busy || !selectedBattleGroup || !canUseManager || battleGroupIsStopped}
                  >
                    <Square size={16} />
                    Stop
                  </button>
                  <button
                    onClick={restartBattleGroup}
                    disabled={busy || !selectedBattleGroup || !canUseManager || !battleGroupIsRunning}
                  >
                    <RotateCcw size={16} />
                    Restart
                  </button>
                  <button onClick={exportLiveConfig} disabled={busy || !selectedBattleGroup || !canUseManager}>
                    <Download size={16} />
                    Export
                  </button>
                </div>
              </div>
              {battleGroups.length === 0 ? (
                <EmptyState text="No BattleGroups were found." />
              ) : (
                <div className="table-wrap">
                  <table>
                    <thead>
                      <tr>
                        <th>Title</th>
                        <th>Phase</th>
                        <th>Server Sets</th>
                        <th>Image</th>
                        <th>Services</th>
                      </tr>
                    </thead>
                    <tbody>
                      {battleGroups.map((group) => (
                        <tr
                          key={group.namespace}
                          className={group.namespace === selectedBattleGroup?.namespace ? "selected" : ""}
                          onClick={() => {
                            setSelectedNamespace(group.namespace);
                            void loadBattleGroupDetail(group);
                            void loadWorkloads(group.namespace);
                          }}
                        >
                          <td>
                            <strong>{group.title || group.name}</strong>
                            <span>{group.namespace}</span>
                          </td>
                          <td>
                            <StatusPill value={group.phase} />
                          </td>
                          <td>{group.serverSets}</td>
                          <td className="mono">{group.serverImage}</td>
                          <td>
                            <div className="link-row">
                              {group.fileBrowserUrl && (
                                <a href={group.fileBrowserUrl} target="_blank" rel="noreferrer">
                                  Files <ExternalLink size={14} />
                                </a>
                              )}
                              {group.directorUrl && (
                                <a href={group.directorUrl} target="_blank" rel="noreferrer">
                                  Director <ExternalLink size={14} />
                                </a>
                              )}
                            </div>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
              {snapshotPath && <p className="success-line">Snapshot exported to {snapshotPath}</p>}
            </section>
        )}

        {managerToolsInstalled && activeView === "config" && (
          <>
            <section className="panel">
              <div className="panel-title">
                <h2>Live Config</h2>
                <SlidersHorizontal size={19} />
              </div>
              {!battleGroupDetail ? (
                <EmptyState text="No live BattleGroup detail loaded." />
              ) : (
                <>
                  <section className="config-summary">
                    <InfoRow label="Title" value={battleGroupDetail.title} />
                    <InfoRow label="Database" value={battleGroupDetail.databasePhase || "Unknown"} />
                    <InfoRow
                      label="Server group"
                      value={battleGroupDetail.serverGroupPhase || battleGroupDetail.phase}
                    />
                    <InfoRow label="Gateway" value={battleGroupDetail.gatewayPhase || "Unknown"} />
                    <InfoRow label="Director" value={battleGroupDetail.directorPhase || "Unknown"} />
                    <InfoRow label="Stop flag" value={battleGroupDetail.stop ? "true" : "false"} />
                  </section>
                  <div className="image-list">
                    <strong>Images</strong>
                    {[battleGroupDetail.serverImage, ...battleGroupDetail.utilityImages]
                      .filter(Boolean)
                      .map((image) => (
                        <span className="mono chip" key={image}>
                          {image}
                        </span>
                      ))}
                  </div>
                  <div className="table-wrap">
                    <table>
                      <thead>
                        <tr>
                          <th>Map</th>
                          <th>Replicas</th>
                          <th>Memory</th>
                          <th>Scaling</th>
                        </tr>
                      </thead>
                      <tbody>
                        {battleGroupDetail.serverSets.map((set) => (
                          <tr key={set.map}>
                            <td>
                              <strong>{set.map}</strong>
                            </td>
                            <td>{set.replicas}</td>
                            <td>{set.memoryLimit || "Unset"}</td>
                            <td>{set.dedicatedScaling ? "Dedicated" : "Fixed"}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </>
              )}
            </section>
            {directorAvailable && (
              <section className="panel">
                <div className="panel-title">
                  <h2>Director Config</h2>
                  <Map size={19} />
                </div>
                <section className="native-config-grid">
                  <div className="native-config-box">
                    <div className="mini-title">
                      <strong>FLS Report Settings</strong>
                      <span>{directorFlsConfig?.webOverrideConfig ? "Override active" : "Base config"}</span>
                    </div>
                    <label>
                      Heartbeat update seconds
                      <input
                        type="number"
                        min="1"
                        value={flsDraft.heartbeatSeconds}
                        onChange={(event) => setFlsDraft({ ...flsDraft, heartbeatSeconds: event.target.value })}
                      />
                    </label>
                    <label>
                      Settings update seconds
                      <input
                        type="number"
                        min="1"
                        value={flsDraft.settingsSeconds}
                        onChange={(event) => setFlsDraft({ ...flsDraft, settingsSeconds: event.target.value })}
                      />
                    </label>
                    <div className="button-row">
                      <button onClick={saveFlsConfig} disabled={busy || !flsDraft.heartbeatSeconds || !flsDraft.settingsSeconds}>
                        Update
                      </button>
                      <button onClick={clearFlsConfig} disabled={busy}>
                        Clear Override
                      </button>
                    </div>
                  </div>

                  <div className="native-config-box">
                    <div className="mini-title">
                      <strong>Character Transfer</strong>
                      <span>{directorTransferConfig?.webOverrideConfig ? "Override active" : "Base config"}</span>
                    </div>
                    <div className="form-grid">
                      <label>
                        Incoming
                        <select
                          value={transferDraft.incoming}
                          onChange={(event) => setTransferDraft({ ...transferDraft, incoming: event.target.value })}
                        >
                          <option value="0">Default</option>
                          <option value="10">Deny all incoming</option>
                          <option value="20">Accept private</option>
                          <option value="30">Accept official</option>
                          <option value="40">Accept all</option>
                        </select>
                      </label>
                      <label>
                        Export timeout
                        <input
                          type="number"
                          min="1"
                          value={transferDraft.exportTimeout}
                          onChange={(event) => setTransferDraft({ ...transferDraft, exportTimeout: event.target.value })}
                        />
                      </label>
                      <label>
                        Import timeout
                        <input
                          type="number"
                          min="1"
                          value={transferDraft.importTimeout}
                          onChange={(event) => setTransferDraft({ ...transferDraft, importTimeout: event.target.value })}
                        />
                      </label>
                      <label>
                        Validate timeout
                        <input
                          type="number"
                          min="1"
                          value={transferDraft.validateTimeout}
                          onChange={(event) => setTransferDraft({ ...transferDraft, validateTimeout: event.target.value })}
                        />
                      </label>
                    </div>
                    <div className="toggle-grid">
                      <label><input type="checkbox" checked={transferDraft.deleteOrigin} onChange={(event) => setTransferDraft({ ...transferDraft, deleteOrigin: event.target.checked })} /> Delete origin</label>
                      <label><input type="checkbox" checked={transferDraft.outgoing} onChange={(event) => setTransferDraft({ ...transferDraft, outgoing: event.target.checked })} /> Outgoing</label>
                      <label><input type="checkbox" checked={transferDraft.freeFrom} onChange={(event) => setTransferDraft({ ...transferDraft, freeFrom: event.target.checked })} /> Free from</label>
                      <label><input type="checkbox" checked={transferDraft.freeTo} onChange={(event) => setTransferDraft({ ...transferDraft, freeTo: event.target.checked })} /> Free to</label>
                      <label><input type="checkbox" checked={transferDraft.worldClosed} onChange={(event) => setTransferDraft({ ...transferDraft, worldClosed: event.target.checked })} /> World closed</label>
                      <label><input type="checkbox" checked={transferDraft.worldClosingSoon} onChange={(event) => setTransferDraft({ ...transferDraft, worldClosingSoon: event.target.checked })} /> Closing soon</label>
                    </div>
                    <div className="button-row">
                      <button onClick={saveTransferConfig} disabled={busy || !transferDraft.exportTimeout || !transferDraft.importTimeout || !transferDraft.validateTimeout}>
                        Update
                      </button>
                      <button onClick={clearTransferConfig} disabled={busy}>
                        Clear Override
                      </button>
                    </div>
                  </div>
                </section>
              </section>
            )}
            {directorAvailable && selectedDirectorMapSummary && (
              <section className="panel">
                <div className="panel-title">
                  <h2>Map Override</h2>
                  <SlidersHorizontal size={19} />
                </div>
                <section className="native-config-box">
                  <div className="form-grid">
                    <label>
                      Map
                      <select
                        value={selectedDirectorMapSummary.name}
                        onChange={(event) => setSelectedDirectorMap(event.target.value)}
                      >
                        {directorMaps.map((map) => (
                          <option value={map.name} key={`${map.kind}-option-${map.name}`}>
                            {map.name} ({map.kind})
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      Player hard cap
                      <input
                        type="number"
                        min="1"
                        value={mapOverrideDraft.playerHardCap}
                        onChange={(event) =>
                          setMapOverrideDraft({ ...mapOverrideDraft, playerHardCap: event.target.value })
                        }
                        placeholder="leave empty for null"
                      />
                    </label>
                    {selectedDirectorMapSummary.kind === "Instanced" && (
                      <>
                        <label>
                          Scaling throttle
                          <input
                            type="number"
                            min="0"
                            value={mapOverrideDraft.throttlingSeconds}
                            onChange={(event) =>
                              setMapOverrideDraft({ ...mapOverrideDraft, throttlingSeconds: event.target.value })
                            }
                            placeholder="seconds"
                          />
                        </label>
                        <label>
                          Min servers
                          <input
                            type="number"
                            min="0"
                            value={mapOverrideDraft.minServers}
                            onChange={(event) =>
                              setMapOverrideDraft({ ...mapOverrideDraft, minServers: event.target.value })
                            }
                          />
                        </label>
                        <label>
                          Extra servers
                          <input
                            type="number"
                            min="0"
                            value={mapOverrideDraft.extraServers}
                            onChange={(event) =>
                              setMapOverrideDraft({ ...mapOverrideDraft, extraServers: event.target.value })
                            }
                          />
                        </label>
                      </>
                    )}
                  </div>
                  <div className="toggle-grid">
                    <label>
                      <input
                        type="checkbox"
                        checked={mapOverrideDraft.updatePlayerCountOnFls}
                        onChange={(event) =>
                          setMapOverrideDraft({ ...mapOverrideDraft, updatePlayerCountOnFls: event.target.checked })
                        }
                      />
                      Update player count on FLS
                    </label>
                    {selectedDirectorMapSummary.kind === "Dimension" && (
                      <label>
                        <input
                          type="checkbox"
                          checked={mapOverrideDraft.enforceSameHomeDimension}
                          onChange={(event) =>
                            setMapOverrideDraft({ ...mapOverrideDraft, enforceSameHomeDimension: event.target.checked })
                          }
                        />
                        Enforce same home dimension
                      </label>
                    )}
                    {selectedDirectorMapSummary.kind === "Instanced" && (
                      <label>
                        <input
                          type="checkbox"
                          checked={mapOverrideDraft.automaticScaling}
                          onChange={(event) =>
                            setMapOverrideDraft({ ...mapOverrideDraft, automaticScaling: event.target.checked })
                          }
                        />
                        Automatic scaling
                      </label>
                    )}
                  </div>
                  <div className="button-row">
                    <button onClick={saveMapOverride} disabled={busy}>
                      Update Override
                    </button>
                    <button
                      onClick={() => clearMapOverride(selectedDirectorMapSummary.name)}
                      disabled={busy || !selectedDirectorMapSummary.hasOverride}
                    >
                      Clear Override
                    </button>
                  </div>
                </section>
              </section>
            )}
          </>
        )}

        {directorAvailable && activeView === "director" && (
          <>
            <section className="panel">
              <div className="panel-title">
                <h2>Director Maps</h2>
                <div className="button-row">
                  <button onClick={loadDirectorData} disabled={busy}>
                    <RefreshCw size={16} />
                    Reload
                  </button>
                  <Map size={19} />
                </div>
              </div>
              {directorMaps.length === 0 ? (
                <EmptyState text="No Director map data loaded." />
              ) : (
                <div className="table-wrap">
                  <table>
                    <thead>
                      <tr>
                        <th>Map</th>
                        <th>Kind</th>
                        <th>Players</th>
                        <th>Queue</th>
                        <th>Servers</th>
                        <th>Override</th>
                      </tr>
                    </thead>
                    <tbody>
                      {directorMaps.map((map) => (
                        <tr key={`${map.kind}-${map.name}`}>
                          <td>
                            <strong>{map.name}</strong>
                          </td>
                          <td>{map.kind}</td>
                          <td>{map.players}</td>
                          <td>{map.queued}</td>
                          <td>{map.servers.length}</td>
                          <td>
                            <div className="button-row">
                              <StatusPill value={map.hasOverride ? "Active" : "None"} />
                              <button
                                onClick={() => {
                                  setSelectedDirectorMap(map.name);
                                  setActiveView("config");
                                }}
                                disabled={busy}
                              >
                                Edit
                              </button>
                              <button onClick={() => clearMapOverride(map.name)} disabled={busy || !map.hasOverride}>
                                Clear
                              </button>
                            </div>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </section>

            {directorMaps.length > 0 && (
              <section className="panel">
                <div className="panel-title">
                  <h2>Server Runtime</h2>
                  <Activity size={19} />
                </div>
                <div className="map-card-grid">
                  {directorMaps.map((map) => (
                    <article className="runtime-map" key={`${map.kind}-runtime-${map.name}`}>
                      <div className="mini-title">
                        <strong>{map.name}</strong>
                        <span>{map.kind}</span>
                      </div>
                      <div className="runtime-stats">
                        <span>{map.players} players</span>
                        <span>{map.online} online</span>
                        <span>{map.queued} queued</span>
                      </div>
                      <div className="runtime-servers">
                        {map.servers.length === 0 ? (
                          <EmptyState text="No server rows reported." />
                        ) : (
                          map.servers.map((server) => (
                            <div key={`${map.name}-${server.partitionId}-${server.dimensionIndex}-${server.serverId}`}>
                              <div>
                                <strong>{server.label || "Unnamed"}</strong>
                                <span className="mono">{server.serverId || "No server id"}</span>
                              </div>
                              <StatusPill value={server.status} />
                              <span>{server.players} players</span>
                              <span>{server.queued ?? "N/A"} queued</span>
                              <span>
                                {server.heartbeatSecondsAgo === null || server.heartbeatSecondsAgo === undefined
                                  ? "No heartbeat"
                                  : `${server.heartbeatSecondsAgo}s ago`}
                              </span>
                            </div>
                          ))
                        )}
                      </div>
                    </article>
                  ))}
                </div>
              </section>
            )}
          </>
        )}

        {managerToolsInstalled && (activeView === "overview" || activeView === "workloads") && (
            <section className="grid two">
              <article className="panel">
                <div className="panel-title">
                  <h2>Pods</h2>
                  <span>{pods.length}</span>
                </div>
                {pods.length === 0 ? (
                  <EmptyState text="No pod data loaded." />
                ) : (
                  <div className="compact-list">
                    {pods.map((pod) => {
                      const status = String(pod.status?.phase ?? "Unknown");
                      return (
                        <div key={pod.metadata?.name}>
                          <strong>{pod.metadata?.name}</strong>
                          <StatusPill value={status} />
                        </div>
                      );
                    })}
                  </div>
                )}
              </article>

              <article className="panel">
                <div className="panel-title">
                  <h2>Services</h2>
                  <span>{services.length}</span>
                </div>
                {services.length === 0 ? (
                  <EmptyState text="No service data loaded." />
                ) : (
                  <div className="compact-list">
                    {services.map((service) => {
                      const ports = Array.isArray(service.spec?.ports)
                        ? service.spec?.ports
                            .map((port: Record<string, unknown>) =>
                              port.nodePort ? `${port.port}:${port.nodePort}` : String(port.port)
                            )
                            .join(", ")
                        : "";
                      return (
                        <div key={service.metadata?.name}>
                          <strong>{service.metadata?.name}</strong>
                          <span>{ports}</span>
                        </div>
                      );
                    })}
                  </div>
                )}
              </article>
            </section>
        )}

        {managerToolsInstalled && activeView === "logs" && (
          <section className="panel">
            <div className="panel-title">
              <h2>Logs</h2>
              <Terminal size={19} />
            </div>
            <EmptyState text="Log export and streaming will live here once the manager log endpoints are wired." />
          </section>
        )}
      </section>
    </main>
  );
}
