import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  Database,
  Download,
  ExternalLink,
  HardDrive,
  RadioTower,
  Play,
  RefreshCw,
  RotateCcw,
  Server,
  ShieldCheck,
  SlidersHorizontal,
  Square,
  Terminal,
  Wifi
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

const defaultConfig: AppConfig = {
  installPath: "",
  vmName: "",
  vmIp: "",
  sshUser: "",
  sshPath: "",
  managerApiUrl: "",
  managerApiToken: ""
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
  if (value === true || ["running", "ready", "operating normally", "active"].includes(text)) {
    return "good";
  }
  if (value === false || ["stopped", "suspended", "error", "failed"].includes(text)) {
    return "bad";
  }
  return "warn";
}

function StatusPill({ value }: { value?: string | boolean | null }) {
  const label = typeof value === "boolean" ? (value ? "Yes" : "No") : value || "Unknown";
  return <span className={`pill ${statusTone(value)}`}>{label}</span>;
}

function InfoRow({ label, value }: { label: string; value?: string | number | null }) {
  return (
    <div className="info-row">
      <span>{label}</span>
      <strong>{value || "Unknown"}</strong>
    </div>
  );
}

function EmptyState({ text }: { text: string }) {
  return <div className="empty-state">{text}</div>;
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

  const selectedBattleGroup = useMemo(
    () => battleGroups.find((group) => group.namespace === selectedNamespace) ?? battleGroups[0],
    [battleGroups, selectedNamespace]
  );
  const vmState = vm?.state.toLowerCase() ?? "";
  const vmIsRunning = vmState === "running";
  const vmIsChanging = ["starting", "stopping", "pausing", "resuming", "resetting", "saving"].includes(vmState);
  const canControlVm = Boolean(host?.isElevated && host?.hypervAvailable && vm);
  const battleGroupIsStopped =
    selectedBattleGroup?.stop === true || selectedBattleGroup?.phase.toLowerCase() === "stopped";
  const battleGroupIsRunning =
    selectedBattleGroup?.stop === false &&
    ["running", "ready", "starting"].includes(selectedBattleGroup?.phase.toLowerCase() ?? "");
  const canUseGuest = vmIsRunning && guest?.connected && guest?.kubectl;
  const managerApiConfigured = config.managerApiUrl.trim().length > 0;

  async function capture<T>(label: string, fn: () => Promise<T>): Promise<T | null> {
    try {
      return await fn();
    } catch (error) {
      const commandError = asError(error);
      setErrors((current) => [{ ...commandError, message: `${label}: ${commandError.message}` }, ...current]);
      return null;
    }
  }

  async function refresh() {
    setBusy(true);
    setErrors([]);
    setSnapshotPath("");
    const nextHost = await capture("Host status", () => invoke<HostStatus>("get_host_status"));
    setHost(nextHost);

    const nextVm = await capture("VM status", () => invoke<VmStatus>("get_vm_status", { vmName: config.vmName }));
    setVm(nextVm);

    const ip = nextVm?.ipAddresses?.[0] ?? guest?.ip ?? config.vmIp;
    const nextGuest = await capture("Guest connection", () =>
      invoke<GuestConnection>("connect_guest", { installPath: config.installPath, ip, sshUser: config.sshUser })
    );
    setGuest(nextGuest);
    if (nextGuest?.ip && nextGuest.ip !== config.vmIp) {
      const updatedConfig = { ...config, vmIp: nextGuest.ip };
      setConfig(updatedConfig);
      void invoke<AppConfig>("save_app_config", { config: updatedConfig });
    }

    const nextBattleGroups = await capture("BattleGroups", () =>
      invoke<BattleGroupSummary[]>("get_battlegroups", {
        installPath: config.installPath,
        ip: nextGuest?.ip ?? ip,
        sshUser: config.sshUser
      })
    );
    if (nextBattleGroups) {
      setBattleGroups(nextBattleGroups);
      const nextSelected = nextBattleGroups.some((group) => group.namespace === selectedNamespace)
        ? selectedNamespace
        : nextBattleGroups[0]?.namespace ?? "";
      setSelectedNamespace(nextSelected);
      const group = nextBattleGroups.find((candidate) => candidate.namespace === nextSelected);
      if (group) {
        await Promise.all([
          loadWorkloads(group.namespace, nextGuest?.ip ?? ip),
          loadBattleGroupDetail(group, nextGuest?.ip ?? ip)
        ]);
      }
    }
    setBusy(false);
  }

  async function loadWorkloads(namespace: string, ip?: string) {
    const nextWorkloads = await capture("Workloads", () =>
      invoke<Workloads>("get_workloads", {
        namespace,
        installPath: config.installPath,
        ip: ip ?? guest?.ip ?? config.vmIp,
        sshUser: config.sshUser
      })
    );
    setWorkloads(nextWorkloads);
  }

  async function loadBattleGroupDetail(group: BattleGroupSummary, ip?: string) {
    const detail = await capture("BattleGroup detail", () =>
      invoke<BattleGroupDetail>("get_battlegroup_detail", {
        namespace: group.namespace,
        name: group.name,
        installPath: config.installPath,
        ip: ip ?? guest?.ip ?? config.vmIp,
        sshUser: config.sshUser
      })
    );
    setBattleGroupDetail(detail);
  }

  async function startVm() {
    setBusy(true);
    const nextVm = await capture("Start VM", () => invoke<VmStatus>("start_vm", { vmName: config.vmName }));
    if (nextVm) setVm(nextVm);
    setBusy(false);
  }

  async function stopVm() {
    setBusy(true);
    const nextVm = await capture("Stop VM", () => invoke<VmStatus>("stop_vm", { vmName: config.vmName }));
    if (nextVm) setVm(nextVm);
    setBusy(false);
  }

  async function setBattleGroupRunning(running: boolean) {
    if (!selectedBattleGroup) return;
    setBusy(true);
    await capture(running ? "Start battlegroup" : "Stop battlegroup", () =>
      invoke(running ? "start_battlegroup" : "stop_battlegroup", {
        namespace: selectedBattleGroup.namespace,
        name: selectedBattleGroup.name,
        installPath: config.installPath,
        ip: guest?.ip ?? config.vmIp,
        sshUser: config.sshUser
      })
    );
    await refresh();
    setBusy(false);
  }

  async function restartBattleGroup() {
    if (!selectedBattleGroup) return;
    setBusy(true);
    await capture("Restart battlegroup", () =>
      invoke("restart_battlegroup", {
        namespace: selectedBattleGroup.namespace,
        name: selectedBattleGroup.name,
        installPath: config.installPath,
        ip: guest?.ip ?? config.vmIp,
        sshUser: config.sshUser
      })
    );
    await refresh();
    setBusy(false);
  }

  async function exportLiveConfig() {
    if (!selectedBattleGroup) return;
    setBusy(true);
    const snapshot = await capture("Export live config", () =>
      invoke<{ filePath: string }>("export_live_config", {
        namespace: selectedBattleGroup.namespace,
        name: selectedBattleGroup.name,
        installPath: config.installPath,
        ip: guest?.ip ?? config.vmIp,
        sshUser: config.sshUser
      })
    );
    if (snapshot) setSnapshotPath(snapshot.filePath);
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

  useEffect(() => {
    void (async () => {
      const loaded = await capture("Load config", () => invoke<AppConfig>("get_app_config"));
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
          <a className="active">Overview</a>
          <a>Host & VM</a>
          <a>BattleGroups</a>
          <a>Manager API</a>
          <a>Pods & Services</a>
          <a>Config</a>
          <a>Logs</a>
        </nav>
      </aside>

      <section className="content">
        <header className="topbar">
          <div>
            <h1>{selectedBattleGroup?.title || "Dune Awakening"}</h1>
            <p>{selectedBattleGroup?.name || "No battlegroup detected"}</p>
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
            <StatusPill value={host?.isElevated ?? false} />
          </div>
          <div>
            <HardDrive size={18} />
            <span>VM</span>
            <StatusPill value={vm?.state} />
          </div>
          <div>
            <Terminal size={18} />
            <span>SSH</span>
            <StatusPill value={guest?.connected ?? false} />
          </div>
          <div>
            <Database size={18} />
            <span>k3s</span>
            <StatusPill value={guest?.kubectl ?? false} />
          </div>
          <div>
            <Activity size={18} />
            <span>BattleGroup</span>
            <StatusPill value={selectedBattleGroup?.phase} />
          </div>
          <div>
            <RadioTower size={18} />
            <span>Manager API</span>
            <StatusPill value={managerSocketState === "connected"} />
          </div>
        </section>

        <section className="settings-band">
          <label>
            Server install path
            <input
              value={config.installPath}
              onChange={(event) => setConfig((current) => ({ ...current, installPath: event.target.value }))}
              onBlur={() => void saveConfig()}
            />
          </label>
          <div className="settings-grid">
            <label>
              VM name
              <input
                value={config.vmName}
                onChange={(event) => setConfig((current) => ({ ...current, vmName: event.target.value }))}
                onBlur={() => void saveConfig()}
              />
            </label>
            <label>
              VM IP
              <input
                value={config.vmIp}
                onChange={(event) => setConfig((current) => ({ ...current, vmIp: event.target.value }))}
                onBlur={() => void saveConfig()}
              />
            </label>
            <label>
              SSH user
              <input
                value={config.sshUser}
                onChange={(event) => setConfig((current) => ({ ...current, sshUser: event.target.value }))}
                onBlur={() => void saveConfig()}
              />
            </label>
            <label>
              SSH path
              <input
                value={config.sshPath}
                onChange={(event) => setConfig((current) => ({ ...current, sshPath: event.target.value }))}
                onBlur={() => void saveConfig()}
              />
            </label>
            <label>
              Manager API URL
              <input
                placeholder="http://127.0.0.1:8787"
                value={config.managerApiUrl}
                onChange={(event) => setConfig((current) => ({ ...current, managerApiUrl: event.target.value }))}
                onBlur={() => void saveConfig()}
              />
            </label>
            <label>
              Manager API token
              <input
                type="password"
                value={config.managerApiToken}
                onChange={(event) => setConfig((current) => ({ ...current, managerApiToken: event.target.value }))}
                onBlur={() => void saveConfig()}
              />
            </label>
          </div>
          <div className="settings-actions">
            <button onClick={() => void saveConfig()} disabled={busy}>
              Save config
            </button>
            {configSaved && <span>Saved to app config.json</span>}
          </div>
        </section>

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

        <section className="grid two">
          <article className="panel">
            <div className="panel-title">
              <h2>Host & VM</h2>
              <div className="button-row">
                <button onClick={startVm} disabled={busy || !canControlVm || vmIsRunning || vmIsChanging}>
                  <Play size={16} />
                  Start VM
                </button>
                <button onClick={stopVm} disabled={busy || !canControlVm || !vmIsRunning || vmIsChanging}>
                  <Square size={16} />
                  Stop VM
                </button>
              </div>
            </div>
            <InfoRow label="Hyper-V" value={host?.hypervAvailable ? "Available" : "Unavailable"} />
            <InfoRow label="vmms service" value={host?.vmmsStatus} />
            <InfoRow label="VM status" value={vm?.status} />
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

        <section className="panel">
          <div className="panel-title">
            <h2>Manager API</h2>
            <RadioTower size={19} />
          </div>
          <section className="config-summary">
            <InfoRow label="URL" value={config.managerApiUrl || "Not configured"} />
            <InfoRow label="Socket" value={managerApiConfigured ? managerSocketState : "Disabled"} />
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
          {managerError && <p className="subtle-line">{managerError}</p>}
        </section>

        <section className="panel">
          <div className="panel-title">
            <h2>BattleGroups</h2>
            <div className="button-row">
              <button
                onClick={() => setBattleGroupRunning(true)}
                disabled={busy || !selectedBattleGroup || !canUseGuest || !battleGroupIsStopped}
              >
                <Play size={16} />
                Start
              </button>
              <button
                onClick={() => setBattleGroupRunning(false)}
                disabled={busy || !selectedBattleGroup || !canUseGuest || battleGroupIsStopped}
              >
                <Square size={16} />
                Stop
              </button>
              <button
                onClick={restartBattleGroup}
                disabled={busy || !selectedBattleGroup || !canUseGuest || !battleGroupIsRunning}
              >
                <RotateCcw size={16} />
                Restart
              </button>
              <button onClick={exportLiveConfig} disabled={busy || !selectedBattleGroup || !canUseGuest}>
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
                <InfoRow label="Server group" value={battleGroupDetail.serverGroupPhase || battleGroupDetail.phase} />
                <InfoRow label="Gateway" value={battleGroupDetail.gatewayPhase || "Unknown"} />
                <InfoRow label="Director" value={battleGroupDetail.directorPhase || "Unknown"} />
                <InfoRow label="Stop flag" value={battleGroupDetail.stop ? "true" : "false"} />
              </section>
              <div className="image-list">
                <strong>Images</strong>
                {[battleGroupDetail.serverImage, ...battleGroupDetail.utilityImages].filter(Boolean).map((image) => (
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
      </section>
    </main>
  );
}
