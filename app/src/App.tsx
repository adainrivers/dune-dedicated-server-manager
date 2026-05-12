import {
  Component,
  type ComponentType,
  type ErrorInfo,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import {
  AlertDialog,
  Badge,
  Box,
  Button,
  Card,
  Checkbox,
  Dialog,
  Flex,
  Grid,
  Heading,
  Link,
  Separator,
  Select,
  Switch,
  TabNav,
  Text,
  TextArea,
  TextField,
  Theme,
} from "@radix-ui/themes";
import {
  CubeIcon,
  GlobeIcon,
  LightningBoltIcon,
  MixIcon,
  RocketIcon,
  DesktopIcon,
} from "@radix-ui/react-icons";

const pages = [
  { id: "home", label: "Home" },
  { id: "servers", label: "Servers" },
  { id: "install", label: "Create New Server" },
] as const;

type PageId = (typeof pages)[number]["id"];

type NetworkMode = "static" | "dhcp";
type PlayerIpMode = "local" | "external";
type SetupTarget = "hyperv" | "ubuntu";

type NetworkAdapterCandidate = {
  name: string;
  interfaceDescription: string;
  ipv4Address: string;
  prefixLength: number;
  gateway: string;
  suggestedIpv4Address: string;
  existingExternalSwitch: string;
};

type HostReadiness = {
  elevated: boolean;
  hypervAvailable: boolean;
  vmmsRunning: boolean;
  virtualizationFirmwareEnabled: boolean | null;
  totalPhysicalMemoryBytes: number;
  availablePhysicalMemoryBytes: number;
  logicalProcessorCount: number;
};

type DriveCandidate = {
  name: string;
  root: string;
  freeBytes: number;
};

type EnvironmentDetection = {
  readiness: HostReadiness;
  drives: DriveCandidate[];
  networkAdapters: NetworkAdapterCandidate[];
  externalIp: string | null;
};

type VmPowerState =
  | "missing"
  | "off"
  | "starting"
  | "running"
  | "stopping"
  | "saved"
  | "paused"
  | "other";

type VmInventoryRecord = {
  name: string;
  state: VmPowerState;
  rawState: string;
  configurationLocation: string;
  path: string;
  memoryAssignedBytes: number;
  processorCount: number;
  uptimeSeconds: number;
  ipv4Addresses: string[];
  hardDiskPaths: string[];
  diskSizeBytes: number;
  diskFileSizeBytes: number;
  switchNames: string[];
};

type DuneVmCandidate = {
  vm: VmInventoryRecord;
  confidence: "high" | "medium" | "low";
  reasons: string[];
};

type DetectionState = "detecting" | "ready" | "failed";
type LogLevel = "debug" | "info" | "warn" | "error";
type LogLevelFilter = LogLevel;
type UpdateStatus = "idle" | "checking" | "available" | "current" | "installing" | "relaunching" | "failed";

type LogRow = {
  id: number;
  timestamp: string;
  level: LogLevel;
  scope: string;
  message: string;
};

type AppErrorBoundaryProps = {
  onError: (message: string) => void;
  children: ReactNode;
};

type AppErrorBoundaryState = {
  error: string | null;
};

type EnvironmentGate = {
  canContinue: boolean;
  reasons: string[];
};

type SetupLogPayload = {
  level: LogLevel;
  scope: string;
  message: string;
};

type SetupRunRequest = {
  vmDestination: string;
  vmName: string;
  diskGb: number;
  memoryGb: number;
  processorCount: number;
  enableSwap: boolean;
  networkMode: NetworkMode;
  switchName: string;
  adapterName: string;
  staticIp: string;
  gateway: string;
  dns: string;
  playerIp: string;
  worldName: string;
  region: string;
  selfHostToken: string;
  survivalInstances: number;
  deepDesertPveInstances: number;
  deepDesertPvpInstances: number;
  deepDesertWarmServers: number;
};

type RemoteSetupRunRequest = {
  host: string;
  user: string;
  keyPath: string;
  playerIp: string;
  worldName: string;
  region: string;
  selfHostToken: string;
  survivalInstances: number;
  deepDesertPveInstances: number;
  deepDesertPvpInstances: number;
  deepDesertWarmServers: number;
  enableSwap: boolean;
};

type RemoteSetupRunResult = {
  namespace: string;
  battlegroupName: string;
  worldUniqueName: string;
  managerApiUrl: string;
  preflight: UbuntuSshPreflight;
};

type RemoteServerRecord = {
  id: string;
  name: string;
  host: string;
  user: string;
  keyPath: string;
  namespace: string;
  battlegroupName: string;
  worldUniqueName: string;
  managerApiUrl: string;
  phase: string;
  createdAt: string;
};

type RemoteBattlegroupStatus = {
  stop: boolean;
  phase: string;
  serverGroupPhase: string;
  directorPhase: string;
};

type RemoteManagerApiServiceStatus = {
  installed: boolean;
  running: boolean;
  healthReachable: boolean;
  serviceManager: string;
  rawState: string;
  port: number;
};

type RemoteServerStatus = {
  battlegroup: RemoteBattlegroupStatus;
  managerApi: RemoteManagerApiServiceStatus;
};

type ManagerApiProbe = {
  url: string;
  reachable: boolean;
  ok: boolean;
  apiVersion: string;
  namespace: string;
  authEnabled: boolean;
  directorConfigured: boolean;
  error: string;
};

type RemoteAttachForm = {
  host: string;
  keyPath: string;
};

type UbuntuSshPreflight = {
  hostname: string;
  osPrettyName: string;
  osId: string;
  versionId: string;
  architecture: string;
  kernelRelease: string;
  user: string;
  uid: number;
  passwordlessSudo: boolean;
  systemdAvailable: boolean;
  logicalProcessorCount: number;
  totalMemoryBytes: number;
  availableMemoryBytes: number;
  swapTotalBytes: number;
  rootDiskTotalBytes: number;
  rootDiskAvailableBytes: number;
  publicIp: string | null;
  ipv4Addresses: string[];
  steamcmdInstalled: boolean;
  k3sInstalled: boolean;
  kubectlAvailable: boolean;
};

type RollbackRequest = {
  vmName: string;
  vmDestination: string;
  switchName: string;
};

const log = {
  debug: (scope: string, message: string): LogRow => logEntry("debug", scope, message),
  info: (scope: string, message: string): LogRow => logEntry("info", scope, message),
  warn: (scope: string, message: string): LogRow => logEntry("warn", scope, message),
  error: (scope: string, message: string): LogRow => logEntry("error", scope, message),
};

let nextLogRowId = 1;

type SetupForm = {
  setupTarget: SetupTarget;
  vmDestination: string;
  vmName: string;
  diskGb: string;
  processorCount: string;
  enableSwap: boolean;
  networkMode: NetworkMode;
  switchName: string;
  adapterName: string;
  staticIp: string;
  gateway: string;
  dns: string;
  playerIpMode: PlayerIpMode;
  playerIp: string;
  worldName: string;
  region: string;
  tokenSource: string;
  survivalInstances: string;
  includeSocial: boolean;
  deepDesertPveInstances: string;
  deepDesertPvpInstances: string;
  deepDesertWarmServers: string;
  remoteHost: string;
  remoteUser: string;
  remoteKeyPath: string;
  saveRemoteServer: boolean;
};

const defaultForm: SetupForm = {
  setupTarget: "hyperv",
  vmDestination: "",
  vmName: "dune-server",
  diskGb: "100",
  processorCount: "4",
  enableSwap: false,
  networkMode: "static",
  switchName: "",
  adapterName: "",
  staticIp: "",
  gateway: "",
  dns: "1.1.1.1",
  playerIpMode: "local",
  playerIp: "",
  worldName: "Arrakis",
  region: "Europe Test",
  tokenSource: "",
  survivalInstances: "1",
  includeSocial: true,
  deepDesertPveInstances: "0",
  deepDesertPvpInstances: "0",
  deepDesertWarmServers: "0",
  remoteHost: "",
  remoteUser: "root",
  remoteKeyPath: "",
  saveRemoteServer: true,
};

const remoteProfileStorageKey = "dune-manager.remote-ubuntu-profile";
const remoteServersStorageKey = "dune-manager.remote-servers";

const defaultRemoteAttachForm: RemoteAttachForm = {
  host: "",
  keyPath: "",
};

const zeroToFour = ["0", "1", "2", "3", "4"];
const oneToFour = ["1", "2", "3", "4"];
const zeroToOne = ["0", "1"];
const playerPortForwards = [
  { ports: "7777-7810", protocol: "UDP", purpose: "Game servers" },
  { ports: "31982", protocol: "TCP", purpose: "RMQ" },
];

export function App() {
  const [activePage, setActivePage] = useState<PageId>("home");
  const [form, setForm] = useState<SetupForm>(defaultForm);
  const [started, setStarted] = useState(false);
  const [setupRunning, setSetupRunning] = useState(false);
  const [setupRows, setSetupRows] = useState<LogRow[]>([]);
  const [initRows, setInitRows] = useState<LogRow[]>([]);
  const [logLevelFilter, setLogLevelFilter] = useState<LogLevelFilter>("info");
  const [rollbackOpen, setRollbackOpen] = useState(false);
  const [rollbackRunning, setRollbackRunning] = useState(false);
  const [failedRollbackRequest, setFailedRollbackRequest] = useState<RollbackRequest | null>(null);
  const [remoteAttachOpen, setRemoteAttachOpen] = useState(false);
  const [remoteAttachRunning, setRemoteAttachRunning] = useState(false);
  const [remoteAttachForm, setRemoteAttachForm] = useState<RemoteAttachForm>(defaultRemoteAttachForm);
  const [remoteServerToRemove, setRemoteServerToRemove] = useState<RemoteServerRecord | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<Update | null>(null);
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>("idle");
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<string | null>(null);
  const [hostReadiness, setHostReadiness] = useState<HostReadiness | null>(null);
  const [driveCandidates, setDriveCandidates] = useState<DriveCandidate[]>([]);
  const [networkAdapters, setNetworkAdapters] = useState<NetworkAdapterCandidate[]>([]);
  const [externalIp, setExternalIp] = useState<string | null>(null);
  const [networkDetection, setNetworkDetection] = useState<DetectionState>("detecting");
  const [duneVms, setDuneVms] = useState<DuneVmCandidate[]>([]);
  const [vmDetection, setVmDetection] = useState<DetectionState>("detecting");
  const [vmDestinationHasVm, setVmDestinationHasVm] = useState(false);
  const [remoteServers, setRemoteServers] = useState<RemoteServerRecord[]>([]);
  const [remoteServerStatuses, setRemoteServerStatuses] = useState<Record<string, RemoteServerStatus>>({});
  const [remoteServerStatusErrors, setRemoteServerStatusErrors] = useState<Record<string, string>>({});
  const [remoteServerBusy, setRemoteServerBusy] = useState<Record<string, string>>({});
  const [managerApiProbes, setManagerApiProbes] = useState<Record<string, ManagerApiProbe>>({});
  const [remotePreflight, setRemotePreflight] = useState<UbuntuSshPreflight | null>(null);
  const [remotePreflightStatus, setRemotePreflightStatus] = useState<DetectionState>("detecting");
  const calculatedMemory = useMemo(() => calculateRequiredMemory(form), [form]);
  const environmentGate = useMemo(
    () => setupEnvironmentGate(networkDetection, hostReadiness, networkAdapters),
    [hostReadiness, networkAdapters, networkDetection],
  );
  const layoutPreview = useMemo(() => setupLayoutPreview(form), [form]);
  const updateCheckInFlight = useRef(false);
  const update = <K extends keyof SetupForm>(key: K, value: SetupForm[K]) => {
    setForm((current) => normalizeSetupForm({ ...current, [key]: value }));
  };
  const appendInitRow = (row: LogRow) => {
    setInitRows((rows) => [...rows, row]);
  };
  const checkForAppUpdate = async (source: "startup" | "manual") => {
    if (updateCheckInFlight.current) return;
    updateCheckInFlight.current = true;
    setUpdateStatus("checking");
    setUpdateProgress(null);
    appendInitRow(log.info("updates", "Checking for app updates."));
    try {
      const nextUpdate = await check();
      setAvailableUpdate(nextUpdate);
      if (nextUpdate) {
        setUpdateStatus("available");
        appendInitRow(
          log.info(
            "updates",
            `Update ${nextUpdate.version} is available; current version is ${nextUpdate.currentVersion}.`,
          ),
        );
        if (source === "startup") setUpdateDialogOpen(true);
      } else {
        setUpdateStatus("current");
        appendInitRow(log.info("updates", "The app is up to date."));
      }
    } catch (err) {
      setUpdateStatus("failed");
      appendInitRow(log.warn("updates", errorMessage(err)));
    } finally {
      updateCheckInFlight.current = false;
    }
  };
  const installAppUpdate = async () => {
    if (!availableUpdate) return;
    let downloaded = 0;
    let total: number | null = null;
    setUpdateStatus("installing");
    setUpdateProgress("Preparing download...");
    appendInitRow(log.info("updates", `Installing update ${availableUpdate.version}.`));
    try {
      await availableUpdate.downloadAndInstall((event: DownloadEvent) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? null;
          downloaded = 0;
          setUpdateProgress(total ? `Downloading 0 of ${formatBytes(total)}` : "Downloading update...");
        }
        if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setUpdateProgress(
            total
              ? `Downloading ${formatBytes(downloaded)} of ${formatBytes(total)}`
              : `Downloading ${formatBytes(downloaded)}`,
          );
        }
        if (event.event === "Finished") {
          setUpdateProgress("Installing update...");
        }
      });
      setUpdateStatus("relaunching");
      setUpdateProgress("Relaunching...");
      appendInitRow(log.info("updates", "Update installed; relaunching the app."));
      await relaunch();
    } catch (err) {
      setUpdateStatus("failed");
      setUpdateProgress(null);
      appendInitRow(log.error("updates", errorMessage(err)));
    }
  };

  const runRemotePreflight = async () => {
    setRemotePreflightStatus("detecting");
    setRemotePreflight(null);
    setSetupRows((rows) => [...rows, log.info("ubuntu.preflight", "Checking remote Ubuntu host resources.")]);
    try {
      const preflight = await invoke<UbuntuSshPreflight>("preflight_remote_ubuntu", {
        request: remoteSetupRunRequest(form),
      });
      setRemotePreflight(preflight);
      setRemotePreflightStatus("ready");
      setSetupRows((rows) => [
        ...rows,
        log.info(
          "ubuntu.preflight",
          `Remote resources: ${formatGiB(preflight.availableMemoryBytes)} available memory, ${preflight.logicalProcessorCount} logical CPUs, ${formatGiB(preflight.rootDiskAvailableBytes)} disk free.`,
        ),
      ]);
      if (preflight.publicIp && form.playerIpMode === "external" && form.playerIp !== preflight.publicIp) {
        update("playerIp", preflight.publicIp);
      }
    } catch (err) {
      setRemotePreflightStatus("failed");
      setSetupRows((rows) => [...rows, log.error("ubuntu.preflight", errorMessage(err))]);
    }
  };

  const attachRemoteServer = async () => {
    setRemoteAttachRunning(true);
    setSetupRows((rows) => [...rows, log.info("remote.attach", "Detecting remote Ubuntu battlegroups.")]);
    try {
      const detected = await invoke<RemoteServerRecord[]>("detect_remote_ubuntu_servers", {
        request: {
          host: remoteAttachForm.host.trim(),
          keyPath: remoteAttachForm.keyPath.trim(),
        },
      });
      if (detected.length === 0) {
        setSetupRows((rows) => [...rows, log.warn("remote.attach", "No remote Dune battlegroups were detected.")]);
        return;
      }
      const createdAt = new Date().toISOString();
      const records = detected.map((server) => ({
        ...server,
        createdAt: server.createdAt || createdAt,
      }));
      setRemoteServers((servers) => persistRemoteServers(mergeRemoteServers(servers, records)));
      setActivePage("servers");
      setRemoteAttachOpen(false);
      setSetupRows((rows) => [
        ...rows,
        log.info("remote.attach", `Added ${records.length} remote Ubuntu server${records.length === 1 ? "" : "s"}.`),
      ]);
    } catch (err) {
      setSetupRows((rows) => [...rows, log.error("remote.attach", errorMessage(err))]);
    } finally {
      setRemoteAttachRunning(false);
    }
  };

  const removeRemoteServer = (server: RemoteServerRecord) => {
    setRemoteServers((servers) => {
      const next = persistRemoteServers(servers.filter((candidate) => candidate.id !== server.id));
      return next;
    });
    setRemoteServerStatuses((statuses) => omitKey(statuses, server.id));
    setRemoteServerStatusErrors((errors) => omitKey(errors, server.id));
    setSetupRows((rows) => [...rows, log.info("remote.attach", `Forgot remote Ubuntu server ${server.host}.`)]);
    setRemoteServerToRemove(null);
  };

  const refreshRemoteServerStatus = async (server: RemoteServerRecord) => {
    if (!server.namespace || !server.battlegroupName || !server.host || !server.keyPath) return;
    setSetupRows((rows) => [...rows, log.info("remote.status", `Checking ${server.host} ${server.battlegroupName}.`)]);
    try {
      const status = await invoke<RemoteServerStatus>("remote_server_status", {
        request: remoteServerActionRequest(server),
      });
      setRemoteServerStatuses((statuses) => ({ ...statuses, [server.id]: status }));
      setRemoteServerStatusErrors((errors) => omitKey(errors, server.id));
      setRemoteServers((servers) =>
        persistRemoteServers(
          servers.map((candidate) =>
            candidate.id === server.id ? { ...candidate, phase: status.battlegroup.phase || server.phase } : candidate,
          ),
        ),
      );
      setSetupRows((rows) => [
        ...rows,
        log.info(
          "remote.status",
          `${server.battlegroupName}: ${status.battlegroup.phase || "unknown"}, server group ${status.battlegroup.serverGroupPhase || "unknown"}, Director ${status.battlegroup.directorPhase || "unknown"}.`,
        ),
      ]);
    } catch (err) {
      const message = errorMessage(err);
      setRemoteServerStatusErrors((errors) => ({ ...errors, [server.id]: message }));
      setSetupRows((rows) => [...rows, log.warn("remote.status", message)]);
    }
  };

  const runRemoteBattlegroupAction = async (server: RemoteServerRecord, action: "start" | "stop") => {
    setRemoteServerBusy((busy) => ({ ...busy, [server.id]: action === "start" ? "Starting battlegroup" : "Stopping battlegroup" }));
    setSetupRows((rows) => [...rows, log.info("bg", `${action === "start" ? "Starting" : "Stopping"} ${server.battlegroupName}.`)]);
    try {
      const status = await invoke<RemoteServerStatus>(
        action === "start" ? "start_remote_battlegroup" : "stop_remote_battlegroup",
        { request: remoteServerActionRequest(server) },
      );
      setRemoteServerStatuses((statuses) => ({ ...statuses, [server.id]: status }));
      setRemoteServerStatusErrors((errors) => omitKey(errors, server.id));
      setRemoteServers((servers) =>
        persistRemoteServers(
          servers.map((candidate) =>
            candidate.id === server.id ? { ...candidate, phase: status.battlegroup.phase || candidate.phase } : candidate,
          ),
        ),
      );
    } catch (err) {
      const message = errorMessage(err);
      setRemoteServerStatusErrors((errors) => ({ ...errors, [server.id]: message }));
      setSetupRows((rows) => [...rows, log.error("bg", message)]);
    } finally {
      setRemoteServerBusy((busy) => omitKey(busy, server.id));
    }
  };

  const runRemoteManagerApiAction = async (server: RemoteServerRecord, action: "start" | "stop") => {
    setRemoteServerBusy((busy) => ({ ...busy, [server.id]: action === "start" ? "Starting Manager API" : "Stopping Manager API" }));
    setSetupRows((rows) => [...rows, log.info("manager-api", `${action === "start" ? "Starting" : "Stopping"} Manager API on ${server.host}.`)]);
    try {
      const managerApi = await invoke<RemoteManagerApiServiceStatus>(
        action === "start" ? "start_remote_manager_api" : "stop_remote_manager_api",
        { request: remoteManagerApiActionRequest(server) },
      );
      setRemoteServerStatuses((statuses) => ({
        ...statuses,
        [server.id]: {
          battlegroup: statuses[server.id]?.battlegroup ?? {
            stop: false,
            phase: server.phase,
            serverGroupPhase: "",
            directorPhase: "",
          },
          managerApi,
        },
      }));
      setRemoteServerStatusErrors((errors) => omitKey(errors, server.id));
    } catch (err) {
      const message = errorMessage(err);
      setRemoteServerStatusErrors((errors) => ({ ...errors, [server.id]: message }));
      setSetupRows((rows) => [...rows, log.error("manager-api", message)]);
    } finally {
      setRemoteServerBusy((busy) => omitKey(busy, server.id));
    }
  };

  useEffect(() => {
    let cancelled = false;
    const appendInit = (row: LogRow) => {
      if (!cancelled) setInitRows((rows) => [...rows, row]);
    };
    appendInit(log.info("init", "Starting initial detection."));
    invoke<string>("default_vm_location")
      .then((location) => {
        if (cancelled) return;
        setForm((current) => (current.vmDestination ? current : { ...current, vmDestination: location }));
      })
      .catch(() => {
        // Keep the field user-editable if the native default path cannot be resolved.
      });
    appendInit(log.info("capabilities", "Checking host capabilities."));
    invoke<EnvironmentDetection>("detect_environment")
      .then((environment) => {
        if (cancelled) return;
        setHostReadiness(environment.readiness);
        setDriveCandidates(environment.drives);
        setNetworkAdapters(environment.networkAdapters);
        setExternalIp(environment.externalIp);
        setNetworkDetection("ready");
        appendInit(log.info("capabilities", "Host capability detection completed."));
        const gate = setupEnvironmentGate("ready", environment.readiness, environment.networkAdapters);
        for (const row of environmentLogRows(
          "ready",
          environment.readiness,
          environment.networkAdapters,
          environment.drives,
          environment.externalIp,
          gate,
        )) {
          appendInit(row);
        }
        const first = environment.networkAdapters[0];
        if (first) {
          setForm((current) => {
            if (current.adapterName || current.staticIp || current.playerIp || current.gateway) {
              return current;
            }
            return {
              ...current,
              adapterName: first.name,
              switchName: first.existingExternalSwitch || first.name,
              staticIp: first.suggestedIpv4Address,
              playerIp: current.playerIpMode === "external" && environment.externalIp
                ? environment.externalIp
                : first.suggestedIpv4Address,
              gateway: first.gateway,
            };
          });
        }
      })
      .catch(() => {
        if (!cancelled) {
          setNetworkDetection("failed");
          appendInit(log.error("capabilities", "Host capability detection failed."));
        }
      })
      .finally(() => {
        if (cancelled) return;
        appendInit(log.info("vms", "Detecting existing Dune VMs."));
        invoke<DuneVmCandidate[]>("detect_dune_vms")
          .then((candidates) => {
            if (cancelled) return;
            setDuneVms(candidates);
            setVmDetection("ready");
            appendInit(
              candidates.length > 0
                ? log.info("vms", `Detected ${candidates.length} Dune VM candidate${candidates.length === 1 ? "" : "s"}.`)
                : log.warn("vms", "No existing Dune VMs were detected."),
            );
          })
          .catch(() => {
            if (!cancelled) {
              setVmDetection("failed");
              appendInit(log.error("vms", "Existing VM detection failed."));
            }
          });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const text = window.localStorage.getItem(remoteProfileStorageKey);
    if (!text) return;
    try {
      const profile = JSON.parse(text) as Partial<Pick<SetupForm, "remoteHost" | "remoteUser" | "remoteKeyPath">>;
      setForm((current) =>
        normalizeSetupForm({
          ...current,
          remoteHost: profile.remoteHost || current.remoteHost,
          remoteUser: profile.remoteUser || current.remoteUser,
          remoteKeyPath: profile.remoteKeyPath || current.remoteKeyPath,
        }),
      );
    } catch {
      window.localStorage.removeItem(remoteProfileStorageKey);
    }
  }, []);

  useEffect(() => {
    setRemoteServers(readRemoteServers());
  }, []);

  useEffect(() => {
    const targets = [
      ...duneVms
        .map((candidate) => [managerApiKeyForVm(candidate), managerApiUrlForVm(candidate)] as const)
        .filter((target): target is readonly [string, string] => !!target[1]),
      ...remoteServers
        .map((server) => [managerApiKeyForRemote(server), server.managerApiUrl] as const)
        .filter((target): target is readonly [string, string] => !!target[1]),
    ];
    if (targets.length === 0) {
      setManagerApiProbes({});
      return;
    }
    let cancelled = false;
    for (const [key, url] of targets) {
      void invoke<ManagerApiProbe>("check_manager_api", { request: { url } })
        .then((probe) => {
          if (!cancelled) {
            setManagerApiProbes((current) => ({ ...current, [key]: probe }));
          }
        })
        .catch((err) => {
          if (!cancelled) {
            setManagerApiProbes((current) => ({
              ...current,
              [key]: managerApiProbeError(url, errorMessage(err)),
            }));
          }
        });
    }
    return () => {
      cancelled = true;
    };
  }, [duneVms, remoteServers]);

  useEffect(() => {
    let cancelled = false;
    for (const server of remoteServers) {
      if (!server.namespace || !server.battlegroupName || !server.host || !server.keyPath) continue;
      setSetupRows((rows) => [
        ...rows,
        log.info("remote.status", `Checking saved remote server ${server.host} ${server.battlegroupName}.`),
      ]);
      void invoke<RemoteServerStatus>("remote_server_status", {
        request: remoteServerActionRequest(server),
      })
        .then((status) => {
          if (cancelled) return;
          setRemoteServerStatuses((statuses) => ({ ...statuses, [server.id]: status }));
          setRemoteServerStatusErrors((errors) => omitKey(errors, server.id));
          setRemoteServers((servers) =>
            persistRemoteServers(
              servers.map((candidate) =>
                candidate.id === server.id
                  ? { ...candidate, phase: status.battlegroup.phase || candidate.phase }
                  : candidate,
              ),
            ),
          );
          setSetupRows((rows) => [
            ...rows,
            log.info(
              "remote.status",
              `${server.battlegroupName}: ${status.battlegroup.phase || "unknown"}, server group ${status.battlegroup.serverGroupPhase || "unknown"}, Director ${status.battlegroup.directorPhase || "unknown"}.`,
            ),
          ]);
        })
        .catch((err) => {
          if (!cancelled) {
            const message = errorMessage(err);
            setRemoteServerStatusErrors((errors) => ({ ...errors, [server.id]: message }));
            setSetupRows((rows) => [...rows, log.warn("remote.status", message)]);
          }
        });
    }
    return () => {
      cancelled = true;
    };
  }, [remoteServers.map((server) => server.id).join("|")]);

  useEffect(() => {
    const profile = {
      remoteHost: form.remoteHost,
      remoteUser: form.remoteUser,
      remoteKeyPath: form.remoteKeyPath,
    };
    window.localStorage.setItem(remoteProfileStorageKey, JSON.stringify(profile));
    setRemotePreflight(null);
    setRemotePreflightStatus(form.remoteHost || form.remoteKeyPath ? "failed" : "detecting");
  }, [form.remoteHost, form.remoteKeyPath, form.remoteUser]);

  useEffect(() => {
    appendInitRow(log.debug("updates", "Automatic update checks are disabled; use the manual check on Home."));
  }, []);

  useEffect(() => {
    const onError = (event: ErrorEvent) => {
      setSetupRows((rows) => [...rows, log.error("ui", event.message || "Unhandled browser error.")]);
    };
    const onRejection = (event: PromiseRejectionEvent) => {
      setSetupRows((rows) => [...rows, log.error("ui", errorMessage(event.reason))]);
    };
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    return () => {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    listen<SetupLogPayload>("setup-log", (event) => {
      if (cancelled) return;
      setSetupRows((rows) => [
        ...rows,
        logEntry(event.payload.level, event.payload.scope, event.payload.message),
      ]);
    }).then((handler) => {
      unlisten = handler;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const path = form.vmDestination.trim();
    if (!/^[A-Za-z]:[\\/]/.test(path)) {
      setVmDestinationHasVm(false);
      return () => {
        cancelled = true;
      };
    }
    const timer = window.setTimeout(() => {
      invoke<boolean>("vm_destination_has_vm", { path })
        .then((hasVm) => {
          if (!cancelled) setVmDestinationHasVm(hasVm);
        })
        .catch(() => {
          if (!cancelled) setVmDestinationHasVm(false);
        });
    }, 150);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [form.vmDestination]);

  const logRows = useMemo(() => [...initRows, ...setupRows], [initRows, setupRows]);
  const visibleLogRows = useMemo(() => filterLogRows(logRows, logLevelFilter), [logLevelFilter, logRows]);
  const initialDetectionRunning = networkDetection === "detecting" || vmDetection === "detecting";
  const initialDetectionMessage =
    networkDetection === "detecting"
      ? "Detecting host environment and capabilities"
      : "Detecting existing Dune servers";

  const startSetup = async () => {
    const request = setupRunRequest(form, calculatedMemory.gb);
    setStarted(true);
    setSetupRunning(true);
    setFailedRollbackRequest(null);
    try {
      if (form.setupTarget === "ubuntu") {
        const pendingRecord = form.saveRemoteServer ? remoteServerDraftFromForm(form) : null;
        if (pendingRecord) {
          setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, pendingRecord)));
          setActivePage("servers");
        }
        const result = await invoke<RemoteSetupRunResult>("start_remote_ubuntu_setup", {
          request: remoteSetupRunRequest(form),
        });
        if (form.saveRemoteServer) {
          const record = remoteServerRecordFromSetup(form, result, pendingRecord?.id);
          setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, record)));
          setActivePage("servers");
        }
      } else {
        await invoke("start_full_setup", {
          request,
        });
      }
    } catch (err) {
      console.error(err);
      if (form.setupTarget === "ubuntu" && form.saveRemoteServer) {
        const pending = remoteServerDraftFromForm(form);
        setRemoteServers((servers) =>
          persistRemoteServers(upsertRemoteServer(servers, { ...pending, phase: "Setup failed" })),
        );
      }
      if (form.setupTarget === "hyperv") {
        setFailedRollbackRequest(rollbackRequestFromSetup(request));
        setRollbackOpen(true);
      }
    } finally {
      setSetupRunning(false);
    }
  };

  const rollback = async () => {
    if (!failedRollbackRequest) return;
    setRollbackRunning(true);
    try {
      await invoke("rollback_setup", { request: failedRollbackRequest });
      setRollbackOpen(false);
      setFailedRollbackRequest(null);
    } finally {
      setRollbackRunning(false);
    }
  };

  return (
    <Theme
      appearance="dark"
      accentColor="bronze"
      grayColor="sand"
      panelBackground="solid"
      radius="medium"
      scaling="95%"
    >
      {initialDetectionRunning ? (
        <InitialDetectionScreen
          message={initialDetectionMessage}
          networkDetection={networkDetection}
          vmDetection={vmDetection}
          rows={initRows}
        />
      ) : (
      <Flex direction="column" className="app-root">
        <Header
          activePage={activePage}
          onNavigate={setActivePage}
        />
        <Separator size="4" />
        <Box className="app-main has-log">
          <AppErrorBoundary
            onError={(message) => setSetupRows((rows) => [...rows, log.error("ui", message)])}
          >
            {activePage === "home" ? (
              <HomePage
                environmentGate={environmentGate}
                networkDetection={networkDetection}
                vmDetection={vmDetection}
                hostReadiness={hostReadiness}
                networkAdapters={networkAdapters}
                externalIp={externalIp}
                duneVms={duneVms}
                remoteServers={remoteServers}
                updateStatus={updateStatus}
                availableUpdate={availableUpdate}
                updateProgress={updateProgress}
                onCheckUpdate={() => void checkForAppUpdate("manual")}
                onInstallUpdate={() => setUpdateDialogOpen(true)}
              />
            ) : null}
            {activePage === "servers" ? (
              <ServersPage
                duneVms={duneVms}
                remoteServers={remoteServers}
                probes={managerApiProbes}
                remoteStatuses={remoteServerStatuses}
                remoteStatusErrors={remoteServerStatusErrors}
                remoteBusy={remoteServerBusy}
                onAddRemoteServer={() => {
                  setRemoteAttachForm({
                    host: form.remoteHost,
                    keyPath: form.remoteKeyPath,
                  });
                  setRemoteAttachOpen(true);
                }}
                onRemoveRemoteServer={setRemoteServerToRemove}
                onRefreshRemoteStatus={(server) => void refreshRemoteServerStatus(server)}
                onStartRemoteBattlegroup={(server) => void runRemoteBattlegroupAction(server, "start")}
                onStopRemoteBattlegroup={(server) => void runRemoteBattlegroupAction(server, "stop")}
                onStartRemoteManagerApi={(server) => void runRemoteManagerApiAction(server, "start")}
                onStopRemoteManagerApi={(server) => void runRemoteManagerApiAction(server, "stop")}
              />
            ) : null}
            {activePage === "install" ? (
              <InstallControls
                form={form}
                calculatedMemory={calculatedMemory}
                layoutPreview={layoutPreview}
                hostReadiness={hostReadiness}
                driveCandidates={driveCandidates}
                networkAdapters={networkAdapters}
                networkDetection={networkDetection}
                externalIp={externalIp}
                environmentGate={environmentGate}
                setupRunning={setupRunning}
                vmDestinationHasVm={vmDestinationHasVm}
                remotePreflight={remotePreflight}
                remotePreflightStatus={remotePreflightStatus}
                update={update}
                onRemotePreflight={() => void runRemotePreflight()}
                onStart={startSetup}
              />
            ) : null}
          </AppErrorBoundary>
          <LogWindow rows={visibleLogRows} level={logLevelFilter} onLevelChange={setLogLevelFilter} />
        </Box>
        <RollbackDialog
          open={rollbackOpen}
          rollbackRunning={rollbackRunning}
          onOpenChange={setRollbackOpen}
          onRollback={rollback}
        />
        <UpdateDialog
          open={updateDialogOpen}
          update={availableUpdate}
          status={updateStatus}
          progress={updateProgress}
          onOpenChange={setUpdateDialogOpen}
          onInstall={() => void installAppUpdate()}
        />
        <RemoteAttachDialog
          open={remoteAttachOpen}
          form={remoteAttachForm}
          running={remoteAttachRunning}
          onOpenChange={setRemoteAttachOpen}
          onChange={setRemoteAttachForm}
          onAttach={() => void attachRemoteServer()}
        />
        <RemoveRemoteServerDialog
          server={remoteServerToRemove}
          onOpenChange={(open) => {
            if (!open) setRemoteServerToRemove(null);
          }}
          onRemove={removeRemoteServer}
        />
      </Flex>
      )}
    </Theme>
  );
}

function InitialDetectionScreen({
  message,
  networkDetection,
  vmDetection,
  rows,
}: {
  message: string;
  networkDetection: DetectionState;
  vmDetection: DetectionState;
  rows: LogRow[];
}) {
  const recentRows = rows.slice(-4);
  return (
    <Flex align="center" justify="center" className="app-root boot-screen">
      <Card size="4" variant="surface" className="boot-card">
        <Flex direction="column" align="center" gap="4">
          <Box className="boot-spinner" aria-hidden />
          <Box>
            <Heading align="center" size="5">
              {message}
            </Heading>
            <Text as="p" align="center" size="2" color="gray" mb="0">
              Checking permissions, Hyper-V readiness, networking, memory, storage, and existing servers.
            </Text>
          </Box>
          <Box className="boot-status-card">
            <BootStatusRow label="Capabilities" status={networkDetection} />
            <BootStatusRow label="Existing VMs" status={vmDetection} />
          </Box>
          {recentRows.length > 0 ? (
            <Flex direction="column" gap="1" width="100%">
              {recentRows.map((row, index) => (
                <Text key={`${row.timestamp}-${index}`} size="1" color="gray" className="mono boot-log-line">
                  {row.message}
                </Text>
              ))}
            </Flex>
          ) : null}
        </Flex>
      </Card>
    </Flex>
  );
}

function BootStatusRow({ label, status }: { label: string; status: DetectionState }) {
  const color = status === "ready" ? "green" : status === "failed" ? "red" : "amber";
  const text = status === "ready" ? "Ready" : status === "failed" ? "Failed" : "Detecting";
  return (
    <Flex align="center" justify="between" gap="4" className="boot-status-row">
      <Text size="2" color="gray">
        {label}
      </Text>
      <Badge color={color} variant="soft">
        {text}
      </Badge>
    </Flex>
  );
}

class AppErrorBoundary extends Component<AppErrorBoundaryProps, AppErrorBoundaryState> {
  state: AppErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): AppErrorBoundaryState {
    return { error: error.message };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.props.onError(`${error.message}${info.componentStack ? `; ${info.componentStack.split("\n")[1]?.trim() ?? ""}` : ""}`);
  }

  render() {
    if (this.state.error) {
      return (
        <Card size="3" variant="surface" className="pane page-pane">
          <Flex direction="column" gap="3">
            <Heading size="4">UI Error</Heading>
            <Text size="2" color="gray">
              The view failed to render. Details were written to the log window.
            </Text>
            <Text size="2" className="mono">
              {this.state.error}
            </Text>
          </Flex>
        </Card>
      );
    }

    return this.props.children;
  }
}

function Header({
  activePage,
  onNavigate,
}: {
  activePage: PageId;
  onNavigate: (page: PageId) => void;
}) {
  return (
    <Flex asChild align="center" justify="between" p="4">
      <header>
        <Flex align="center" gap="5">
          <Flex align="center" gap="3">
            <CubeIcon width="24" height="24" />
            <Heading size="4">Dune Dedicated Server Manager</Heading>
          </Flex>
          <TopNav
            activePage={activePage}
            onNavigate={onNavigate}
          />
        </Flex>
      </header>
    </Flex>
  );
}

function TopNav({
  activePage,
  onNavigate,
}: {
  activePage: PageId;
  onNavigate: (page: PageId) => void;
}) {
  return (
    <Box asChild>
      <nav aria-label="Primary navigation">
        <TabNav.Root size="2" color="bronze">
          {pages.map((page) => (
            <TabNav.Link
              key={page.id}
              href="#"
              active={page.id === activePage}
              onClick={(event) => {
                event.preventDefault();
                onNavigate(page.id);
              }}
            >
              {page.label}
            </TabNav.Link>
          ))}
        </TabNav.Root>
      </nav>
    </Box>
  );
}

function HomePage({
  environmentGate,
  networkDetection,
  vmDetection,
  hostReadiness,
  networkAdapters,
  externalIp,
  duneVms,
  remoteServers,
  updateStatus,
  availableUpdate,
  updateProgress,
  onCheckUpdate,
  onInstallUpdate,
}: {
  environmentGate: EnvironmentGate;
  networkDetection: DetectionState;
  vmDetection: DetectionState;
  hostReadiness: HostReadiness | null;
  networkAdapters: NetworkAdapterCandidate[];
  externalIp: string | null;
  duneVms: DuneVmCandidate[];
  remoteServers: RemoteServerRecord[];
  updateStatus: UpdateStatus;
  availableUpdate: Update | null;
  updateProgress: string | null;
  onCheckUpdate: () => void;
  onInstallUpdate: () => void;
}) {
  const vmDetectionReady = vmDetection === "ready";
  const primaryAdapter = networkAdapters[0];

  return (
    <Card size="3" variant="surface" className="pane page-pane">
      <Flex direction="column" gap="5" height="100%" minHeight="0">
        <Box className="info-card">
          <InfoRow
            label="Privileges"
            value={hostReadiness?.elevated ? "Administrator" : "Not elevated"}
            tone={hostReadiness?.elevated ? "green" : "red"}
          />
          <InfoRow
            label="Hyper-V"
            value={
              hostReadiness?.hypervAvailable && hostReadiness.vmmsRunning
                ? "Available"
                : networkDetection === "failed"
                  ? "Failed"
                  : "Checking"
            }
            tone={hostReadiness?.hypervAvailable && hostReadiness.vmmsRunning ? "green" : "amber"}
          />
          <InfoRow
            label="Virtualization"
            value={
              hostReadiness?.virtualizationFirmwareEnabled === false
                ? "Disabled"
                : hostReadiness
                  ? "Operational"
                  : "Checking"
            }
            tone={hostReadiness?.virtualizationFirmwareEnabled === false ? "red" : hostReadiness ? "green" : "amber"}
          />
          <InfoRow
            label="Memory"
            value={
              hostReadiness
                ? `${formatGiB(hostReadiness.availablePhysicalMemoryBytes)} available of ${formatGiB(hostReadiness.totalPhysicalMemoryBytes)}`
                : "Checking"
            }
            tone={hostReadiness ? "green" : "amber"}
          />
          <InfoRow
            label="CPU Cores"
            value={hostReadiness ? `${hostReadiness.logicalProcessorCount || "unknown"} logical` : "Checking"}
            tone={hostReadiness?.logicalProcessorCount ? "green" : "amber"}
          />
          <InfoRow
            label="Network"
            value={
              primaryAdapter
                ? `${primaryAdapter.name} ${primaryAdapter.ipv4Address}/${primaryAdapter.prefixLength}`
                : networkDetection === "failed"
                  ? "Failed"
                  : "Checking"
            }
            tone={primaryAdapter ? "green" : networkDetection === "failed" ? "red" : "amber"}
          />
          <InfoRow label="External IP" value={externalIp ?? "Not detected"} tone={externalIp ? "green" : "amber"} />
          <InfoRow
            label="Local VMs"
            value={vmDetectionReady ? `${duneVms.length} found` : vmDetection === "failed" ? "Failed" : "Checking"}
            tone={vmDetectionReady ? "green" : vmDetection === "failed" ? "red" : "amber"}
          />
          <InfoRow label="Remote Servers" value={`${remoteServers.length} saved`} tone="green" />
          <InfoActionRow
            label="App Update"
            value={updateLabel(updateStatus, availableUpdate, updateProgress)}
            tone={updateTone(updateStatus)}
            actionLabel={availableUpdate ? "Install" : "Check"}
            disabled={
              updateStatus === "checking" ||
              updateStatus === "installing" ||
              updateStatus === "relaunching"
            }
            onAction={availableUpdate ? onInstallUpdate : onCheckUpdate}
          />
        </Box>

        {!environmentGate.canContinue ? (
          <Box className="setup-readiness">
            <ul className="setup-issues">
              {environmentGate.reasons.map((reason) => (
                <li key={reason}>{reason}</li>
              ))}
            </ul>
          </Box>
        ) : null}

      </Flex>
    </Card>
  );
}

function ServersPage({
  duneVms,
  remoteServers,
  probes,
  remoteStatuses,
  remoteStatusErrors,
  remoteBusy,
  onAddRemoteServer,
  onRemoveRemoteServer,
  onRefreshRemoteStatus,
  onStartRemoteBattlegroup,
  onStopRemoteBattlegroup,
  onStartRemoteManagerApi,
  onStopRemoteManagerApi,
}: {
  duneVms: DuneVmCandidate[];
  remoteServers: RemoteServerRecord[];
  probes: Record<string, ManagerApiProbe>;
  remoteStatuses: Record<string, RemoteServerStatus>;
  remoteStatusErrors: Record<string, string>;
  remoteBusy: Record<string, string>;
  onAddRemoteServer: () => void;
  onRemoveRemoteServer: (server: RemoteServerRecord) => void;
  onRefreshRemoteStatus: (server: RemoteServerRecord) => void;
  onStartRemoteBattlegroup: (server: RemoteServerRecord) => void;
  onStopRemoteBattlegroup: (server: RemoteServerRecord) => void;
  onStartRemoteManagerApi: (server: RemoteServerRecord) => void;
  onStopRemoteManagerApi: (server: RemoteServerRecord) => void;
}) {
  return (
    <Card size="3" variant="surface" className="pane page-pane">
      <Flex direction="column" gap="4" height="100%" minHeight="0">
        <Flex align="center" justify="between" gap="3">
          <Box>
            <Heading size="5">Servers</Heading>
            <Text as="p" size="2" color="gray" mb="0">
              Setup happens in the desktop app. Management uses the Manager API once it is reachable.
            </Text>
          </Box>
          <Button type="button" variant="surface" onClick={onAddRemoteServer}>
            Add remote Ubuntu server
          </Button>
        </Flex>
        <Box className="setup-scroll">
          <Flex direction="column" gap="3">
            {duneVms.length + remoteServers.length > 0 ? (
              <>
                {duneVms.map((candidate) => (
                  <ServerCard
                    key={candidate.vm.name}
                    candidate={candidate}
                    compact
                    managerApiProbe={probes[managerApiKeyForVm(candidate)]}
                  />
                ))}
                {remoteServers.map((server) => (
                  <RemoteServerCard
                    key={server.id}
                    server={server}
                    compact
                    managerApiProbe={probes[managerApiKeyForRemote(server)]}
                    status={remoteStatuses[server.id]}
                    statusError={remoteStatusErrors[server.id]}
                    busyLabel={remoteBusy[server.id]}
                    onRemove={() => onRemoveRemoteServer(server)}
                    onRefresh={() => onRefreshRemoteStatus(server)}
                    onStartBattlegroup={() => onStartRemoteBattlegroup(server)}
                    onStopBattlegroup={() => onStopRemoteBattlegroup(server)}
                    onStartManagerApi={() => onStartRemoteManagerApi(server)}
                    onStopManagerApi={() => onStopRemoteManagerApi(server)}
                  />
                ))}
              </>
            ) : (
              <EmptyState
                title="No Dune servers detected"
                body="Create a new server or add a remote Ubuntu server profile."
              />
            )}
          </Flex>
        </Box>
      </Flex>
    </Card>
  );
}

function ServerCard({
  candidate,
  compact = false,
  managerApiProbe,
}: {
  candidate: DuneVmCandidate;
  compact?: boolean;
  managerApiProbe?: ManagerApiProbe;
}) {
  const vm = candidate.vm;
  const primaryIp = vm.ipv4Addresses[0] ?? "No IPv4 reported";
  const diskLabel = vm.diskSizeBytes > 0 ? `${formatGiB(vm.diskSizeBytes)} disk` : "Disk size unknown";
  const usedDiskLabel = vm.diskFileSizeBytes > 0 ? `${formatGiB(vm.diskFileSizeBytes)} used` : "usage unknown";
  const managerApiUrl = managerApiUrlForVm(candidate);

  return (
    <Box className="server-card">
      <Flex align="start" justify="between" gap="3">
        <Box>
          <Flex align="center" gap="2">
            <Heading size={compact ? "3" : "4"}>{vm.name}</Heading>
            <Badge color={candidate.confidence === "high" ? "green" : candidate.confidence === "medium" ? "amber" : "gray"} variant="soft">
              {candidate.confidence}
            </Badge>
          </Flex>
          <Text as="div" size="2" color="gray">
            {vm.rawState} · {primaryIp}
          </Text>
        </Box>
        <Badge color={vm.state === "running" ? "green" : vm.state === "off" ? "gray" : "amber"} variant="surface">
          {vm.state}
        </Badge>
      </Flex>

      <Grid columns={compact ? "2" : "5"} gap="3" mt="3">
        <Metric label="Memory" value={formatGiB(vm.memoryAssignedBytes)} />
        <Metric label="CPU" value={vm.processorCount ? `${vm.processorCount} cores` : "unknown"} />
        <Metric label="Disk" value={`${diskLabel}; ${usedDiskLabel}`} />
        <Metric label="Switch" value={vm.switchNames.join(", ") || "none"} />
        <Metric label="Uptime" value={formatDuration(vm.uptimeSeconds)} />
      </Grid>
      <ManagerApiStatus probe={managerApiProbe} url={managerApiUrl} />

    </Box>
  );
}

function RemoteServerCard({
  server,
  compact = false,
  onRemove,
  managerApiProbe,
  status,
  statusError,
  busyLabel,
  onRefresh,
  onStartBattlegroup,
  onStopBattlegroup,
  onStartManagerApi,
  onStopManagerApi,
}: {
  server: RemoteServerRecord;
  compact?: boolean;
  onRemove?: () => void;
  managerApiProbe?: ManagerApiProbe;
  status?: RemoteServerStatus;
  statusError?: string;
  busyLabel?: string;
  onRefresh?: () => void;
  onStartBattlegroup?: () => void;
  onStopBattlegroup?: () => void;
  onStartManagerApi?: () => void;
  onStopManagerApi?: () => void;
}) {
  const battlegroupStarted = status ? isBattlegroupStarted(status.battlegroup) : false;
  const battlegroupStartRequested = status ? !status.battlegroup.stop : false;
  const battlegroupStopped = status ? status.battlegroup.stop : false;
  const managerInstalled = status?.managerApi.installed ?? false;
  const managerRunning = status?.managerApi.running ?? false;
  const busy = !!busyLabel;
  return (
    <Box className="server-card">
      <Flex align="start" justify="between" gap="3">
        <Box>
          <Flex align="center" gap="2">
            <Heading size={compact ? "3" : "4"}>{server.name}</Heading>
            <Badge color="bronze" variant="soft">
              Ubuntu
            </Badge>
          </Flex>
          <Text as="div" size="2" color="gray">
            {server.host} · {server.battlegroupName || "setup pending"}
          </Text>
        </Box>
        <Flex align="center" gap="2">
          <Badge
            color={
              statusError
                ? "red"
                : battlegroupStarted
                  ? "green"
                  : battlegroupStartRequested
                    ? "amber"
                  : battlegroupStopped
                    ? "gray"
                    : server.phase === "Setup running"
                      ? "amber"
                      : "green"
            }
            variant="surface"
          >
            {statusError
              ? "Check failed"
              : status
                ? battlegroupStarted
                  ? "Started"
                  : battlegroupStartRequested
                    ? "Starting"
                    : "Stopped"
                : server.phase}
          </Badge>
          {onRemove ? (
            <Button
              type="button"
              size="1"
              color="red"
              variant="soft"
              onClick={(event) => {
                event.stopPropagation();
                onRemove();
              }}
            >
              Forget
            </Button>
          ) : null}
        </Flex>
      </Flex>

      <Grid columns={compact ? "2" : "5"} gap="3" mt="3">
        <Metric label="Manager API" value={server.managerApiUrl || "pending"} />
        <Metric label="Namespace" value={server.namespace || "pending"} />
        <Metric label="BattleGroup" value={server.battlegroupName || "pending"} />
        <Metric label="SSH User" value={server.user} />
        <Metric label="Created" value={new Date(server.createdAt).toLocaleString()} />
      </Grid>
      <Box className="server-state" mt="3">
        <Grid columns="2" gap="3">
          <Metric
            label="BattleGroup State"
            value={
              status
                ? `${status.battlegroup.phase || "unknown"}; stop=${status.battlegroup.stop ? "true" : "false"}`
                : statusError || "Checking"
            }
          />
          <Metric
            label="Director"
            value={status ? status.battlegroup.directorPhase || "unknown" : statusError || "Checking"}
          />
          <Metric
            label="Server Group"
            value={status ? status.battlegroup.serverGroupPhase || "unknown" : statusError || "Checking"}
          />
          <Metric
            label="Manager API Service"
            value={
              status
                ? `${managerInstalled ? status.managerApi.serviceManager : "not installed"}; ${managerRunning ? "running" : "stopped"}`
                : statusError || "Checking"
            }
          />
        </Grid>
        <Flex align="center" justify="between" gap="2" mt="3" wrap="wrap">
          <Flex gap="2" wrap="wrap">
            <Button size="1" variant="surface" disabled={busy} onClick={onRefresh}>
              Refresh
            </Button>
            <Button
              size="1"
              variant="surface"
              disabled={busy || !status || !battlegroupStopped}
              onClick={onStartBattlegroup}
            >
              Start BattleGroup
            </Button>
            <Button
              size="1"
              variant="surface"
              disabled={busy || !status || !battlegroupStartRequested}
              onClick={onStopBattlegroup}
            >
              Stop BattleGroup
            </Button>
            <Button
              size="1"
              variant="surface"
              disabled={busy || !status || !managerInstalled || managerRunning}
              onClick={onStartManagerApi}
            >
              Start Manager API
            </Button>
            <Button
              size="1"
              variant="surface"
              disabled={busy || !status || !managerInstalled || !managerRunning}
              onClick={onStopManagerApi}
            >
              Stop Manager API
            </Button>
          </Flex>
          {busyLabel ? (
            <Text size="1" color="gray" className="mono">
              {busyLabel}
            </Text>
          ) : null}
        </Flex>
      </Box>
      <ManagerApiStatus probe={managerApiProbe} url={server.managerApiUrl} />
    </Box>
  );
}

function InfoRow({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: "green" | "amber" | "red";
}) {
  return (
    <Grid columns="160px 1fr auto" gap="3" align="center" className="info-row">
      <Text as="div" size="2" color="gray">
        {label}
      </Text>
      <Text as="div" size="2" className="mono metric-value">
        {value}
      </Text>
      <Badge color={tone} variant="soft">
        {tone === "green" ? "OK" : tone === "red" ? "Issue" : "Check"}
      </Badge>
    </Grid>
  );
}

function InfoActionRow({
  label,
  value,
  tone,
  actionLabel,
  disabled,
  onAction,
}: {
  label: string;
  value: string;
  tone: "green" | "amber" | "red";
  actionLabel: string;
  disabled: boolean;
  onAction: () => void;
}) {
  return (
    <Grid columns="160px 1fr auto auto" gap="3" align="center" className="info-row">
      <Text as="div" size="2" color="gray">
        {label}
      </Text>
      <Text as="div" size="2" className="mono metric-value">
        {value}
      </Text>
      <Badge color={tone} variant="soft">
        {tone === "green" ? "OK" : tone === "red" ? "Issue" : "Check"}
      </Badge>
      <Button size="1" variant="soft" color="bronze" disabled={disabled} onClick={onAction}>
        {actionLabel}
      </Button>
    </Grid>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <Box>
      <Text as="div" size="1" color="gray">
        {label}
      </Text>
      <Text as="div" size="2" className="mono metric-value">
        {value}
      </Text>
    </Box>
  );
}

function ManagerApiStatus({ probe, url }: { probe?: ManagerApiProbe; url: string }) {
  const tone = !url ? "amber" : probe?.ok ? "green" : probe?.reachable === false ? "red" : "amber";
  const label = !url
    ? "No Manager API URL"
    : !probe
      ? "Checking Manager API"
      : probe.ok
        ? `Reachable${probe.apiVersion ? ` (${probe.apiVersion})` : ""}`
        : probe.error || "Manager API is not healthy";
  return (
    <Box className="manager-api-status" mt="3">
      <Flex align="center" justify="between" gap="3">
        <Box>
          <Text as="div" size="1" color="gray">
            Manager API
          </Text>
          <Text as="div" size="2" className="mono metric-value">
            {url || "pending"}
          </Text>
        </Box>
        <Badge color={tone} variant="soft">
          {label}
        </Badge>
      </Flex>
      {probe?.ok ? (
        <Grid columns="3" gap="3" mt="2">
          <Metric label="Namespace" value={probe.namespace || "unknown"} />
          <Metric label="Auth" value={probe.authEnabled ? "enabled" : "disabled"} />
          <Metric label="Director" value={probe.directorConfigured ? "configured" : "auto-detect"} />
        </Grid>
      ) : null}
    </Box>
  );
}

function EmptyState({ title, body }: { title: string; body: string }) {
  return (
    <Box className="empty-state">
      <Heading size="3">{title}</Heading>
      <Text as="p" size="2" color="gray">
        {body}
      </Text>
    </Box>
  );
}

function InstallControls({
  form,
  calculatedMemory,
  layoutPreview,
  hostReadiness,
  driveCandidates,
  networkAdapters,
  networkDetection,
  externalIp,
  environmentGate,
  setupRunning,
  vmDestinationHasVm,
  remotePreflight,
  remotePreflightStatus,
  update,
  onRemotePreflight,
  onStart,
}: {
  form: SetupForm;
  calculatedMemory: CalculatedMemory;
  layoutPreview: SetupLayoutPreview;
  hostReadiness: HostReadiness | null;
  driveCandidates: DriveCandidate[];
  networkAdapters: NetworkAdapterCandidate[];
  networkDetection: DetectionState;
  externalIp: string | null;
  environmentGate: EnvironmentGate;
  setupRunning: boolean;
  vmDestinationHasVm: boolean;
  remotePreflight: UbuntuSshPreflight | null;
  remotePreflightStatus: DetectionState;
  update: <K extends keyof SetupForm>(key: K, value: SetupForm[K]) => void;
  onRemotePreflight: () => void;
  onStart: () => void;
}) {
  const deepDesertEnabled = layoutPreview.deepDesertTotal > 0;
  const warmOptions = zeroTo(layoutPreview.deepDesertTotal);
  const requirements =
    form.setupTarget === "ubuntu"
      ? remoteSetupRequirementStatus(calculatedMemory, form.diskGb, form.processorCount, remotePreflight)
      : setupRequirementStatus(
          calculatedMemory,
          form.diskGb,
          form.processorCount,
          form.vmDestination,
          hostReadiness,
          driveCandidates,
        );
  const hasServiceToken = form.tokenSource.trim().length > 0;
  const setupIssues =
    form.setupTarget === "ubuntu"
      ? remoteSetupBlockingIssues(requirements, hasServiceToken, form, remotePreflight)
      : setupBlockingIssues(environmentGate, requirements, hasServiceToken, vmDestinationHasVm, form);
  const canStart = setupIssues.length === 0;

  return (
    <Card size="3" variant="surface" className="pane setup-pane">
      <Flex direction="column" gap="4" height="100%" minHeight="0">
        <Flex align="start" justify="between" gap="4">
          <Box>
            <Heading size="5">Server Setup</Heading>
            <Text as="p" size="2" color="gray">
              Please configure your server settings below. You'll be able to change them later.
            </Text>
          </Box>
        </Flex>

        <Box className="setup-scroll">
          <Flex direction="column" gap="5" className={setupRunning ? "setup-controls is-disabled" : "setup-controls"}>
            <SetupSection icon={DesktopIcon} title="Setup Target" className="setup-order-target">
              <Grid columns="180px 1fr" gap="3" align="center">
                <Text size="2" weight="medium">
                  Target
                </Text>
                <Select.Root
                  value={form.setupTarget}
                  onValueChange={(value) => update("setupTarget", value as SetupTarget)}
                >
                  <Select.Trigger />
                  <Select.Content>
                    <Select.Item value="hyperv">Local Windows Hyper-V</Select.Item>
                    <Select.Item value="ubuntu">Remote Ubuntu over SSH</Select.Item>
                  </Select.Content>
                </Select.Root>
              </Grid>
              {form.setupTarget === "ubuntu" ? (
                <Box className="destructive-warning" mt="3">
                  <Text as="div" size="2" weight="medium">
                    Dedicated host strongly recommended
                  </Text>
                  <Text as="p" size="2" color="gray">
                    Remote setup installs packages, creates users, configures k3s, downloads the server payload, and
                    writes service files. Do not point it at a server used for other workloads.
                  </Text>
                </Box>
              ) : null}
            </SetupSection>

            <SetupSection icon={GlobeIcon} title="World" className="setup-order-world">
              <Grid columns="2" gap="3">
                <Field label="World name">
                  <TextField.Root value={form.worldName} onChange={(event) => update("worldName", event.target.value)} />
                </Field>
                <Field label="Region">
                  <Select.Root value={form.region} onValueChange={(value) => update("region", value)}>
                    <Select.Trigger />
                    <Select.Content>
                      <Select.Item value="Europe Test">Europe Test</Select.Item>
                      <Select.Item value="North America Test">North America Test</Select.Item>
                    </Select.Content>
                  </Select.Root>
                </Field>
              </Grid>
              <Field label="Self-Host Service Token">
                <TextArea
                  placeholder="Paste your Self-Host Service Token"
                  value={form.tokenSource}
                  onChange={(event) => update("tokenSource", event.target.value)}
                />
                <Text as="p" size="2" color="gray">
                  Get the token from{" "}
                  <Link href="https://account-pts.duneawakening.com/account" target="_blank" rel="noreferrer">
                    account-pts.duneawakening.com/account
                  </Link>
                  .
                </Text>
              </Field>
            </SetupSection>

            <SetupSection icon={RocketIcon} title="World Layout" className="setup-order-layout">
              <Flex direction="column" gap="2">
                <LayoutRow label="Hagga Basin">
                  <Select.Root
                    value={form.survivalInstances}
                    onValueChange={(value) => update("survivalInstances", value)}
                  >
                    <Select.Trigger />
                    <Select.Content>
                      {oneToFour.map((value) => (
                        <Select.Item key={value} value={value}>
                          {value} {value === "1" ? "instance" : "instances"}
                        </Select.Item>
                      ))}
                    </Select.Content>
                  </Select.Root>
                </LayoutRow>
                <LayoutRow label="Social Hubs">
                  <Flex align="center" gap="3">
                    <Checkbox
                      checked={deepDesertEnabled || form.includeSocial}
                      disabled={deepDesertEnabled}
                      onCheckedChange={(value) => update("includeSocial", value === true)}
                    />
                    <Text size="2" color="gray">
                      {deepDesertEnabled ? "Required by Deep Desert" : "Optional"}
                    </Text>
                  </Flex>
                </LayoutRow>
                <LayoutRow label="Deep Desert PvE">
                  <Select.Root
                    value={form.deepDesertPveInstances}
                    onValueChange={(value) => update("deepDesertPveInstances", value)}
                  >
                    <Select.Trigger />
                    <Select.Content>
                      {zeroToOne.map((value) => (
                        <Select.Item key={value} value={value}>
                          {value} {value === "1" ? "instance" : "instances"}
                        </Select.Item>
                      ))}
                    </Select.Content>
                  </Select.Root>
                </LayoutRow>
                <LayoutRow label="Deep Desert PvP">
                  <Select.Root
                    value={form.deepDesertPvpInstances}
                    onValueChange={(value) => update("deepDesertPvpInstances", value)}
                  >
                    <Select.Trigger />
                    <Select.Content>
                      {zeroToOne.map((value) => (
                        <Select.Item key={value} value={value}>
                          {value} {value === "1" ? "instance" : "instances"}
                        </Select.Item>
                      ))}
                    </Select.Content>
                  </Select.Root>
                </LayoutRow>
                <LayoutRow label="Warm Deep Desert Instances">
                  <Select.Root
                    value={form.deepDesertWarmServers}
                    onValueChange={(value) => update("deepDesertWarmServers", value)}
                  >
                    <Select.Trigger />
                    <Select.Content>
                      {warmOptions.map((value) => (
                        <Select.Item key={value} value={value}>
                          {value === "0" ? "0, on demand" : `${value} warm`}
                        </Select.Item>
                      ))}
                    </Select.Content>
                  </Select.Root>
                </LayoutRow>
              </Flex>
            </SetupSection>

            {form.setupTarget === "hyperv" ? (
            <SetupSection icon={DesktopIcon} title="Host and VM" className="setup-order-vm">
              <Flex direction="column" gap="2">
                <FormRow label="VM Name">
                  <TextField.Root value={form.vmName} onChange={(event) => update("vmName", event.target.value)} />
                </FormRow>
                <FormRow label="VM Location">
                  <Grid columns="1fr auto" gap="2">
                    <TextField.Root
                      placeholder="Resolving default VM location..."
                      value={form.vmDestination}
                      onChange={(event) => update("vmDestination", event.target.value)}
                    />
                    <Button
                      type="button"
                      variant="surface"
                      onClick={async () => {
                        const selected = await open({
                          directory: true,
                          defaultPath: form.vmDestination || undefined,
                          multiple: false,
                          title: "Choose VM files destination",
                        });
                        if (typeof selected === "string") {
                          update("vmDestination", selected);
                        }
                      }}
                    >
                      Choose
                    </Button>
                  </Grid>
                  <InlineRequirement
                    ok={requirements.diskOk && !vmDestinationHasVm}
                    text={
                      vmDestinationHasVm
                        ? "Destination already contains VM files. Choose another folder."
                        : `${requirements.diskRequired}; ${requirements.diskAvailable}`
                    }
                  />
                </FormRow>
                <FormRow label="Disk Size">
                  <TextField.Root value={form.diskGb} onChange={(event) => update("diskGb", event.target.value)}>
                    <TextField.Slot side="right">GB</TextField.Slot>
                  </TextField.Root>
                </FormRow>
                <FormRow label="CPU Cores">
                  <TextField.Root
                    value={form.processorCount}
                    onChange={(event) => update("processorCount", event.target.value)}
                  />
                  <InlineRequirement
                    ok={requirements.processorOk}
                    text={`${requirements.processorRequired}; ${requirements.processorAvailable}`}
                  />
                </FormRow>
              </Flex>

              <Box className="memory-calculation">
                <Flex align="start" justify="between" gap="4">
                  <Box>
                    <Text as="div" size="2" weight="medium">
                      Calculated VM memory
                    </Text>
                    <Text as="div" size="2" color="gray">
                      Derived from the selected world layout.
                    </Text>
                  </Box>
                  <Text size="7" weight="bold" color="bronze">
                    {calculatedMemory.gb} GB
                  </Text>
                </Flex>
                <InlineRequirement
                  ok={requirements.memoryOk}
                  text={`${requirements.memoryRequired}; ${requirements.memoryAvailable}`}
                />
                <Separator size="4" my="3" />
                <Flex direction="column" gap="1">
                  {calculatedMemory.lines.map((line) => (
                    <Text key={line} size="2" color="gray">
                      {line}
                    </Text>
                  ))}
                </Flex>
              </Box>

              <Flex align="center" justify="between" gap="3">
                <Box>
                  <Text as="div" size="2" weight="medium">
                    Enable experimental swap
                  </Text>
                  <Text as="div" size="2" color="gray">
                    Helps large layouts fit on constrained hosts.
                  </Text>
                </Box>
                <Switch checked={form.enableSwap} onCheckedChange={(value) => update("enableSwap", value)} />
              </Flex>
            </SetupSection>
            ) : (
            <SetupSection icon={DesktopIcon} title="Remote Ubuntu Host" className="setup-order-vm">
              <Flex direction="column" gap="2">
                <FormRow label="Server IP">
                  <TextField.Root
                    placeholder="Remote public IP or hostname"
                    value={form.remoteHost}
                    onChange={(event) => update("remoteHost", event.target.value)}
                  />
                </FormRow>
                <FormRow label="SSH User">
                  <TextField.Root value={form.remoteUser} onChange={(event) => update("remoteUser", event.target.value)} />
                </FormRow>
                <FormRow label="Private Key">
                  <Grid columns="1fr auto" gap="2">
                    <TextField.Root
                      placeholder="Choose SSH private key"
                      value={form.remoteKeyPath}
                      onChange={(event) => update("remoteKeyPath", event.target.value)}
                    />
                    <Button
                      type="button"
                      variant="surface"
                      onClick={async () => {
                        const selected = await open({
                          directory: false,
                          multiple: false,
                          title: "Choose SSH private key",
                        });
                        if (typeof selected === "string") {
                          update("remoteKeyPath", selected);
                        }
                      }}
                    >
                      Choose
                    </Button>
                  </Grid>
                </FormRow>
                <FormRow label="Save Server">
                  <Flex align="center" gap="3">
                    <Checkbox
                      checked={form.saveRemoteServer}
                      onCheckedChange={(value) => update("saveRemoteServer", value === true)}
                    />
                    <Text size="2" color="gray">
                      Add this remote Ubuntu server to Home when setup starts
                    </Text>
                  </Flex>
                </FormRow>
                <FormRow label="Disk Budget">
                  <TextField.Root value={form.diskGb} onChange={(event) => update("diskGb", event.target.value)}>
                    <TextField.Slot side="right">GB</TextField.Slot>
                  </TextField.Root>
                  <InlineRequirement ok={requirements.diskOk} text={`${requirements.diskRequired}; ${requirements.diskAvailable}`} />
                </FormRow>
                <FormRow label="CPU Check">
                  <TextField.Root
                    value={form.processorCount}
                    onChange={(event) => update("processorCount", event.target.value)}
                  />
                  <InlineRequirement
                    ok={requirements.processorOk}
                    text={`${requirements.processorRequired}; ${requirements.processorAvailable}`}
                  />
                </FormRow>
              </Flex>

              <Box className="memory-calculation">
                <Flex align="start" justify="between" gap="4">
                  <Box>
                    <Text as="div" size="2" weight="medium">
                      Required memory
                    </Text>
                    <Text as="div" size="2" color="gray">
                      Queried from the remote host during preflight.
                    </Text>
                  </Box>
                  <Text size="7" weight="bold" color="bronze">
                    {calculatedMemory.gb} GB
                  </Text>
                </Flex>
                <InlineRequirement
                  ok={requirements.memoryOk}
                  text={`${requirements.memoryRequired}; ${requirements.memoryAvailable}`}
                />
                <Separator size="4" my="3" />
                <Flex direction="column" gap="1">
                  {calculatedMemory.lines.map((line) => (
                    <Text key={line} size="2" color="gray">
                      {line}
                    </Text>
                  ))}
                </Flex>
              </Box>

              {remotePreflight ? <RemotePreflightSummary preflight={remotePreflight} /> : null}
              <Button
                type="button"
                variant="surface"
                onClick={onRemotePreflight}
                disabled={!form.remoteHost.trim() || !form.remoteUser.trim() || !form.remoteKeyPath.trim()}
              >
                {remotePreflightStatus === "detecting" && remotePreflight ? "Refresh remote resources" : "Detect remote resources"}
              </Button>
            </SetupSection>
            )}

            {form.setupTarget === "hyperv" ? (
            <SetupSection icon={MixIcon} title="Network" className="setup-order-network">
              <Field label="Network mode">
                <Select.Root
                  value={form.networkMode}
                  onValueChange={(value) => update("networkMode", value as NetworkMode)}
                >
                  <Select.Trigger />
                  <Select.Content>
                    <Select.Item value="static">Static internal IP</Select.Item>
                    <Select.Item value="dhcp">DHCP</Select.Item>
                  </Select.Content>
                </Select.Root>
              </Field>
              <Field label="Host network adapter">
                <Select.Root
                  value={form.adapterName || undefined}
                  onValueChange={(value) => {
                    const adapter = networkAdapters.find((candidate) => candidate.name === value);
                    if (!adapter) return;
                    update("adapterName", value);
                    update("switchName", adapter.existingExternalSwitch || adapter.name);
                    update("staticIp", adapter.suggestedIpv4Address);
                    update(
                      "playerIp",
                      form.playerIpMode === "external" && externalIp ? externalIp : adapter.suggestedIpv4Address,
                    );
                    update("gateway", adapter.gateway);
                  }}
                >
                  <Select.Trigger placeholder={networkStatusLabel(networkDetection)} />
                  <Select.Content>
                    {networkAdapters.map((adapter) => (
                      <Select.Item key={adapter.name} value={adapter.name}>
                        {adapter.name} - {adapter.ipv4Address}/{adapter.prefixLength}
                      </Select.Item>
                    ))}
                  </Select.Content>
                </Select.Root>
              </Field>
              <Field label="Hyper-V switch">
                <TextField.Root
                  placeholder="Detected from adapter"
                  value={form.switchName}
                  onChange={(event) => update("switchName", event.target.value)}
                />
              </Field>
              <Grid columns="3" gap="3">
                <Field label="VM IP">
                  <TextField.Root
                    placeholder="Detected suggestion"
                    value={form.staticIp}
                    onChange={(event) => update("staticIp", event.target.value)}
                  />
                </Field>
                <Field label="Gateway">
                  <TextField.Root
                    placeholder="Detected gateway"
                    value={form.gateway}
                    onChange={(event) => update("gateway", event.target.value)}
                  />
                </Field>
                <Field label="DNS">
                  <TextField.Root value={form.dns} onChange={(event) => update("dns", event.target.value)} />
                </Field>
              </Grid>
              <Field label="Player-facing IP">
                <Grid columns="160px 1fr" gap="3">
                  <Select.Root
                    value={form.playerIpMode}
                    onValueChange={(value) => {
                      const mode = value as PlayerIpMode;
                      update("playerIpMode", mode);
                      update("playerIp", mode === "external" ? externalIp || "" : form.staticIp);
                    }}
                  >
                    <Select.Trigger />
                    <Select.Content>
                      <Select.Item value="local">Local IP</Select.Item>
                      <Select.Item value="external">External IP</Select.Item>
                    </Select.Content>
                  </Select.Root>
                  <TextField.Root
                    placeholder={form.playerIpMode === "external" ? "Detected external IP" : "Same as VM IP for LAN"}
                    value={form.playerIp}
                    onChange={(event) => update("playerIp", event.target.value)}
                  />
                </Grid>
              </Field>
              {form.playerIpMode === "external" ? <PortForwardingNotice /> : null}
            </SetupSection>
            ) : (
            <SetupSection icon={MixIcon} title="Network" className="setup-order-network">
              <Field label="Player-facing IP">
                <Grid columns="160px 1fr" gap="3">
                  <Select.Root
                    value={form.playerIpMode}
                    onValueChange={(value) => {
                      const mode = value as PlayerIpMode;
                      update("playerIpMode", mode);
                      update(
                        "playerIp",
                        mode === "external"
                          ? remotePreflight?.publicIp || form.remoteHost
                          : remotePreflight?.ipv4Addresses[0] || form.remoteHost,
                      );
                    }}
                  >
                    <Select.Trigger />
                    <Select.Content>
                      <Select.Item value="local">Local IP</Select.Item>
                      <Select.Item value="external">External IP</Select.Item>
                    </Select.Content>
                  </Select.Root>
                  <TextField.Root
                    placeholder="Address players use to connect"
                    value={form.playerIp}
                    onChange={(event) => update("playerIp", event.target.value)}
                  />
                </Grid>
              </Field>
              {form.playerIpMode === "external" ? <PortForwardingNotice /> : null}
            </SetupSection>
            )}

          </Flex>
        </Box>

        <Separator size="4" />

        <Flex align="center" justify="between" gap="3">
          <Box className="setup-readiness">
            {setupRunning ? null : canStart ? (
              <Text size="2" color="gray">
                Ready to create one full setup plan.
              </Text>
            ) : (
              <ul className="setup-issues">
                {setupIssues.map((issue) => (
                  <li key={issue}>{issue}</li>
                ))}
              </ul>
            )}
          </Box>
          <Button size="3" onClick={onStart} disabled={!canStart || setupRunning}>
            <LightningBoltIcon /> {setupRunning ? "Setup running..." : "Start full setup"}
          </Button>
        </Flex>
      </Flex>
    </Card>
  );
}

type CalculatedMemory = {
  gb: number;
  lines: string[];
};

type SetupLayoutPreview = {
  survivalDimensions: string;
  deepDesertTotal: number;
  deepDesertPvp: number;
};

type SetupRequirements = {
  canContinue: boolean;
  memoryOk: boolean;
  processorOk: boolean;
  diskOk: boolean;
  memoryRequired: string;
  memoryAvailable: string;
  processorRequired: string;
  processorAvailable: string;
  diskRequired: string;
  diskAvailable: string;
};

function setupLayoutPreview(form: SetupForm): SetupLayoutPreview {
  const survivalInstances = Math.max(1, parsePositiveInt(form.survivalInstances));
  const deepDesertPve = parsePositiveInt(form.deepDesertPveInstances);
  const deepDesertPvp = parsePositiveInt(form.deepDesertPvpInstances);
  const deepDesertTotal = deepDesertPve + deepDesertPvp;
  const survivalDimensions = Array.from({ length: survivalInstances }, (_, index) => index).join(", ");

  return {
    survivalDimensions,
    deepDesertTotal,
    deepDesertPvp,
  };
}

function setupRunRequest(form: SetupForm, memoryGb: number): SetupRunRequest {
  return {
    vmDestination: form.vmDestination,
    vmName: form.vmName,
    diskGb: parsePositiveInt(form.diskGb),
    memoryGb,
    processorCount: parsePositiveInt(form.processorCount),
    enableSwap: form.enableSwap,
    networkMode: form.networkMode,
    switchName: form.switchName,
    adapterName: form.adapterName,
    staticIp: form.staticIp,
    gateway: form.gateway,
    dns: form.dns,
    playerIp: form.playerIp,
    worldName: form.worldName,
    region: form.region,
    selfHostToken: form.tokenSource,
    survivalInstances: Math.max(1, parsePositiveInt(form.survivalInstances)),
    deepDesertPveInstances: parsePositiveInt(form.deepDesertPveInstances),
    deepDesertPvpInstances: parsePositiveInt(form.deepDesertPvpInstances),
    deepDesertWarmServers: parsePositiveInt(form.deepDesertWarmServers),
  };
}

function remoteSetupRunRequest(form: SetupForm): RemoteSetupRunRequest {
  return {
    host: form.remoteHost.trim(),
    user: form.remoteUser.trim() || "root",
    keyPath: form.remoteKeyPath.trim(),
    playerIp: form.playerIp.trim(),
    worldName: form.worldName,
    region: form.region,
    selfHostToken: form.tokenSource,
    survivalInstances: Math.max(1, parsePositiveInt(form.survivalInstances)),
    deepDesertPveInstances: parsePositiveInt(form.deepDesertPveInstances),
    deepDesertPvpInstances: parsePositiveInt(form.deepDesertPvpInstances),
    deepDesertWarmServers: parsePositiveInt(form.deepDesertWarmServers),
    enableSwap: form.enableSwap,
  };
}

function remoteServerDraftFromForm(form: SetupForm): RemoteServerRecord {
  const host = form.remoteHost.trim();
  const id = `ubuntu:${host || "pending"}:${Date.now()}`;
  return {
    id,
    name: form.worldName.trim() || host || "Remote Ubuntu Server",
    host,
    user: form.remoteUser.trim() || "root",
    keyPath: form.remoteKeyPath.trim(),
    namespace: "",
    battlegroupName: "",
    worldUniqueName: "",
    managerApiUrl: host ? `http://${host}:8787` : "",
    phase: "Setup running",
    createdAt: new Date().toISOString(),
  };
}

function remoteServerRecordFromSetup(
  form: SetupForm,
  result: RemoteSetupRunResult,
  existingId?: string,
): RemoteServerRecord {
  const host = form.remoteHost.trim();
  return {
    id: existingId || `ubuntu:${host}:${result.namespace}:${result.battlegroupName}`,
    name: form.worldName.trim() || result.battlegroupName,
    host,
    user: form.remoteUser.trim() || "root",
    keyPath: form.remoteKeyPath.trim(),
    namespace: result.namespace,
    battlegroupName: result.battlegroupName,
    worldUniqueName: result.worldUniqueName,
    managerApiUrl: result.managerApiUrl || `http://${host}:8787`,
    phase: "Ready",
    createdAt: new Date().toISOString(),
  };
}

function remoteServerActionRequest(server: RemoteServerRecord) {
  return {
    host: server.host,
    user: server.user || "root",
    keyPath: server.keyPath,
    namespace: server.namespace,
    battlegroupName: server.battlegroupName,
  };
}

function remoteManagerApiActionRequest(server: RemoteServerRecord) {
  return {
    host: server.host,
    user: server.user || "root",
    keyPath: server.keyPath,
  };
}

function upsertRemoteServer(servers: RemoteServerRecord[], record: RemoteServerRecord): RemoteServerRecord[] {
  const index = servers.findIndex((server) => server.id === record.id);
  if (index === -1) {
    return [...servers, record];
  }
  const next = [...servers];
  next[index] = { ...next[index], ...record };
  return next;
}

function mergeRemoteServers(
  servers: RemoteServerRecord[],
  records: RemoteServerRecord[],
): RemoteServerRecord[] {
  return records.reduce((next, record) => upsertRemoteServer(next, record), servers);
}

function readRemoteServers(): RemoteServerRecord[] {
  const text = window.localStorage.getItem(remoteServersStorageKey);
  if (!text) return [];
  try {
    const value = JSON.parse(text);
    if (!Array.isArray(value)) return [];
    return value.filter(isRemoteServerRecord);
  } catch {
    window.localStorage.removeItem(remoteServersStorageKey);
    return [];
  }
}

function persistRemoteServers(servers: RemoteServerRecord[]): RemoteServerRecord[] {
  window.localStorage.setItem(remoteServersStorageKey, JSON.stringify(servers));
  return servers;
}

function isRemoteServerRecord(value: unknown): value is RemoteServerRecord {
  if (!value || typeof value !== "object") return false;
  const record = value as Partial<RemoteServerRecord>;
  return typeof record.id === "string" && typeof record.host === "string" && typeof record.name === "string";
}

function managerApiKeyForVm(candidate: DuneVmCandidate): string {
  return `vm:${candidate.vm.name}`;
}

function managerApiUrlForVm(candidate: DuneVmCandidate): string {
  const ip = candidate.vm.ipv4Addresses[0];
  return ip ? `http://${ip}:8787` : "";
}

function managerApiKeyForRemote(server: RemoteServerRecord): string {
  return `remote:${server.id}`;
}

function managerApiProbeError(url: string, error: string): ManagerApiProbe {
  return {
    url,
    reachable: false,
    ok: false,
    apiVersion: "",
    namespace: "",
    authEnabled: false,
    directorConfigured: false,
    error,
  };
}

function isBattlegroupStarted(status: RemoteBattlegroupStatus): boolean {
  return (
    !status.stop &&
    isStartedPhase(status.phase) &&
    isStartedPhase(status.serverGroupPhase) &&
    isDirectorReadyPhase(status.directorPhase)
  );
}

function isStartedPhase(phase: string): boolean {
  return ["running", "ready", "healthy", "available", "reconciling"].includes(
    phase.trim().toLowerCase(),
  );
}

function isDirectorReadyPhase(phase: string): boolean {
  const normalized = phase.trim().toLowerCase();
  return normalized.length === 0 || isStartedPhase(normalized);
}

function omitKey<T>(record: Record<string, T>, key: string): Record<string, T> {
  const { [key]: _removed, ...rest } = record;
  return rest;
}

function rollbackRequestFromSetup(request: SetupRunRequest): RollbackRequest {
  return {
    vmName: request.vmName,
    vmDestination: request.vmDestination,
    switchName: request.switchName,
  };
}

function remoteSetupRequirementStatus(
  calculatedMemory: CalculatedMemory,
  diskGb: string,
  processorCount: string,
  preflight: UbuntuSshPreflight | null,
): SetupRequirements {
  const requiredMemoryBytes = calculatedMemory.gb * 1024 * 1024 * 1024;
  const requiredProcessors = Math.max(0, parsePositiveInt(processorCount));
  const requiredDiskGb = Math.max(0, parsePositiveInt(diskGb));
  const requiredDiskBytes = requiredDiskGb * 1024 * 1024 * 1024;
  const memoryAvailable = preflight?.availableMemoryBytes ?? 0;
  const processorsAvailable = preflight?.logicalProcessorCount ?? 0;
  const diskAvailable = preflight?.rootDiskAvailableBytes ?? 0;

  return {
    canContinue:
      !!preflight &&
      memoryAvailable >= requiredMemoryBytes &&
      requiredProcessors > 0 &&
      requiredProcessors <= processorsAvailable &&
      diskAvailable >= requiredDiskBytes,
    memoryOk: !!preflight && memoryAvailable >= requiredMemoryBytes,
    processorOk: !!preflight && requiredProcessors > 0 && requiredProcessors <= processorsAvailable,
    diskOk: !!preflight && diskAvailable >= requiredDiskBytes,
    memoryRequired: `${calculatedMemory.gb} GB required`,
    memoryAvailable: preflight ? `${formatGiB(memoryAvailable)} available` : "Run remote detection",
    processorRequired: `${requiredProcessors || "A positive number of"} cores requested`,
    processorAvailable: preflight ? `${processorsAvailable} logical available` : "Run remote detection",
    diskRequired: `${requiredDiskGb} GB required`,
    diskAvailable: preflight ? `${formatGiB(diskAvailable)} free on /` : "Run remote detection",
  };
}

function remoteSetupBlockingIssues(
  requirements: SetupRequirements,
  hasServiceToken: boolean,
  form: SetupForm,
  preflight: UbuntuSshPreflight | null,
): string[] {
  const issues: string[] = [];
  if (!form.remoteHost.trim()) issues.push("Remote server IP is required.");
  if (!form.remoteUser.trim()) issues.push("SSH user is required.");
  if (!form.remoteKeyPath.trim()) issues.push("SSH private key file is required.");
  if (!preflight) issues.push("Run remote resource detection before setup.");
  if (preflight && preflight.osId !== "ubuntu") issues.push("Remote host must be Ubuntu.");
  if (preflight && preflight.uid !== 0 && !preflight.passwordlessSudo) {
    issues.push("Remote setup requires root login or passwordless sudo.");
  }
  if (preflight && !preflight.systemdAvailable) issues.push("Remote host must support systemd.");
  if (!requirements.memoryOk) {
    issues.push(`Memory: ${requirements.memoryRequired}; ${requirements.memoryAvailable}.`);
  }
  if (!requirements.processorOk) {
    issues.push(`CPU Cores: ${requirements.processorRequired}; ${requirements.processorAvailable}.`);
  }
  if (!requirements.diskOk) {
    issues.push(`Disk: ${requirements.diskRequired}; ${requirements.diskAvailable}.`);
  }
  if (!form.playerIp.trim()) issues.push("Player-facing IP is required.");
  if (parsePositiveInt(form.deepDesertWarmServers) > 0) {
    issues.push("Warm Deep Desert Instances are not wired yet; set them to 0 for this build.");
  }
  if (deepDesertInstanceCount(form) > 1) {
    issues.push("Only one Deep Desert instance is supported in this build.");
  }
  if (!hasServiceToken) issues.push("Self-Host Service Token is required.");
  return issues;
}

function calculateRequiredMemory(form: SetupForm): CalculatedMemory {
  const survivalInstances = Math.max(1, parsePositiveInt(form.survivalInstances));
  const deepDesertInstances =
    parsePositiveInt(form.deepDesertPveInstances) + parsePositiveInt(form.deepDesertPvpInstances);
  const survivalGb = survivalInstances * 20;
  const socialGb = form.includeSocial || deepDesertInstances > 0 ? 10 : 0;
  const deepDesertGb = deepDesertInstances * 10;
  const gb = survivalGb + socialGb + deepDesertGb;
  const lines = [
    `${survivalInstances} Hagga Basin ${survivalInstances === 1 ? "instance" : "instances"} x 20 GB = ${survivalGb} GB`,
  ];

  if (form.includeSocial || deepDesertInstances > 0) {
    lines.push("Social Hubs = 10 GB");
  }
  if (deepDesertInstances > 0) {
    lines.push(
      `${deepDesertInstances} Deep Desert ${
        deepDesertInstances === 1 ? "instance" : "instances"
      } x 10 GB = ${deepDesertGb} GB`,
    );
  }

  return { gb, lines };
}

function normalizeSetupForm(form: SetupForm): SetupForm {
  const deepDesertPve = parsePositiveInt(form.deepDesertPveInstances);
  const deepDesertPvp = parsePositiveInt(form.deepDesertPvpInstances);
  const deepDesertInstances = deepDesertPve + deepDesertPvp;
  const warmServers = Math.min(parsePositiveInt(form.deepDesertWarmServers), deepDesertInstances);
  const normalized = {
    ...form,
    includeSocial: deepDesertInstances > 0 ? true : form.includeSocial,
    deepDesertPveInstances: deepDesertPve > 0 ? "1" : "0",
    deepDesertPvpInstances: deepDesertPve > 0 ? "0" : deepDesertPvp > 0 ? "1" : "0",
    deepDesertWarmServers: String(warmServers),
  };
  if (normalized.playerIpMode === "local" && normalized.staticIp && normalized.playerIp !== normalized.staticIp) {
    return { ...normalized, playerIp: normalized.staticIp };
  }
  return normalized;
}

function deepDesertInstanceCount(form: SetupForm): number {
  return parsePositiveInt(form.deepDesertPveInstances) + parsePositiveInt(form.deepDesertPvpInstances);
}

function parsePositiveInt(value: string): number {
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
}

function zeroTo(max: number): string[] {
  return Array.from({ length: Math.max(0, max) + 1 }, (_, index) => String(index));
}

function environmentLogRows(
  status: DetectionState,
  readiness: HostReadiness | null,
  adapters: NetworkAdapterCandidate[],
  drives: DriveCandidate[],
  externalIp: string | null,
  gate: EnvironmentGate,
): LogRow[] {
  if (status === "detecting") {
    return [
      log.debug("env", "Checking administrator privileges..."),
      log.debug("env", "Checking virtualization firmware, Hyper-V, and vmms service..."),
      log.debug("env", "Waiting to detect host networking after host checks complete..."),
    ];
  }
  if (status === "failed") {
    return [log.error("env", "Environment detection failed. Network fields can still be filled manually.")];
  }
  const rows: LogRow[] = [];
  if (readiness) {
    rows.push(
      readiness.elevated
        ? log.info("env", "Administrator privileges detected.")
        : log.warn("env", "This app is not elevated; restart it as administrator to continue setup."),
    );
    rows.push(
      readiness.virtualizationFirmwareEnabled === false
        ? log.warn("env", "Virtualization firmware is disabled or unavailable.")
        : log.info("env", "Hyper-V virtualization support is operational."),
    );
    rows.push(
      readiness.hypervAvailable && readiness.vmmsRunning
        ? log.info("env", "Hyper-V available; vmms service running.")
        : log.warn(
            "env",
            `Hyper-V ${readiness.hypervAvailable ? "available" : "missing"}; vmms service ${
              readiness.vmmsRunning ? "running" : "not running"
            }.`,
          ),
    );
    rows.push(
      log.info(
        "env",
        `Physical memory: ${formatGiB(readiness.availablePhysicalMemoryBytes)} available of ${formatGiB(readiness.totalPhysicalMemoryBytes)} total.`,
      ),
    );
    rows.push(
      log.info(
        "env",
        `CPU cores: ${readiness.logicalProcessorCount || "unknown"} logical processors detected.`,
      ),
    );
  }
  if (drives.length > 0) {
    rows.push(
      log.debug(
        "env",
        `Detected drives: ${drives
          .map((drive) => `${drive.root} ${formatGiB(drive.freeBytes)} free`)
          .join(", ")}.`,
      ),
    );
  }
  rows.push(
    externalIp
      ? log.info("env", `Detected external IP ${externalIp}.`)
      : log.warn("env", "External IP was not detected; it can be entered manually."),
  );
  if (adapters.length === 0) {
    rows.push(log.warn("env", "No active physical adapters with IPv4 gateway were detected."));
    return rows;
  }

  rows.push(
    ...adapters.map((adapter) =>
      log.info(
        "env",
        `Detected ${adapter.name}: ${adapter.ipv4Address}/${adapter.prefixLength}, gateway ${adapter.gateway}, suggested VM IP ${adapter.suggestedIpv4Address || "unavailable"}.`,
      ),
    ),
  );
  if (!gate.canContinue) {
    rows.push(...gate.reasons.map((reason) => log.error("env", reason)));
  }
  return rows;
}

function setupEnvironmentGate(
  status: DetectionState,
  readiness: HostReadiness | null,
  adapters: NetworkAdapterCandidate[],
): EnvironmentGate {
  const reasons: string[] = [];
  if (status !== "ready") {
    reasons.push("Environment detection has not completed.");
  }
  if (!readiness) {
    reasons.push("Host readiness was not detected.");
  } else {
    if (!readiness.elevated) {
      reasons.push("Restart the app as administrator to continue setup.");
    }
    if (readiness.virtualizationFirmwareEnabled === false) {
      reasons.push("Hyper-V virtualization support is not operational.");
    }
    if (!readiness.hypervAvailable) {
      reasons.push("Hyper-V PowerShell support is missing.");
    }
    if (!readiness.vmmsRunning) {
      reasons.push("Hyper-V vmms service is not running.");
    }
  }
  if (adapters.length === 0) {
    reasons.push("A physical network adapter with IPv4 and gateway is required.");
  }
  return {
    canContinue: reasons.length === 0,
    reasons,
  };
}

function setupRequirementStatus(
  calculatedMemory: CalculatedMemory,
  diskGb: string,
  processorCount: string,
  vmDestination: string,
  readiness: HostReadiness | null,
  drives: DriveCandidate[],
): SetupRequirements {
  const requiredMemoryBytes = calculatedMemory.gb * 1024 * 1024 * 1024;
  const requiredProcessors = Math.max(0, parsePositiveInt(processorCount));
  const requiredDiskGb = Math.max(0, parsePositiveInt(diskGb));
  const requiredDiskBytes = requiredDiskGb * 1024 * 1024 * 1024;
  const memoryAvailable = readiness?.availablePhysicalMemoryBytes ?? 0;
  const processorsAvailable = readiness?.logicalProcessorCount ?? 0;
  const memoryOk = memoryAvailable >= requiredMemoryBytes;
  const processorOk =
    requiredProcessors > 0 && (processorsAvailable === 0 || requiredProcessors <= processorsAvailable);
  const destinationDrive = findDriveForPath(vmDestination, drives);
  const diskOk = destinationDrive ? destinationDrive.freeBytes >= requiredDiskBytes : false;

  return {
    canContinue: memoryOk && processorOk && diskOk,
    memoryOk,
    processorOk,
    diskOk,
    memoryRequired: `${calculatedMemory.gb} GB required`,
    memoryAvailable: readiness ? `${formatGiB(memoryAvailable)} available` : "Detecting",
    processorRequired: `${requiredProcessors || "A positive number of"} cores requested`,
    processorAvailable: readiness
      ? processorsAvailable
        ? `${processorsAvailable} logical available`
        : "Host CPU count unavailable"
      : "Detecting",
    diskRequired: `${requiredDiskGb} GB required`,
    diskAvailable: destinationDrive
      ? `${destinationDrive.root} has ${formatGiB(destinationDrive.freeBytes)} free`
      : "Choose a VM destination folder",
  };
}

function findDriveForPath(path: string, drives: DriveCandidate[]): DriveCandidate | null {
  const normalizedPath = path.trim().replace(/\//g, "\\").toUpperCase();
  if (!/^[A-Z]:\\/.test(normalizedPath)) {
    return null;
  }

  return (
    drives.find((drive) => {
      const root = drive.root.trim().replace(/\//g, "\\").toUpperCase();
      return normalizedPath.startsWith(root);
    }) ?? null
  );
}

function setupBlockingIssues(
  gate: EnvironmentGate,
  requirements: SetupRequirements,
  hasServiceToken: boolean,
  vmDestinationHasVm: boolean,
  form: SetupForm,
): string[] {
  const issues = [...gate.reasons];
  if (!requirements.memoryOk) {
    issues.push(`Memory: ${requirements.memoryRequired}; ${requirements.memoryAvailable}.`);
  }
  if (!requirements.processorOk) {
    issues.push(`CPU Cores: ${requirements.processorRequired}; ${requirements.processorAvailable}.`);
  }
  if (!requirements.diskOk) {
    issues.push(`VM Location: ${requirements.diskRequired}; ${requirements.diskAvailable}.`);
  }
  if (vmDestinationHasVm) {
    issues.push("VM Location already contains VM files. Choose another folder.");
  }
  if (parsePositiveInt(form.deepDesertWarmServers) > 0) {
    issues.push("Warm Deep Desert Instances are not wired yet; set them to 0 for this build.");
  }
  if (deepDesertInstanceCount(form) > 1) {
    issues.push("Only one Deep Desert instance is supported in this build.");
  }
  if (!hasServiceToken) {
    issues.push("Self-Host Service Token is required.");
  }
  return issues;
}

function LayoutRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Grid columns="minmax(180px, 1fr) 210px" gap="3" align="center" className="layout-row">
      <Text size="2" weight="medium">
        {label}
      </Text>
      <Box>{children}</Box>
    </Grid>
  );
}

function FormRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Grid columns="130px minmax(0, 1fr)" gap="3" align="start" className="form-row">
      <Text size="2" weight="medium" mt="2">
        {label}
      </Text>
      <Box>{children}</Box>
    </Grid>
  );
}

function InlineRequirement({ ok, text }: { ok: boolean; text: string }) {
  return (
    <Flex align="center" gap="2" mt="2">
      <Badge color={ok ? "green" : "amber"} variant="soft">
        {ok ? "Enough" : "Needs attention"}
      </Badge>
      <Text size="2" color="gray">
        {text}
      </Text>
    </Flex>
  );
}

function RemotePreflightSummary({ preflight }: { preflight: UbuntuSshPreflight }) {
  const rows = [
    ["Host", `${preflight.hostname} (${preflight.osPrettyName})`],
    ["Public IP", preflight.publicIp || "Not detected"],
    ["Private IPs", preflight.ipv4Addresses.length ? preflight.ipv4Addresses.join(", ") : "None detected"],
    ["Memory", `${formatGiB(preflight.availableMemoryBytes)} available of ${formatGiB(preflight.totalMemoryBytes)}`],
    ["Disk", `${formatGiB(preflight.rootDiskAvailableBytes)} free of ${formatGiB(preflight.rootDiskTotalBytes)} on /`],
    ["CPU", `${preflight.logicalProcessorCount} logical processors`],
    ["Access", preflight.uid === 0 ? "root" : preflight.passwordlessSudo ? "passwordless sudo" : "limited"],
    ["Existing tools", `SteamCMD ${preflight.steamcmdInstalled ? "present" : "missing"}, k3s ${preflight.k3sInstalled ? "present" : "missing"}`],
  ];
  return (
    <Box className="info-card">
      {rows.map(([label, value]) => (
        <InfoRow key={label} label={label} value={value} tone="green" />
      ))}
    </Box>
  );
}

function PortForwardingNotice() {
  return (
    <Box className="port-forwarding">
      <Text as="div" size="2" weight="medium">
        Port forwarding required
      </Text>
      <Text as="p" size="2" color="gray">
        Forward these ports from your router to the VM IP when players connect through the external IP.
      </Text>
      <Flex direction="column" gap="2">
        {playerPortForwards.map((entry) => (
          <Grid key={`${entry.ports}-${entry.protocol}`} columns="120px 70px 1fr" gap="3">
            <Text size="2" className="mono">
              {entry.ports}
            </Text>
            <Badge color={entry.protocol === "UDP" ? "bronze" : "gray"} variant="surface">
              {entry.protocol}
            </Badge>
            <Text size="2" color="gray">
              {entry.purpose}
            </Text>
          </Grid>
        ))}
      </Flex>
    </Box>
  );
}

function logEntry(level: LogLevel, scope: string, message: string): LogRow {
  return {
    id: nextLogRowId++,
    timestamp: new Date().toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    }),
    level,
    scope,
    message,
  };
}

function filterLogRows(rows: LogRow[], minimum: LogLevelFilter): LogRow[] {
  const rank: Record<LogLevel, number> = {
    debug: 0,
    info: 1,
    warn: 2,
    error: 3,
  };
  return rows.filter((row) => rank[row.level] >= rank[minimum]);
}

function updateLabel(status: UpdateStatus, availableUpdate: Update | null, progress: string | null): string {
  if (status === "checking") return "Checking";
  if (status === "installing") return progress ?? "Installing";
  if (status === "relaunching") return progress ?? "Relaunching";
  if (status === "failed") return "Check failed";
  if (availableUpdate) return `${availableUpdate.version} available`;
  if (status === "current") return "Up to date";
  return "Not checked";
}

function updateTone(status: UpdateStatus): "green" | "amber" | "red" {
  if (status === "failed") return "red";
  if (status === "current") return "green";
  return "amber";
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return "Operation failed.";
}

async function openFileDialog(title: string): Promise<string | null> {
  const selected = await open({
    directory: false,
    multiple: false,
    title,
  });
  return typeof selected === "string" ? selected : null;
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "unknown";
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${Math.round(bytes / 1024 / 1024)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function networkStatusLabel(status: DetectionState): string {
  if (status === "detecting") return "Detecting adapters...";
  if (status === "failed") return "Detection failed";
  return "Choose adapter";
}

function formatGiB(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "unknown";
  return `${Math.round(bytes / 1024 / 1024 / 1024)} GB`;
}

function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "00:00:00";
  const total = Math.floor(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  return [hours, minutes, secs].map((value) => String(value).padStart(2, "0")).join(":");
}

function SetupSection({
  className,
  icon: Icon,
  title,
  children,
}: {
  className?: string;
  icon: ComponentType<{ width?: number | string; height?: number | string }>;
  title: string;
  children: ReactNode;
}) {
  return (
    <Box className={["setup-section", className].filter(Boolean).join(" ")}>
      <Flex align="center" gap="2" mb="3">
        <Icon width="17" height="17" />
        <Heading size="3">{title}</Heading>
      </Flex>
      <Flex direction="column" gap="3">
        {children}
      </Flex>
    </Box>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Box>
      <Text as="label" size="2" weight="medium" mb="1" className="field-label">
        {label}
      </Text>
      {children}
    </Box>
  );
}

function LogWindow({
  rows,
  level,
  onLevelChange,
}: {
  rows: LogRow[];
  level: LogLevelFilter;
  onLevelChange: (level: LogLevelFilter) => void;
}) {
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const stickToBottomRef = useRef(true);

  useLayoutEffect(() => {
    const body = bodyRef.current;
    if (!body) return;
    if (stickToBottomRef.current) {
      body.scrollTop = body.scrollHeight;
    }
  }, [rows]);

  return (
    <Card size="3" variant="surface" className="pane">
      <Flex direction="column" height="100%" minHeight="0">
        <Flex align="center" justify="between" gap="3" mb="3">
          <Text size="2" color="gray">
            Showing {rows.length} entries
          </Text>
          <Select.Root value={level} onValueChange={(value) => onLevelChange(value as LogLevelFilter)}>
            <Select.Trigger aria-label="Minimum log level" />
            <Select.Content>
              <Select.Item value="debug">Debug</Select.Item>
              <Select.Item value="info">Info</Select.Item>
              <Select.Item value="warn">Warn</Select.Item>
              <Select.Item value="error">Error</Select.Item>
            </Select.Content>
          </Select.Root>
        </Flex>
        <Box
          className="log-body"
          ref={bodyRef}
          onScroll={(event) => {
            const body = event.currentTarget;
            const distanceFromBottom = body.scrollHeight - body.scrollTop - body.clientHeight;
            stickToBottomRef.current = distanceFromBottom < 80;
          }}
        >
          <Flex direction="column" gap="0">
            {rows.map((row) => (
              <Grid
                key={row.id}
                columns="96px 58px 62px 1fr"
                gap="3"
                align="center"
                className={`log-line log-${row.level}`}
              >
                <Text size="2" color="gray" className="mono log-meta">
                  {row.timestamp}
                </Text>
                <Text size="2" className="mono log-meta log-level">
                  {row.level}
                </Text>
                <Text size="2" color="gray" className="mono log-meta">
                  {row.scope}
                </Text>
                <Text size="2" className="mono">
                  {row.message}
                </Text>
              </Grid>
            ))}
          </Flex>
        </Box>
      </Flex>
    </Card>
  );
}

function RollbackDialog({
  open,
  rollbackRunning,
  onOpenChange,
  onRollback,
}: {
  open: boolean;
  rollbackRunning: boolean;
  onOpenChange: (open: boolean) => void;
  onRollback: () => void;
}) {
  return (
    <AlertDialog.Root open={open} onOpenChange={onOpenChange}>
      <AlertDialog.Content maxWidth="460px">
        <AlertDialog.Title>Rollback setup artifacts?</AlertDialog.Title>
        <AlertDialog.Description size="2">
          Setup failed after creating or touching host resources. Rollback removes the selected VM,
          removes VM files when they look like manager-created VM files, and removes the Hyper-V
          switch only if no other VMs use it.
        </AlertDialog.Description>
        <Flex gap="3" mt="4" justify="end">
          <AlertDialog.Cancel disabled={rollbackRunning}>Keep artifacts</AlertDialog.Cancel>
          <AlertDialog.Action disabled={rollbackRunning} onClick={onRollback}>
            {rollbackRunning ? "Rolling back..." : "Rollback"}
          </AlertDialog.Action>
        </Flex>
      </AlertDialog.Content>
    </AlertDialog.Root>
  );
}

function RemoteAttachDialog({
  open,
  form,
  running,
  onOpenChange,
  onChange,
  onAttach,
}: {
  open: boolean;
  form: RemoteAttachForm;
  running: boolean;
  onOpenChange: (open: boolean) => void;
  onChange: (form: RemoteAttachForm) => void;
  onAttach: () => void;
}) {
  const canAttach = form.host.trim().length > 0 && form.keyPath.trim().length > 0 && !running;
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Content maxWidth="520px">
        <Dialog.Title>Add Remote Ubuntu Server</Dialog.Title>
        <Dialog.Description size="2" color="gray">
          Connect over SSH and detect existing Dune battlegroups. This does not provision or modify the server.
        </Dialog.Description>
        <Flex direction="column" gap="3" mt="4">
          <Field label="Host or IP">
            <TextField.Root
              placeholder="159.69.146.71"
              value={form.host}
              onChange={(event) => onChange({ ...form, host: event.target.value })}
            />
          </Field>
          <Field label="Private Key">
            <Grid columns="1fr auto" gap="2">
              <TextField.Root
                placeholder="Choose SSH private key"
                value={form.keyPath}
                onChange={(event) => onChange({ ...form, keyPath: event.target.value })}
              />
              <Button
                type="button"
                variant="surface"
                disabled={running}
                onClick={async () => {
                  const selected = await openFileDialog("Choose SSH private key");
                  if (selected) onChange({ ...form, keyPath: selected });
                }}
              >
                Choose
              </Button>
            </Grid>
          </Field>
        </Flex>
        <Flex gap="3" justify="end" mt="5">
          <Dialog.Close>
            <Button variant="soft" color="gray" disabled={running}>
              Cancel
            </Button>
          </Dialog.Close>
          <Button disabled={!canAttach} onClick={onAttach}>
            {running ? "Detecting..." : "Detect and Add"}
          </Button>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}

function RemoveRemoteServerDialog({
  server,
  onOpenChange,
  onRemove,
}: {
  server: RemoteServerRecord | null;
  onOpenChange: (open: boolean) => void;
  onRemove: (server: RemoteServerRecord) => void;
}) {
  return (
    <AlertDialog.Root open={!!server} onOpenChange={onOpenChange}>
      <AlertDialog.Content maxWidth="520px">
        <AlertDialog.Title>Forget Remote Server</AlertDialog.Title>
        <AlertDialog.Description size="2" color="gray">
          This only removes the saved server entry from this app. The remote Ubuntu host, Manager API, and Dune
          battlegroup will not be changed.
        </AlertDialog.Description>
        {server ? (
          <Box className="info-card" mt="4">
            <InfoRow label="Host" value={server.host} tone="amber" />
            <InfoRow label="Battlegroup" value={server.battlegroupName || "Setup pending"} tone="amber" />
          </Box>
        ) : null}
        <Flex gap="3" justify="end" mt="5">
          <AlertDialog.Cancel>
            <Button variant="soft" color="gray">
              Cancel
            </Button>
          </AlertDialog.Cancel>
          <AlertDialog.Action>
            <Button color="red" onClick={() => server && onRemove(server)}>
              Forget Server
            </Button>
          </AlertDialog.Action>
        </Flex>
      </AlertDialog.Content>
    </AlertDialog.Root>
  );
}

function UpdateDialog({
  open,
  update,
  status,
  progress,
  onOpenChange,
  onInstall,
}: {
  open: boolean;
  update: Update | null;
  status: UpdateStatus;
  progress: string | null;
  onOpenChange: (open: boolean) => void;
  onInstall: () => void;
}) {
  const busy = status === "installing" || status === "relaunching";

  return (
    <AlertDialog.Root open={open} onOpenChange={onOpenChange}>
      <AlertDialog.Content maxWidth="520px">
        <AlertDialog.Title>Install app update?</AlertDialog.Title>
        <AlertDialog.Description size="2">
          {update
            ? `Version ${update.version} is available. The app will download the signed installer, install it, and relaunch.`
            : "No update is currently selected."}
        </AlertDialog.Description>
        {update?.body ? (
          <TextArea mt="3" value={update.body} readOnly rows={7} />
        ) : null}
        {progress ? (
          <Text as="p" size="2" color="gray" mt="3" className="mono">
            {progress}
          </Text>
        ) : null}
        <Flex gap="3" mt="4" justify="end">
          <AlertDialog.Cancel disabled={busy}>Later</AlertDialog.Cancel>
          <AlertDialog.Action disabled={!update || busy} onClick={onInstall}>
            {busy ? "Installing..." : "Install update"}
          </AlertDialog.Action>
        </Flex>
      </AlertDialog.Content>
    </AlertDialog.Root>
  );
}
