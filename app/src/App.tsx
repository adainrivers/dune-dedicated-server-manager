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
import { open as openExternal } from "@tauri-apps/plugin-shell";
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
  ChevronDownIcon,
  ChevronUpIcon,
  CubeIcon,
  GlobeIcon,
  LightningBoltIcon,
  MixIcon,
  RocketIcon,
  DesktopIcon,
} from "@radix-ui/react-icons";

const pages = [
  { id: "servers", label: "Servers" },
  { id: "install", label: "Create New Server" },
  { id: "tools", label: "Tools" },
] as const;

type PageId = (typeof pages)[number]["id"];

type NetworkMode = "static" | "dhcp";
type PlayerIpMode = "local" | "external";
type SetupTarget = "hyperv" | "ubuntu" | "proxmox";
type RemoteServerKind = "ubuntu" | "alpine";

const startupUpdateChecksEnabled = import.meta.env.VITE_ENABLE_STARTUP_UPDATE_CHECK === "true";

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

type DetectionState = "idle" | "detecting" | "ready" | "failed";
type LogLevel = "debug" | "info" | "warn" | "error";
type LogLevelFilter = LogLevel;
type UpdateStatus = "idle" | "checking" | "available" | "current" | "installing" | "relaunching" | "failed";
type ServerPackageCheckStatus = "idle" | "checking" | "current" | "available" | "missing" | "updating" | "failed";

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

type ServerPackageStatus = {
  packageDir: string;
  appId: string;
  installedBuildId?: string | null;
  latestBuildId?: string | null;
  updateAvailable: boolean;
  complete: boolean;
  layout?: "legacyInternalScripts" | "battlegroupManagement" | null;
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
  preflight: UbuntuSshPreflight;
};

type ProxmoxNode = {
  node: string;
  status: string;
  cpu: number;
  maxcpu: number;
  mem: number;
  maxmem: number;
};

type ProxmoxStorage = {
  storage: string;
  type: string;
  content: string;
  active: number;
  shared: number;
  avail: number;
  total: number;
};

type ProxmoxBridge = {
  iface: string;
  type: string;
  active: number;
  cidr?: string | null;
  autostart: number;
};

type ProxmoxDetection = {
  version: { version: string; release: string; repoid: string };
  certificateSha256: string;
  certificateTrusted: boolean;
  nodes: ProxmoxNode[];
  storages: ProxmoxStorage[];
  bridges: ProxmoxBridge[];
  nextVmid: number;
};

type ProxmoxProvisioner = {
  type: "proxmox";
  profileId: string;
  hostUrl: string;
  tokenId: string;
  acceptedCertificateSha256?: string;
  node: string;
  vmid: number;
  vmName: string;
};

type ProxmoxAlpineSetupResult = {
  host: string;
  user: string;
  keyPath: string;
  namespace: string;
  battlegroupName: string;
  worldUniqueName: string;
  node: string;
  vmid: number;
  vmName: string;
};

type ProxmoxVmStatus = {
  status: string;
  name: string;
  pid?: number | null;
  maxmem: number;
  cpus: number;
};

type SetupRunResult = {
  vmName: string;
  namespace: string;
  battlegroupName: string;
  worldUniqueName: string;
  directorNodePort: number | null;
};

type GenerateSshKeyResult = {
  privateKeyPath: string;
  publicKeyPath: string;
  publicKey: string;
};

type RemoteServerRecord = {
  type: RemoteServerKind;
  id: string;
  name: string;
  host: string;
  user: string;
  keyPath: string;
  namespace: string;
  battlegroupName: string;
  worldUniqueName: string;
  phase: string;
  createdAt: string;
  provisioner?: ProxmoxProvisioner;
};

type RemoteServerProfile = {
  type: RemoteServerKind;
  host: string;
  keyPath?: string;
  createdAt: string;
  provisioner?: ProxmoxProvisioner;
};

type LocalServerProfile = {
  type: "hyperv";
  vmName: string;
  staticIp: string;
  createdAt: string;
};

type RemoteBattlegroupStatus = {
  stop: boolean;
  phase: string;
  serverGroupPhase: string;
  directorPhase: string;
};

type RemoteServerStatus = {
  battlegroup: RemoteBattlegroupStatus;
  package: RemoteServerPackageStatus;
};

type RemoteServerPackageStatus = {
  installedBuildId?: string | null;
  battlegroupVersion?: string | null;
  liveBattlegroupVersion?: string | null;
  operatorVersion?: string | null;
};

type PendingServerUpdate =
  | { type: "remote"; server: RemoteServerRecord }
  | { type: "local"; server: DuneVmCandidate };

type LocalHyperVRuntime = {
  namespace: string;
  battlegroupName: string;
  status: RemoteServerStatus;
  components: RemoteServerComponent[];
};

type RemoteServerComponent = {
  name: string;
  logKey: string;
  category: "system" | "map";
  state: string;
  tone: "green" | "amber" | "red" | "gray";
  summary: string;
  details: string[];
};

type RemoteComponentLogResult = {
  component: string;
  output: string;
};

type TunnelService = "director" | "fileBrowser";

type ServerTunnelStatus = {
  tunnelId: string;
  service: TunnelService;
  localPort: number;
  remotePort: number;
  url: string;
};

type ServerTunnelStartRequest = {
  tunnelId: string;
  serverKind: "hyperv" | "ubuntu" | "alpine";
  service: TunnelService;
  host: string;
  user?: string;
  keyPath?: string;
  vmName?: string;
  namespace: string;
};

type RemoteComponentRestartResult = {
  component: string;
  output: string;
};

type RemoteAttachForm = {
  type: RemoteServerKind;
  host: string;
  keyPath: string;
};

type LocalHyperVAttachForm = {
  vmName: string;
  staticIp: string;
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
  vmMemoryGb: string;
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
  proxmoxHostUrl: string;
  proxmoxTokenId: string;
  proxmoxTokenSecret: string;
  proxmoxAcceptedCertificateSha256: string;
  proxmoxNode: string;
  proxmoxVmStorage: string;
  proxmoxImportStorage: string;
  proxmoxBridge: string;
  proxmoxBridgeCidr: string;
  proxmoxVmid: string;
  proxmoxInstallQemuGuestAgent: boolean;
  saveLocalServer: boolean;
  saveRemoteServer: boolean;
};

const defaultHyperVVmName = "dune-awakening";
const defaultHyperVSwitchName = "DuneAwakeningServerSwitch";

const defaultForm: SetupForm = {
  setupTarget: "hyperv",
  vmDestination: "",
  vmName: defaultHyperVVmName,
  diskGb: "100",
  vmMemoryGb: "",
  processorCount: "4",
  enableSwap: false,
  networkMode: "static",
  switchName: defaultHyperVSwitchName,
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
  proxmoxHostUrl: "",
  proxmoxTokenId: "",
  proxmoxTokenSecret: "",
  proxmoxAcceptedCertificateSha256: "",
  proxmoxNode: "",
  proxmoxVmStorage: "",
  proxmoxImportStorage: "",
  proxmoxBridge: "",
  proxmoxBridgeCidr: "",
  proxmoxVmid: "",
  proxmoxInstallQemuGuestAgent: true,
  saveLocalServer: true,
  saveRemoteServer: true,
};

const remoteProfileStorageKey = "dune-manager.remote-ubuntu-profile";
const proxmoxProfileStorageKey = "dune-manager.proxmox-profile";
const remoteServersStorageKey = "dune-manager.remote-servers";
const localServersStorageKey = "dune-manager.local-hyperv-servers";
const maxStoredLogRows = 2500;
const maxRenderedLogRows = 1200;

const defaultRemoteAttachForm: RemoteAttachForm = {
  type: "ubuntu",
  host: "",
  keyPath: "",
};

const defaultLocalHyperVAttachForm: LocalHyperVAttachForm = {
  vmName: defaultHyperVVmName,
  staticIp: "",
};

const zeroToFour = ["0", "1", "2", "3", "4"];
const oneToFour = ["1", "2", "3", "4"];
const zeroToOne = ["0", "1"];
const playerPortForwards = [
  { ports: "7777-7810", protocol: "UDP", purpose: "Game servers" },
  { ports: "31982", protocol: "TCP", purpose: "RMQ" },
];

export function App() {
  const [activePage, setActivePage] = useState<PageId>("servers");
  const [form, setForm] = useState<SetupForm>(defaultForm);
  const [started, setStarted] = useState(false);
  const [setupRunning, setSetupRunning] = useState(false);
  const [setupRows, setSetupRows] = useState<LogRow[]>([]);
  const [initRows, setInitRows] = useState<LogRow[]>([]);
  const [logLevelFilter, setLogLevelFilter] = useState<LogLevelFilter>("info");
  const [logPanelCollapsed, setLogPanelCollapsed] = useState(false);
  const [rollbackOpen, setRollbackOpen] = useState(false);
  const [rollbackRunning, setRollbackRunning] = useState(false);
  const [failedRollbackRequest, setFailedRollbackRequest] = useState<RollbackRequest | null>(null);
  const [pendingServerUpdate, setPendingServerUpdate] = useState<PendingServerUpdate | null>(null);
  const [localAttachOpen, setLocalAttachOpen] = useState(false);
  const [localAttachRunning, setLocalAttachRunning] = useState(false);
  const [localAttachForm, setLocalAttachForm] = useState<LocalHyperVAttachForm>(defaultLocalHyperVAttachForm);
  const [remoteAttachOpen, setRemoteAttachOpen] = useState(false);
  const [remoteAttachRunning, setRemoteAttachRunning] = useState(false);
  const [remoteAttachForm, setRemoteAttachForm] = useState<RemoteAttachForm>(defaultRemoteAttachForm);
  const [remoteServerToRemove, setRemoteServerToRemove] = useState<RemoteServerRecord | null>(null);
  const [generatedSshKey, setGeneratedSshKey] = useState<GenerateSshKeyResult | null>(null);
  const [sshKeyGenerationRunning, setSshKeyGenerationRunning] = useState(false);
  const [availableUpdate, setAvailableUpdate] = useState<Update | null>(null);
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>("idle");
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<string | null>(null);
  const [serverPackageStatus, setServerPackageStatus] = useState<ServerPackageStatus | null>(null);
  const [serverPackageCheckStatus, setServerPackageCheckStatus] = useState<ServerPackageCheckStatus>("idle");
  const [hostReadiness, setHostReadiness] = useState<HostReadiness | null>(null);
  const [driveCandidates, setDriveCandidates] = useState<DriveCandidate[]>([]);
  const [networkAdapters, setNetworkAdapters] = useState<NetworkAdapterCandidate[]>([]);
  const [externalIp, setExternalIp] = useState<string | null>(null);
  const [networkDetection, setNetworkDetection] = useState<DetectionState>("idle");
  const [duneVms, setDuneVms] = useState<DuneVmCandidate[]>([]);
  const [localHyperVRuntimes, setLocalHyperVRuntimes] = useState<Record<string, LocalHyperVRuntime>>({});
  const [localHyperVRuntimeErrors, setLocalHyperVRuntimeErrors] = useState<Record<string, string>>({});
  const [vmDestinationHasVm, setVmDestinationHasVm] = useState(false);
  const [remoteServers, setRemoteServers] = useState<RemoteServerRecord[]>([]);
  const [remoteServerStatuses, setRemoteServerStatuses] = useState<Record<string, RemoteServerStatus>>({});
  const [remoteServerComponents, setRemoteServerComponents] = useState<Record<string, RemoteServerComponent[]>>({});
  const [remoteComponentLogs, setRemoteComponentLogs] = useState<Record<string, string>>({});
  const [remoteComponentLogBusy, setRemoteComponentLogBusy] = useState<Record<string, boolean>>({});
  const [remoteComponentRestartBusy, setRemoteComponentRestartBusy] = useState<Record<string, boolean>>({});
  const [remoteServerStatusErrors, setRemoteServerStatusErrors] = useState<Record<string, string>>({});
  const [remoteServerBusy, setRemoteServerBusy] = useState<Record<string, string>>({});
  const [serverTunnels, setServerTunnels] = useState<Record<string, ServerTunnelStatus>>({});
  const [serverTunnelBusy, setServerTunnelBusy] = useState<Record<string, boolean>>({});
  const [remotePreflight, setRemotePreflight] = useState<UbuntuSshPreflight | null>(null);
  const [remotePreflightStatus, setRemotePreflightStatus] = useState<DetectionState>("idle");
  const [proxmoxDetection, setProxmoxDetection] = useState<ProxmoxDetection | null>(null);
  const [proxmoxDetectionStatus, setProxmoxDetectionStatus] = useState<DetectionState>("idle");
  const [proxmoxVmStatuses, setProxmoxVmStatuses] = useState<Record<string, ProxmoxVmStatus>>({});
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
    setInitRows((rows) => limitLogRows([...rows, row]));
  };
  const appendSetupRow = (row: LogRow) => {
    setSetupRows((rows) => limitLogRows([...rows, row]));
  };
  const clearLogRows = () => {
    setInitRows([]);
    setSetupRows([]);
  };
  const checkForAppUpdate = async (source: "startup" | "manual") => {
    if (updateCheckInFlight.current) return;
    updateCheckInFlight.current = true;
    setUpdateStatus("checking");
    setUpdateProgress(null);
    appendInitRow(log.info("updates", "Checking for app updates."));
    try {
      const nextUpdate = await check({ timeout: 15_000 });
      setAvailableUpdate(nextUpdate);
      if (nextUpdate) {
        setUpdateStatus("available");
        appendInitRow(
          log.info(
            "updates",
            `Update ${nextUpdate.version} is available; current version is ${nextUpdate.currentVersion}.`,
          ),
        );
        setUpdateDialogOpen(true);
      } else {
        setUpdateStatus("current");
        appendInitRow(log.info("updates", "The app is up to date."));
      }
    } catch (err) {
      setUpdateStatus("failed");
      appendInitRow(log.warn("updates", `Update check failed: ${errorMessage(err)}`));
    } finally {
      updateCheckInFlight.current = false;
    }
  };
  const refreshServerPackageStatus = async () => {
    setServerPackageCheckStatus("checking");
    appendInitRow(log.info("server-package", "Checking Dune server package status."));
    try {
      const status = await invoke<ServerPackageStatus>("server_package_status");
      setServerPackageStatus(status);
      setServerPackageCheckStatus(
        !status.complete ? "missing" : status.updateAvailable ? "available" : "current",
      );
      appendInitRow(log.info("server-package", status.message));
    } catch (err) {
      setServerPackageCheckStatus("failed");
      appendInitRow(log.warn("server-package", `Package status check failed: ${errorMessage(err)}`));
    }
  };
  const updateServerPackage = async () => {
    setServerPackageCheckStatus("updating");
    try {
      const status = await invoke<ServerPackageStatus>("update_server_package");
      setServerPackageStatus(status);
      setServerPackageCheckStatus(
        !status.complete ? "missing" : status.updateAvailable ? "available" : "current",
      );
    } catch (err) {
      setServerPackageCheckStatus("failed");
      appendSetupRow(log.error("server-package", errorMessage(err)));
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
      await availableUpdate.downloadAndInstall(
        (event: DownloadEvent) => {
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
        },
        { timeout: 120_000 },
      );
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

  const runLocalDetection = async () => {
    setNetworkDetection("detecting");
    setSetupRows((rows) => [...rows, log.info("capabilities", "Detecting local host capabilities.")]);
    try {
      const [location, environment] = await Promise.all([
        invoke<string>("default_vm_location").catch(() => ""),
        invoke<EnvironmentDetection>("detect_environment"),
      ]);
      setHostReadiness(environment.readiness);
      setDriveCandidates(environment.drives);
      setNetworkAdapters(environment.networkAdapters);
      setExternalIp(environment.externalIp);
      setNetworkDetection("ready");
      if (location) {
        setForm((current) => (current.vmDestination ? current : { ...current, vmDestination: location }));
      }
      const first = environment.networkAdapters[0];
      if (first) {
        setForm((current) => ({
          ...current,
          adapterName: current.adapterName || first.name,
          switchName: current.switchName || first.existingExternalSwitch || defaultHyperVSwitchName,
          staticIp: current.staticIp || first.suggestedIpv4Address,
          playerIp: current.playerIp || (current.playerIpMode === "external" && environment.externalIp
            ? environment.externalIp
            : first.suggestedIpv4Address),
          gateway: current.gateway || first.gateway,
        }));
      }
      const gate = setupEnvironmentGate("ready", environment.readiness, environment.networkAdapters);
      setSetupRows((rows) => [
        ...rows,
        log.info("capabilities", "Local host capability detection completed."),
        ...environmentLogRows(
          "ready",
          environment.readiness,
          environment.networkAdapters,
          environment.drives,
          environment.externalIp,
          gate,
        ),
      ]);
    } catch (err) {
      setNetworkDetection("failed");
      setSetupRows((rows) => [...rows, log.error("capabilities", errorMessage(err))]);
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
      const publicIp = preflight.publicIp;
      if (publicIp && form.playerIpMode === "external" && form.playerIp !== publicIp) {
        update("playerIp", publicIp);
      } else if (publicIp && form.setupTarget === "ubuntu" && !form.playerIp.trim()) {
        setForm((current) => normalizeSetupForm({ ...current, playerIpMode: "external", playerIp: publicIp }));
      }
    } catch (err) {
      setRemotePreflightStatus("failed");
      setSetupRows((rows) => [...rows, log.error("ubuntu.preflight", errorMessage(err))]);
    }
  };

  const runProxmoxDetection = async () => {
    setProxmoxDetectionStatus("detecting");
    setProxmoxDetection(null);
    setSetupRows((rows) => [...rows, log.info("proxmox.detect", "Detecting Proxmox resources.")]);
    try {
      const detection = await invoke<ProxmoxDetection>("detect_proxmox", {
        request: proxmoxConnectionRequest(form),
      });
      setProxmoxDetection(detection);
      setProxmoxDetectionStatus("ready");
      const firstNode = detection.nodes[0]?.node || "";
      const vmStorage = detection.storages.find((storage) => storage.content.includes("images"))?.storage || detection.storages[0]?.storage || "";
      const importStorage = detection.storages.find((storage) => storage.content.includes("import"))?.storage || detection.storages[0]?.storage || "";
      const bridge = detection.bridges[0];
      setForm((current) => ({
        ...current,
        proxmoxNode: current.proxmoxNode || firstNode,
        proxmoxVmStorage: current.proxmoxVmStorage || vmStorage,
        proxmoxImportStorage: current.proxmoxImportStorage || importStorage,
        proxmoxBridge: current.proxmoxBridge || bridge?.iface || "",
        proxmoxBridgeCidr: current.proxmoxBridgeCidr || bridge?.cidr || "",
        proxmoxVmid: current.proxmoxVmid || String(detection.nextVmid || ""),
        proxmoxAcceptedCertificateSha256:
          current.proxmoxAcceptedCertificateSha256 || detection.certificateSha256,
      }));
      setSetupRows((rows) => [
        ...rows,
        log.info(
          "proxmox.detect",
          `Detected Proxmox ${detection.version.version}; ${detection.nodes.length} node(s), ${detection.storages.length} storage target(s), ${detection.bridges.length} bridge(s).`,
        ),
      ]);
      persistProxmoxProfile({
        hostUrl: form.proxmoxHostUrl.trim(),
        tokenId: form.proxmoxTokenId.trim(),
        acceptedCertificateSha256: detection.certificateSha256,
      });
    } catch (err) {
      setProxmoxDetectionStatus("failed");
      setSetupRows((rows) => [...rows, log.error("proxmox.detect", errorMessage(err))]);
    }
  };

  const generateUbuntuSshKey = async () => {
    setSshKeyGenerationRunning(true);
    setSetupRows((rows) => [...rows, log.info("ssh-key", "Generating an Ubuntu setup SSH key pair.")]);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Choose where to create the SSH key pair",
      });
      if (typeof selected !== "string") return;
      const result = await invoke<GenerateSshKeyResult>("generate_ubuntu_ssh_key", {
        request: {
          directory: selected,
          fileName: "dune_ubuntu_setup_ed25519",
        },
      });
      setGeneratedSshKey(result);
      update("remoteKeyPath", result.privateKeyPath);
      setSetupRows((rows) => [...rows, log.info("ssh-key", `Generated SSH key pair at ${result.privateKeyPath}.`)]);
    } catch (err) {
      setSetupRows((rows) => [...rows, log.error("ssh-key", errorMessage(err))]);
    } finally {
      setSshKeyGenerationRunning(false);
    }
  };

  const attachRemoteServer = async () => {
    setRemoteAttachRunning(true);
    setSetupRows((rows) => [...rows, log.info("remote.attach", "Adding remote server profile.")]);
    try {
      const record = remoteServerPlaceholder({
        type: remoteAttachForm.type,
        host: remoteAttachForm.host.trim(),
        keyPath: remoteAttachForm.keyPath.trim(),
        createdAt: new Date().toISOString(),
      });
      setRemoteServerStatuses((statuses) => omitKey(statuses, record.id));
      setRemoteServerComponents((components) => omitKey(components, record.id));
      setRemoteComponentLogs((logs) => omitPrefix(logs, `${record.id}:`));
      setRemoteComponentLogBusy((busy) => omitPrefix(busy, `${record.id}:`));
      setRemoteComponentRestartBusy((busy) => omitPrefix(busy, `${record.id}:`));
      setRemoteServerStatusErrors((errors) => omitKey(errors, record.id));
      setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, record)));
      setActivePage("servers");
      setRemoteAttachOpen(false);
      setRemoteAttachForm(defaultRemoteAttachForm);
      setSetupRows((rows) => [
        ...rows,
        log.info("remote.attach", "Added remote server profile."),
      ]);
      void refreshRemoteServerStatus(record);
    } catch (err) {
      setSetupRows((rows) => [...rows, log.error("remote.attach", errorMessage(err))]);
    } finally {
      setRemoteAttachRunning(false);
    }
  };

  const attachLocalHyperVServer = async () => {
    if (!localAttachForm.vmName.trim()) return;
    setLocalAttachRunning(true);
    setSetupRows((rows) => [...rows, log.info("local.attach", `Registering Hyper-V VM ${localAttachForm.vmName.trim()}.`)]);
    try {
      const candidate = await invoke<DuneVmCandidate>("register_local_hyperv_server", {
        request: { vmName: localAttachForm.vmName.trim() },
      });
      const record = mergeLocalServerAddress(localServerPlaceholder(candidate.vm.name, localAttachForm.staticIp), candidate);
      setDuneVms((servers) => persistLocalServers(upsertLocalServer(servers, record)));
      setLocalAttachOpen(false);
      setLocalAttachForm(defaultLocalHyperVAttachForm);
      setSetupRows((rows) => [...rows, log.info("local.attach", `Added local Hyper-V VM ${candidate.vm.name}.`)]);
    } catch (err) {
      setSetupRows((rows) => [...rows, log.error("local.attach", errorMessage(err))]);
    } finally {
      setLocalAttachRunning(false);
    }
  };

  const removeRemoteServer = (server: RemoteServerRecord) => {
    stopTunnelsForServer(server.id);
    setRemoteServers((servers) => {
      const next = persistRemoteServers(servers.filter((candidate) => candidate.id !== server.id));
      return next;
    });
    setRemoteServerStatuses((statuses) => omitKey(statuses, server.id));
    setRemoteServerComponents((components) => omitKey(components, server.id));
    setRemoteComponentLogs((logs) => omitPrefix(logs, `${server.id}:`));
    setRemoteComponentLogBusy((busy) => omitPrefix(busy, `${server.id}:`));
    setRemoteComponentRestartBusy((busy) => omitPrefix(busy, `${server.id}:`));
    setRemoteServerStatusErrors((errors) => omitKey(errors, server.id));
    setSetupRows((rows) => [...rows, log.info("remote.attach", "Forgot remote server profile.")]);
    setRemoteServerToRemove(null);
  };

  const removeLocalHyperVServer = (server: DuneVmCandidate) => {
    stopTunnelsForServer(localServerKey(server));
    setDuneVms((servers) => persistLocalServers(servers.filter((candidate) => candidate.vm.name !== server.vm.name)));
    setLocalHyperVRuntimes((runtimes) => omitKey(runtimes, localServerKey(server)));
    setLocalHyperVRuntimeErrors((errors) => omitKey(errors, localServerKey(server)));
    setRemoteComponentLogs((logs) => omitPrefix(logs, `${localServerKey(server)}:`));
    setRemoteComponentLogBusy((busy) => omitPrefix(busy, `${localServerKey(server)}:`));
    setRemoteComponentRestartBusy((busy) => omitPrefix(busy, `${localServerKey(server)}:`));
    setSetupRows((rows) => [...rows, log.info("local.attach", `Forgot local Hyper-V VM ${server.vm.name}.`)]);
  };

  const stopTunnelsForServer = (serverKey: string) => {
    for (const tunnelId of Object.keys(serverTunnels).filter((id) => id.startsWith(`${serverKey}:tunnel:`))) {
      void stopServerTunnel(tunnelId);
    }
  };

  const refreshLocalHyperVServer = async (server: DuneVmCandidate) => {
    const serverKey = localServerKey(server);
    setRemoteServerBusy((busy) => ({ ...busy, [serverKey]: "Retrieving server information" }));
    setLocalHyperVRuntimeErrors((errors) => omitKey(errors, serverKey));
    setRemoteComponentLogs((logs) => omitPrefix(logs, `${serverKey}:`));
    try {
      const candidate = await invoke<DuneVmCandidate>("register_local_hyperv_server", {
        request: { vmName: server.vm.name },
      });
      const mergedCandidate = mergeLocalServerAddress(server, candidate);
      setDuneVms((servers) => persistLocalServers(upsertLocalServer(servers, mergedCandidate)));
      if (mergedCandidate.vm.state === "running") {
        const runtime = await invoke<LocalHyperVRuntime>("local_hyperv_runtime", {
          request: { vmName: mergedCandidate.vm.name, host: primaryLocalServerIp(mergedCandidate) },
        });
        setLocalHyperVRuntimes((runtimes) => ({ ...runtimes, [localServerKey(mergedCandidate)]: runtime }));
      } else {
        setLocalHyperVRuntimes((runtimes) => omitKey(runtimes, localServerKey(mergedCandidate)));
      }
    } catch (err) {
      const message = errorMessage(err);
      setLocalHyperVRuntimeErrors((errors) => ({ ...errors, [serverKey]: message }));
      setSetupRows((rows) => [...rows, log.warn("local.status", message)]);
    } finally {
      setRemoteServerBusy((busy) => omitKey(busy, serverKey));
    }
  };

  const runLocalHyperVAction = async (server: DuneVmCandidate, action: "start" | "stop") => {
    const serverKey = localServerKey(server);
    setRemoteServerBusy((busy) => ({ ...busy, [serverKey]: action === "start" ? "Starting VM" : "Stopping VM" }));
    try {
      const candidate = await invoke<DuneVmCandidate>(
        action === "start" ? "start_local_hyperv_server" : "stop_local_hyperv_server",
        { request: { vmName: server.vm.name } },
      );
      setDuneVms((servers) => persistLocalServers(upsertLocalServer(servers, candidate)));
      setLocalHyperVRuntimes((runtimes) => omitKey(runtimes, serverKey));
      setLocalHyperVRuntimeErrors((errors) => omitKey(errors, serverKey));
      if (candidate.vm.state === "running") {
        void refreshLocalHyperVServer(candidate);
      }
    } catch (err) {
      setSetupRows((rows) => [...rows, log.error("local.vm", errorMessage(err))]);
    } finally {
      setRemoteServerBusy((busy) => omitKey(busy, serverKey));
    }
  };

  const detectRemoteServerDetails = async (server: RemoteServerRecord): Promise<RemoteServerRecord> => {
    const command = server.type === "alpine" ? "detect_remote_alpine_servers" : "detect_remote_ubuntu_servers";
    const detected = await invoke<RemoteServerRecord[]>(command, {
      request:
        server.type === "alpine"
          ? { host: server.host, serverType: "alpine", user: "dune" }
          : { host: server.host, keyPath: server.keyPath, serverType: "ubuntu", user: "root" },
    });
    if (detected.length === 0) {
      throw new Error("No Dune battlegroups were detected on the remote server.");
    }
    const selected =
      detected.find((candidate) => candidate.battlegroupName === server.battlegroupName) ?? detected[0];
    return remoteServerFromDetected(server, selected);
  };

  const refreshRemoteServerStatus = async (server: RemoteServerRecord) => {
    if (!server.host || (server.type === "ubuntu" && !server.keyPath)) return;
    setRemoteServerBusy((busy) => ({ ...busy, [server.id]: "Retrieving server information" }));
    setRemoteServerStatuses((statuses) => omitKey(statuses, server.id));
    setRemoteServerComponents((components) => omitKey(components, server.id));
    setRemoteComponentLogs((logs) => omitPrefix(logs, `${server.id}:`));
    setRemoteComponentLogBusy((busy) => omitPrefix(busy, `${server.id}:`));
    setRemoteComponentRestartBusy((busy) => omitPrefix(busy, `${server.id}:`));
    setRemoteServerStatusErrors((errors) => omitKey(errors, server.id));
    setSetupRows((rows) => [...rows, log.info("remote.status", "Retrieving remote server information.")]);
    try {
      const liveServer = await detectRemoteServerDetails(server);
      setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, liveServer)));
      const status = await invoke<RemoteServerStatus>("remote_server_status", {
        request: remoteServerActionRequest(liveServer),
      });
      const components = await invoke<RemoteServerComponent[]>("remote_server_components", {
        request: remoteServerActionRequest(liveServer),
      });
      setRemoteServerStatuses((statuses) => ({ ...statuses, [server.id]: status }));
      setRemoteServerComponents((current) => ({ ...current, [server.id]: components }));
      setRemoteServerStatusErrors((errors) => omitKey(errors, server.id));
      setRemoteServers((servers) =>
        persistRemoteServers(
          servers.map((candidate) =>
            candidate.id === server.id ? { ...liveServer, phase: status.battlegroup.phase || liveServer.phase } : candidate,
          ),
        ),
      );
      setSetupRows((rows) => [
        ...rows,
        log.info(
          "remote.status",
          `${liveServer.battlegroupName}: ${status.battlegroup.phase || "unknown"}, server group ${status.battlegroup.serverGroupPhase || "unknown"}, Director ${status.battlegroup.directorPhase || "unknown"}.`,
        ),
      ]);
    } catch (err) {
      const message = errorMessage(err);
      setRemoteServerStatuses((statuses) => omitKey(statuses, server.id));
      setRemoteServerComponents((components) => omitKey(components, server.id));
      setRemoteComponentLogs((logs) => omitPrefix(logs, `${server.id}:`));
      setRemoteServerStatusErrors((errors) => ({ ...errors, [server.id]: message }));
      setSetupRows((rows) => [...rows, log.warn("remote.status", message)]);
    } finally {
      setRemoteServerBusy((busy) => omitKey(busy, server.id));
    }
  };

  const runRemoteBattlegroupAction = async (server: RemoteServerRecord, action: "start" | "stop" | "update") => {
    const busyText =
      action === "start" ? "Starting battlegroup" : action === "stop" ? "Stopping battlegroup" : "Updating battlegroup";
    const verb = action === "start" ? "Starting" : action === "stop" ? "Stopping" : "Updating";
    setRemoteServerBusy((busy) => ({ ...busy, [server.id]: busyText }));
    setSetupRows((rows) => [...rows, log.info("bg", `${verb} remote battlegroup.`)]);
    try {
      const liveServer = server.namespace && server.battlegroupName ? server : await detectRemoteServerDetails(server);
      setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, liveServer)));
      const command =
        action === "start"
          ? "start_remote_battlegroup"
          : action === "stop"
            ? "stop_remote_battlegroup"
            : "update_remote_battlegroup";
      const status = await invoke<RemoteServerStatus>(command, { request: remoteServerActionRequest(liveServer) });
      const components = await invoke<RemoteServerComponent[]>("remote_server_components", {
        request: remoteServerActionRequest(liveServer),
      });
      setRemoteServerStatuses((statuses) => ({ ...statuses, [server.id]: status }));
      setRemoteServerComponents((current) => ({ ...current, [server.id]: components }));
      setRemoteServerStatusErrors((errors) => omitKey(errors, server.id));
      setRemoteServers((servers) =>
        persistRemoteServers(
          servers.map((candidate) =>
            candidate.id === server.id ? { ...liveServer, phase: status.battlegroup.phase || liveServer.phase } : candidate,
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

  const runProxmoxVmAction = async (server: RemoteServerRecord, action: "start" | "stop" | "status") => {
    if (!server.provisioner) return;
    const key = server.id;
    setRemoteServerBusy((busy) => ({ ...busy, [key]: action === "status" ? "Checking Proxmox VM" : `${action === "start" ? "Starting" : "Stopping"} Proxmox VM` }));
    try {
      const command =
        action === "start" ? "start_proxmox_vm" : action === "stop" ? "stop_proxmox_vm" : "proxmox_vm_status";
      const status = await invoke<ProxmoxVmStatus>(command, {
        request: proxmoxVmActionRequest(server.provisioner),
      });
      setProxmoxVmStatuses((statuses) => ({ ...statuses, [key]: status }));
      setSetupRows((rows) => [...rows, log.info("proxmox.vm", `VM ${server.provisioner?.vmid} is ${status.status || "unknown"}.`)]);
    } catch (err) {
      setSetupRows((rows) => [...rows, log.error("proxmox.vm", errorMessage(err))]);
    } finally {
      setRemoteServerBusy((busy) => omitKey(busy, key));
    }
  };

  const runLocalHyperVBattlegroupAction = async (server: DuneVmCandidate, action: "start" | "stop" | "update") => {
    const serverKey = localServerKey(server);
    const runtime = localHyperVRuntimes[serverKey];
    if (!runtime) return;
    const busyText =
      action === "start" ? "Starting battlegroup" : action === "stop" ? "Stopping battlegroup" : "Updating battlegroup";
    const verb = action === "start" ? "Starting" : action === "stop" ? "Stopping" : "Updating";
    setRemoteServerBusy((busy) => ({
      ...busy,
      [serverKey]: busyText,
    }));
    setSetupRows((rows) => [...rows, log.info("bg", `${verb} local battlegroup.`)]);
    try {
      const command =
        action === "start"
          ? "start_local_hyperv_battlegroup"
          : action === "stop"
            ? "stop_local_hyperv_battlegroup"
            : "update_local_hyperv_battlegroup";
      const status = await invoke<RemoteServerStatus>(command, {
        request: {
          vmName: server.vm.name,
          host: primaryLocalServerIp(server),
          namespace: runtime.namespace,
          battlegroupName: runtime.battlegroupName,
        },
      });
      setLocalHyperVRuntimes((runtimes) => ({
        ...runtimes,
        [serverKey]: { ...runtime, status },
      }));
      void refreshLocalHyperVServer(server);
    } catch (err) {
      const message = errorMessage(err);
      setLocalHyperVRuntimeErrors((errors) => ({ ...errors, [serverKey]: message }));
      setSetupRows((rows) => [...rows, log.error("bg", message)]);
    } finally {
      setRemoteServerBusy((busy) => omitKey(busy, serverKey));
    }
  };

  const startServerTunnel = async (request: ServerTunnelStartRequest) => {
    setServerTunnelBusy((busy) => ({ ...busy, [request.tunnelId]: true }));
    setSetupRows((rows) => [...rows, log.info("tunnel", `Starting ${tunnelServiceLabel(request.service)} tunnel.`)]);
    try {
      const status = await invoke<ServerTunnelStatus>("start_server_tunnel", { request });
      setServerTunnels((tunnels) => ({ ...tunnels, [status.tunnelId]: status }));
      setSetupRows((rows) => [
        ...rows,
        log.info("tunnel", `${tunnelServiceLabel(request.service)} tunnel is ready at ${status.url}`),
      ]);
    } catch (err) {
      setSetupRows((rows) => [...rows, log.error("tunnel", errorMessage(err))]);
    } finally {
      setServerTunnelBusy((busy) => omitKey(busy, request.tunnelId));
    }
  };

  const openServerTunnel = async (tunnel: ServerTunnelStatus) => {
    try {
      const status = await invoke<ServerTunnelStatus | null>("server_tunnel_status", {
        request: { tunnelId: tunnel.tunnelId },
      });
      if (!status) {
        setServerTunnels((tunnels) => omitKey(tunnels, tunnel.tunnelId));
        setSetupRows((rows) => [...rows, log.warn("tunnel", "The SSH tunnel is no longer running.")]);
        return;
      }
      setServerTunnels((tunnels) => ({ ...tunnels, [status.tunnelId]: status }));
      await openExternal(status.url);
    } catch (err) {
      setSetupRows((rows) => [...rows, log.error("tunnel", errorMessage(err))]);
    }
  };

  const stopServerTunnel = async (tunnelId: string) => {
    setServerTunnelBusy((busy) => ({ ...busy, [tunnelId]: true }));
    try {
      await invoke("stop_server_tunnel", { request: { tunnelId } });
      setServerTunnels((tunnels) => omitKey(tunnels, tunnelId));
      setSetupRows((rows) => [...rows, log.info("tunnel", "SSH tunnel stopped.")]);
    } catch (err) {
      setSetupRows((rows) => [...rows, log.error("tunnel", errorMessage(err))]);
    } finally {
      setServerTunnelBusy((busy) => omitKey(busy, tunnelId));
    }
  };

  const refreshRemoteComponentLog = async (server: RemoteServerRecord, component: RemoteServerComponent) => {
    const key = componentLogStateKey(server.id, component);
    setRemoteComponentLogBusy((busy) => ({ ...busy, [key]: true }));
    setSetupRows((rows) => [...rows, log.info("remote.logs", `Refreshing ${component.name} logs.`)]);
    try {
      const liveServer = server.namespace ? server : await detectRemoteServerDetails(server);
      if (!server.namespace) {
        setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, liveServer)));
      }
      const result = await invoke<RemoteComponentLogResult>("remote_component_log_tail", {
        request: {
          serverType: liveServer.type,
          host: liveServer.host,
          user: liveServer.user || remoteServerDefaultUser(liveServer.type),
          keyPath: liveServer.keyPath || undefined,
          namespace: liveServer.namespace,
          component: component.logKey,
          tail: 160,
        },
      });
      setRemoteComponentLogs((logs) => ({
        ...logs,
        [key]: sanitizeLogMessage(result.output || "No log output."),
      }));
    } catch (err) {
      const message = errorMessage(err);
      setRemoteComponentLogs((logs) => ({ ...logs, [key]: sanitizeLogMessage(message) }));
      setSetupRows((rows) => [...rows, log.warn("remote.logs", message)]);
    } finally {
      setRemoteComponentLogBusy((busy) => omitKey(busy, key));
    }
  };

  const refreshLocalHyperVComponentLog = async (server: DuneVmCandidate, component: RemoteServerComponent) => {
    const serverKey = localServerKey(server);
    const runtime = localHyperVRuntimes[serverKey];
    if (!runtime) return;
    const key = componentLogStateKey(serverKey, component);
    setRemoteComponentLogBusy((busy) => ({ ...busy, [key]: true }));
    setSetupRows((rows) => [...rows, log.info("local.logs", `Refreshing ${component.name} logs.`)]);
    try {
      const result = await invoke<RemoteComponentLogResult>("local_hyperv_component_log_tail", {
        request: {
          vmName: server.vm.name,
          host: primaryLocalServerIp(server),
          namespace: runtime.namespace,
          component: component.logKey,
          tail: 160,
        },
      });
      setRemoteComponentLogs((logs) => ({
        ...logs,
        [key]: sanitizeLogMessage(result.output || "No log output."),
      }));
    } catch (err) {
      const message = errorMessage(err);
      setRemoteComponentLogs((logs) => ({ ...logs, [key]: sanitizeLogMessage(message) }));
      setSetupRows((rows) => [...rows, log.warn("local.logs", message)]);
    } finally {
      setRemoteComponentLogBusy((busy) => omitKey(busy, key));
    }
  };

  const restartRemoteComponent = async (server: RemoteServerRecord, component: RemoteServerComponent) => {
    if (isCriticalRestartComponent(component)) {
      const confirmed = window.confirm(
        `Restart ${component.name}? This can temporarily interrupt persistence, messaging, or active players.`,
      );
      if (!confirmed) return;
    }
    const key = componentLogStateKey(server.id, component);
    setRemoteComponentRestartBusy((busy) => ({ ...busy, [key]: true }));
    setSetupRows((rows) => [...rows, log.warn("remote.restart", `Restarting ${component.name}.`)]);
    try {
      const liveServer = server.namespace ? server : await detectRemoteServerDetails(server);
      if (!server.namespace) {
        setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, liveServer)));
      }
      const result = await invoke<RemoteComponentRestartResult>("restart_remote_component", {
        request: {
          serverType: liveServer.type,
          host: liveServer.host,
          user: liveServer.user || remoteServerDefaultUser(liveServer.type),
          keyPath: liveServer.keyPath || undefined,
          namespace: liveServer.namespace,
          component: component.logKey,
        },
      });
      setRemoteComponentLogs((logs) => ({
        ...logs,
        [key]: sanitizeLogMessage(result.output || `${component.name} restart requested.`),
      }));
      const components = await invoke<RemoteServerComponent[]>("remote_server_components", {
        request: remoteServerActionRequest(liveServer),
      });
      setRemoteServerComponents((current) => ({ ...current, [server.id]: components }));
    } catch (err) {
      const message = errorMessage(err);
      setRemoteComponentLogs((logs) => ({ ...logs, [key]: sanitizeLogMessage(message) }));
      setSetupRows((rows) => [...rows, log.error("remote.restart", message)]);
    } finally {
      setRemoteComponentRestartBusy((busy) => omitKey(busy, key));
    }
  };

  const restartLocalHyperVComponent = async (server: DuneVmCandidate, component: RemoteServerComponent) => {
    if (isCriticalRestartComponent(component)) {
      const confirmed = window.confirm(
        `Restart ${component.name}? This can temporarily interrupt persistence, messaging, or active players.`,
      );
      if (!confirmed) return;
    }
    const serverKey = localServerKey(server);
    const runtime = localHyperVRuntimes[serverKey];
    if (!runtime) return;
    const key = componentLogStateKey(serverKey, component);
    setRemoteComponentRestartBusy((busy) => ({ ...busy, [key]: true }));
    setSetupRows((rows) => [...rows, log.warn("local.restart", `Restarting ${component.name}.`)]);
    try {
      const result = await invoke<RemoteComponentRestartResult>("restart_local_hyperv_component", {
        request: {
          vmName: server.vm.name,
          host: primaryLocalServerIp(server),
          namespace: runtime.namespace,
          component: component.logKey,
          tail: 160,
        },
      });
      setRemoteComponentLogs((logs) => ({
        ...logs,
        [key]: sanitizeLogMessage(result.output || `${component.name} restart requested.`),
      }));
      void refreshLocalHyperVServer(server);
    } catch (err) {
      const message = errorMessage(err);
      setRemoteComponentLogs((logs) => ({ ...logs, [key]: sanitizeLogMessage(message) }));
      setSetupRows((rows) => [...rows, log.error("local.restart", message)]);
    } finally {
      setRemoteComponentRestartBusy((busy) => omitKey(busy, key));
    }
  };

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
    const text = window.localStorage.getItem(proxmoxProfileStorageKey);
    if (!text) return;
    try {
      const profile = JSON.parse(text) as Partial<
        Pick<SetupForm, "proxmoxHostUrl" | "proxmoxTokenId" | "proxmoxAcceptedCertificateSha256">
      >;
      setForm((current) =>
        normalizeSetupForm({
          ...current,
          proxmoxHostUrl: profile.proxmoxHostUrl || current.proxmoxHostUrl,
          proxmoxTokenId: profile.proxmoxTokenId || current.proxmoxTokenId,
          proxmoxAcceptedCertificateSha256:
            profile.proxmoxAcceptedCertificateSha256 || current.proxmoxAcceptedCertificateSha256,
        }),
      );
    } catch {
      window.localStorage.removeItem(proxmoxProfileStorageKey);
    }
  }, []);

  useEffect(() => {
    setDuneVms(readLocalServers());
    setRemoteServers(readRemoteServers());
    void refreshServerPackageStatus();
  }, []);

  useEffect(() => {
    let cancelled = false;
    for (const server of remoteServers) {
      if (!server.host || (server.type === "ubuntu" && !server.keyPath) || remoteServerBusy[server.id]) continue;
      void refreshRemoteServerStatus(server);
    }
    return () => {
      cancelled = true;
    };
  }, [remoteServers.map((server) => server.id).join("|")]);

  useEffect(() => {
    for (const server of duneVms) {
      if (remoteServerBusy[localServerKey(server)]) continue;
      void refreshLocalHyperVServer(server);
    }
  }, [duneVms.map((server) => server.vm.name).join("|")]);

  useEffect(() => {
    const profile = {
      remoteHost: form.remoteHost,
      remoteUser: form.remoteUser,
      remoteKeyPath: form.remoteKeyPath,
    };
    window.localStorage.setItem(remoteProfileStorageKey, JSON.stringify(profile));
    setRemotePreflight(null);
    setRemotePreflightStatus("idle");
  }, [form.remoteHost, form.remoteKeyPath, form.remoteUser]);

  useEffect(() => {
    persistProxmoxProfile({
      hostUrl: form.proxmoxHostUrl,
      tokenId: form.proxmoxTokenId,
      acceptedCertificateSha256: form.proxmoxAcceptedCertificateSha256,
    });
  }, [form.proxmoxHostUrl, form.proxmoxTokenId, form.proxmoxAcceptedCertificateSha256]);

  useEffect(() => {
    setProxmoxDetection(null);
    setProxmoxDetectionStatus("idle");
  }, [form.proxmoxHostUrl, form.proxmoxTokenId]);

  useEffect(() => {
    if (!startupUpdateChecksEnabled) {
      appendInitRow(log.debug("updates", "Automatic update checks are disabled for this local build."));
      return;
    }

    const timer = window.setTimeout(() => {
      void checkForAppUpdate("startup");
    }, 1_500);

    return () => window.clearTimeout(timer);
  }, []);

  useEffect(() => {
    return () => {
      void invoke("stop_all_tunnels");
    };
  }, []);

  useEffect(() => {
    const timer = window.setInterval(() => {
      for (const tunnel of Object.values(serverTunnels)) {
        invoke<ServerTunnelStatus | null>("server_tunnel_status", {
          request: { tunnelId: tunnel.tunnelId },
        })
          .then((status) => {
            if (!status) {
              setServerTunnels((tunnels) => omitKey(tunnels, tunnel.tunnelId));
            }
          })
          .catch(() => {
            setServerTunnels((tunnels) => omitKey(tunnels, tunnel.tunnelId));
          });
      }
    }, 5000);
    return () => window.clearInterval(timer);
  }, [serverTunnels]);


  useEffect(() => {
    const onError = (event: ErrorEvent) => {
      appendSetupRow(log.error("ui", event.message || "Unhandled browser error."));
    };
    const onRejection = (event: PromiseRejectionEvent) => {
      appendSetupRow(log.error("ui", errorMessage(event.reason)));
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
      appendSetupRow(logEntry(event.payload.level, event.payload.scope, event.payload.message));
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

  const logRows = useMemo(() => limitLogRows([...initRows, ...setupRows]), [initRows, setupRows]);
  const visibleLogRows = useMemo(
    () => filterLogRows(logRows, logLevelFilter).slice(-maxRenderedLogRows),
    [logLevelFilter, logRows],
  );
  const startSetup = async () => {
    const setupMemoryGb =
      form.setupTarget === "proxmox"
        ? effectiveProxmoxVmMemoryGb(form, calculatedMemory, proxmoxDetection)
        : effectiveVmMemoryGb(form, calculatedMemory);
    const request = setupRunRequest(form, setupMemoryGb);
    setStarted(true);
    setSetupRunning(true);
    setFailedRollbackRequest(null);
    try {
      if (form.setupTarget === "ubuntu") {
        const pendingRecord = form.saveRemoteServer ? remoteServerDraftFromForm(form) : null;
        if (pendingRecord) {
          setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, pendingRecord)));
        }
        const result = await invoke<RemoteSetupRunResult>("start_remote_ubuntu_setup", {
          request: remoteSetupRunRequest(form),
        });
        setSetupRows((rows) => [
          ...rows,
          log.info("ubuntu", "Server provisioning completed. It can take some time before the server appears in-game."),
        ]);
        if (form.saveRemoteServer) {
          const record = remoteServerRecordFromSetup(form, result, pendingRecord?.id);
          setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, record)));
        }
      } else if (form.setupTarget === "proxmox") {
        const pendingRecord = form.saveRemoteServer ? proxmoxServerDraftFromForm(form) : null;
        if (pendingRecord) {
          setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, pendingRecord)));
        }
        const result = await invoke<ProxmoxAlpineSetupResult>("start_proxmox_alpine_setup", {
          request: proxmoxSetupRunRequest(form, setupMemoryGb),
        });
        setSetupRows((rows) => [
          ...rows,
          log.info("proxmox", "Proxmox Alpine provisioning completed. The server is starting through the guest bootstrap flow."),
        ]);
        if (form.saveRemoteServer) {
          const record = proxmoxServerRecordFromSetup(form, result, pendingRecord?.id);
          setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, record)));
        }
      } else {
        if (form.saveLocalServer) {
          const pending = localServerPlaceholder(request.vmName, request.staticIp);
          setDuneVms((servers) => persistLocalServers(upsertLocalServer(servers, pending)));
          setLocalHyperVRuntimeErrors((errors) => omitKey(errors, localServerKey(pending)));
        }
        const result = await invoke<SetupRunResult>("start_full_setup", {
          request,
        });
        if (form.saveLocalServer) {
          try {
            const candidate = await invoke<DuneVmCandidate>("register_local_hyperv_server", {
              request: { vmName: result.vmName || request.vmName },
            });
            setDuneVms((servers) => persistLocalServers(upsertLocalServer(servers, candidate)));
          } catch (err) {
            setSetupRows((rows) => [...rows, log.warn("local.attach", `Setup completed but server registration failed: ${errorMessage(err)}`)]);
          }
        }
      }
    } catch (err) {
      console.error(err);
      appendSetupRow(log.error("setup", errorMessage(err)));
      if ((form.setupTarget === "ubuntu" || form.setupTarget === "proxmox") && form.saveRemoteServer) {
        const pending = form.setupTarget === "proxmox" ? proxmoxServerDraftFromForm(form) : remoteServerDraftFromForm(form);
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

  const confirmPendingServerUpdate = () => {
    const pending = pendingServerUpdate;
    if (!pending) return;
    setPendingServerUpdate(null);
    if (pending.type === "remote") {
      void runRemoteBattlegroupAction(pending.server, "update");
    } else {
      void runLocalHyperVBattlegroupAction(pending.server, "update");
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
      <Flex direction="column" className="app-root">
        <Header
          activePage={activePage}
          onNavigate={setActivePage}
          serverCount={duneVms.length + remoteServers.length}
          updateStatus={updateStatus}
          update={availableUpdate}
          updateProgress={updateProgress}
          serverPackageStatus={serverPackageStatus}
          serverPackageCheckStatus={serverPackageCheckStatus}
          onCheckUpdate={() => void checkForAppUpdate("manual")}
          onOpenUpdate={() => setUpdateDialogOpen(true)}
          onCheckServerPackage={() => void refreshServerPackageStatus()}
          onUpdateServerPackage={() => void updateServerPackage()}
        />
        <Separator size="4" />
        <Box className={logPanelCollapsed ? "app-main log-collapsed" : "app-main has-log"}>
          <AppErrorBoundary
            onError={(message) => setSetupRows((rows) => [...rows, log.error("ui", message)])}
          >
            {activePage === "servers" ? (
              <ServersPage
                duneVms={duneVms}
                remoteServers={remoteServers}
                remoteStatuses={remoteServerStatuses}
                remoteComponents={remoteServerComponents}
                localRuntimes={localHyperVRuntimes}
                localRuntimeErrors={localHyperVRuntimeErrors}
                remoteComponentLogs={remoteComponentLogs}
                remoteComponentLogBusy={remoteComponentLogBusy}
                remoteComponentRestartBusy={remoteComponentRestartBusy}
                remoteStatusErrors={remoteServerStatusErrors}
                remoteBusy={remoteServerBusy}
                serverPackageStatus={serverPackageStatus}
                proxmoxVmStatuses={proxmoxVmStatuses}
                tunnels={serverTunnels}
                tunnelBusy={serverTunnelBusy}
                onAddLocalServer={() => setLocalAttachOpen(true)}
                onAddRemoteServer={() => {
                  setRemoteAttachForm({
                    type: "ubuntu",
                    host: form.remoteHost,
                    keyPath: form.remoteKeyPath,
                  });
                  setRemoteAttachOpen(true);
                }}
                onRemoveLocalServer={removeLocalHyperVServer}
                onRefreshLocalServer={(server) => void refreshLocalHyperVServer(server)}
                onStartLocalServer={(server) => void runLocalHyperVAction(server, "start")}
                onStopLocalServer={(server) => void runLocalHyperVAction(server, "stop")}
                onRemoveRemoteServer={setRemoteServerToRemove}
                onRefreshRemoteStatus={(server) => void refreshRemoteServerStatus(server)}
                onStartRemoteBattlegroup={(server) => void runRemoteBattlegroupAction(server, "start")}
                onStopRemoteBattlegroup={(server) => void runRemoteBattlegroupAction(server, "stop")}
                onUpdateRemoteBattlegroup={(server) => setPendingServerUpdate({ type: "remote", server })}
                onRefreshProxmoxVm={(server) => void runProxmoxVmAction(server, "status")}
                onStartProxmoxVm={(server) => void runProxmoxVmAction(server, "start")}
                onStopProxmoxVm={(server) => void runProxmoxVmAction(server, "stop")}
                onStartLocalBattlegroup={(server) => void runLocalHyperVBattlegroupAction(server, "start")}
                onStopLocalBattlegroup={(server) => void runLocalHyperVBattlegroupAction(server, "stop")}
                onUpdateLocalBattlegroup={(server) => setPendingServerUpdate({ type: "local", server })}
                onStartTunnel={(request) => void startServerTunnel(request)}
                onStopTunnel={(tunnelId) => void stopServerTunnel(tunnelId)}
                onOpenTunnel={(tunnel) => void openServerTunnel(tunnel)}
                onRefreshRemoteComponentLog={(server, component) =>
                  void refreshRemoteComponentLog(server, component)
                }
                onRestartRemoteComponent={(server, component) =>
                  void restartRemoteComponent(server, component)
                }
                onRefreshLocalComponentLog={(server, component) =>
                  void refreshLocalHyperVComponentLog(server, component)
                }
                onRestartLocalComponent={(server, component) =>
                  void restartLocalHyperVComponent(server, component)
                }
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
                proxmoxDetection={proxmoxDetection}
                proxmoxDetectionStatus={proxmoxDetectionStatus}
                serverPackageStatus={serverPackageStatus}
                serverPackageCheckStatus={serverPackageCheckStatus}
                update={update}
                onUpdateServerPackage={() => void updateServerPackage()}
                onLocalDetection={() => void runLocalDetection()}
                onRemotePreflight={() => void runRemotePreflight()}
                onProxmoxDetection={() => void runProxmoxDetection()}
                onStart={startSetup}
              />
            ) : null}
            {activePage === "tools" ? (
              <ToolsPage
                generatedSshKey={generatedSshKey}
                sshKeyGenerationRunning={sshKeyGenerationRunning}
                onGenerateUbuntuSshKey={() => void generateUbuntuSshKey()}
              />
            ) : null}
            <LogWindow
              rows={visibleLogRows}
              level={logLevelFilter}
              collapsed={logPanelCollapsed}
              onLevelChange={setLogLevelFilter}
              onClear={clearLogRows}
              onToggleCollapsed={() => setLogPanelCollapsed((collapsed) => !collapsed)}
            />
          </AppErrorBoundary>
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
        <ServerUpdateConfirmDialog
          pending={pendingServerUpdate}
          onOpenChange={(open) => {
            if (!open) setPendingServerUpdate(null);
          }}
          onConfirm={confirmPendingServerUpdate}
        />
        <RemoteAttachDialog
          open={remoteAttachOpen}
          form={remoteAttachForm}
          running={remoteAttachRunning}
          onOpenChange={setRemoteAttachOpen}
          onChange={setRemoteAttachForm}
          onAttach={() => void attachRemoteServer()}
        />
        <LocalHyperVAttachDialog
          open={localAttachOpen}
          form={localAttachForm}
          running={localAttachRunning}
          onOpenChange={setLocalAttachOpen}
          onChange={setLocalAttachForm}
          onAttach={() => void attachLocalHyperVServer()}
        />
        <RemoveRemoteServerDialog
          server={remoteServerToRemove}
          onOpenChange={(open) => {
            if (!open) setRemoteServerToRemove(null);
          }}
          onRemove={removeRemoteServer}
        />
      </Flex>
    </Theme>
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
  serverCount,
  updateStatus,
  update,
  updateProgress,
  serverPackageStatus,
  serverPackageCheckStatus,
  onCheckUpdate,
  onOpenUpdate,
  onCheckServerPackage,
  onUpdateServerPackage,
}: {
  activePage: PageId;
  onNavigate: (page: PageId) => void;
  serverCount: number;
  updateStatus: UpdateStatus;
  update: Update | null;
  updateProgress: string | null;
  serverPackageStatus: ServerPackageStatus | null;
  serverPackageCheckStatus: ServerPackageCheckStatus;
  onCheckUpdate: () => void;
  onOpenUpdate: () => void;
  onCheckServerPackage: () => void;
  onUpdateServerPackage: () => void;
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
            serverCount={serverCount}
          />
        </Flex>
        <Flex align="center" gap="2" wrap="wrap" justify="end">
          <ServerPackageHeaderControl
            status={serverPackageCheckStatus}
            packageStatus={serverPackageStatus}
            onCheck={onCheckServerPackage}
            onUpdate={onUpdateServerPackage}
          />
          <UpdateHeaderControl
            status={updateStatus}
            update={update}
            progress={updateProgress}
            onCheck={onCheckUpdate}
            onOpenUpdate={onOpenUpdate}
          />
        </Flex>
      </header>
    </Flex>
  );
}

function ServerPackageHeaderControl({
  status,
  packageStatus,
  onCheck,
  onUpdate,
}: {
  status: ServerPackageCheckStatus;
  packageStatus: ServerPackageStatus | null;
  onCheck: () => void;
  onUpdate: () => void;
}) {
  const busy = status === "checking" || status === "updating";
  const canUpdate = status === "available" || status === "missing" || (packageStatus ? !packageStatus.complete : false);
  return (
    <Flex align="center" gap="2" className="header-update">
      <Badge color={serverPackageTone(status)} variant="soft">
        {serverPackageLabel(status, packageStatus)}
      </Badge>
      <Button size="1" variant="surface" disabled={busy} onClick={canUpdate ? onUpdate : onCheck}>
        {busy ? "Working..." : canUpdate ? "Update package" : "Check package"}
      </Button>
    </Flex>
  );
}

function UpdateHeaderControl({
  status,
  update,
  progress,
  onCheck,
  onOpenUpdate,
}: {
  status: UpdateStatus;
  update: Update | null;
  progress: string | null;
  onCheck: () => void;
  onOpenUpdate: () => void;
}) {
  const busy = status === "checking" || status === "installing" || status === "relaunching";
  const hasUpdate = Boolean(update);
  const actionLabel = hasUpdate ? "Install" : "Check for updates";

  return (
    <Flex align="center" gap="2" className="header-update">
      <Badge color={updateTone(status)} variant="soft">
        {updateLabel(status, update, progress)}
      </Badge>
      <Button
        size="1"
        variant={hasUpdate ? "solid" : "surface"}
        disabled={busy}
        onClick={hasUpdate ? onOpenUpdate : onCheck}
      >
        {busy ? "Working..." : actionLabel}
      </Button>
    </Flex>
  );
}

function TopNav({
  activePage,
  onNavigate,
  serverCount,
}: {
  activePage: PageId;
  onNavigate: (page: PageId) => void;
  serverCount: number;
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
              {page.id === "servers" ? `${page.label} (${serverCount})` : page.label}
            </TabNav.Link>
          ))}
        </TabNav.Root>
      </nav>
    </Box>
  );
}

function ServersPage({
  duneVms,
  remoteServers,
  remoteStatuses,
  remoteComponents,
  localRuntimes,
  localRuntimeErrors,
  remoteComponentLogs,
  remoteComponentLogBusy,
  remoteComponentRestartBusy,
  remoteStatusErrors,
  remoteBusy,
  serverPackageStatus,
  proxmoxVmStatuses,
  tunnels,
  tunnelBusy,
  onAddLocalServer,
  onAddRemoteServer,
  onRemoveLocalServer,
  onRefreshLocalServer,
  onStartLocalServer,
  onStopLocalServer,
  onRemoveRemoteServer,
  onRefreshRemoteStatus,
  onStartRemoteBattlegroup,
  onStopRemoteBattlegroup,
  onUpdateRemoteBattlegroup,
  onRefreshProxmoxVm,
  onStartProxmoxVm,
  onStopProxmoxVm,
  onStartLocalBattlegroup,
  onStopLocalBattlegroup,
  onUpdateLocalBattlegroup,
  onStartTunnel,
  onStopTunnel,
  onOpenTunnel,
  onRefreshRemoteComponentLog,
  onRestartRemoteComponent,
  onRefreshLocalComponentLog,
  onRestartLocalComponent,
}: {
  duneVms: DuneVmCandidate[];
  remoteServers: RemoteServerRecord[];
  remoteStatuses: Record<string, RemoteServerStatus>;
  remoteComponents: Record<string, RemoteServerComponent[]>;
  localRuntimes: Record<string, LocalHyperVRuntime>;
  localRuntimeErrors: Record<string, string>;
  remoteComponentLogs: Record<string, string>;
  remoteComponentLogBusy: Record<string, boolean>;
  remoteComponentRestartBusy: Record<string, boolean>;
  remoteStatusErrors: Record<string, string>;
  remoteBusy: Record<string, string>;
  serverPackageStatus: ServerPackageStatus | null;
  proxmoxVmStatuses: Record<string, ProxmoxVmStatus>;
  tunnels: Record<string, ServerTunnelStatus>;
  tunnelBusy: Record<string, boolean>;
  onAddLocalServer: () => void;
  onAddRemoteServer: () => void;
  onRemoveLocalServer: (server: DuneVmCandidate) => void;
  onRefreshLocalServer: (server: DuneVmCandidate) => void;
  onStartLocalServer: (server: DuneVmCandidate) => void;
  onStopLocalServer: (server: DuneVmCandidate) => void;
  onRemoveRemoteServer: (server: RemoteServerRecord) => void;
  onRefreshRemoteStatus: (server: RemoteServerRecord) => void;
  onStartRemoteBattlegroup: (server: RemoteServerRecord) => void;
  onStopRemoteBattlegroup: (server: RemoteServerRecord) => void;
  onUpdateRemoteBattlegroup: (server: RemoteServerRecord) => void;
  onRefreshProxmoxVm: (server: RemoteServerRecord) => void;
  onStartProxmoxVm: (server: RemoteServerRecord) => void;
  onStopProxmoxVm: (server: RemoteServerRecord) => void;
  onStartLocalBattlegroup: (server: DuneVmCandidate) => void;
  onStopLocalBattlegroup: (server: DuneVmCandidate) => void;
  onUpdateLocalBattlegroup: (server: DuneVmCandidate) => void;
  onStartTunnel: (request: ServerTunnelStartRequest) => void;
  onStopTunnel: (tunnelId: string) => void;
  onOpenTunnel: (tunnel: ServerTunnelStatus) => void;
  onRefreshRemoteComponentLog: (server: RemoteServerRecord, component: RemoteServerComponent) => void;
  onRestartRemoteComponent: (server: RemoteServerRecord, component: RemoteServerComponent) => void;
  onRefreshLocalComponentLog: (server: DuneVmCandidate, component: RemoteServerComponent) => void;
  onRestartLocalComponent: (server: DuneVmCandidate, component: RemoteServerComponent) => void;
}) {
  return (
    <Card size="3" variant="surface" className="pane page-pane">
      <Flex direction="column" gap="4" height="100%" minHeight="0">
        <Flex align="center" justify="between" gap="3">
          <Box>
            <Heading size="5">Servers</Heading>
            <Text as="p" size="2" color="gray" mb="0">
              Setup and basic management run through the desktop app and CLI tooling.
            </Text>
          </Box>
          <Flex gap="2" wrap="wrap" justify="end">
            <Button type="button" variant="surface" onClick={onAddLocalServer}>
              Add local Hyper-V server
            </Button>
            <Button type="button" variant="surface" onClick={onAddRemoteServer}>
              Add remote server
            </Button>
          </Flex>
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
                    runtime={localRuntimes[localServerKey(candidate)]}
                    runtimeError={localRuntimeErrors[localServerKey(candidate)]}
                    packageStatus={serverPackageStatus}
                    componentLogs={remoteComponentLogs}
                    componentLogBusy={remoteComponentLogBusy}
                    componentRestartBusy={remoteComponentRestartBusy}
                    busyLabel={remoteBusy[localServerKey(candidate)]}
                    tunnels={tunnels}
                    tunnelBusy={tunnelBusy}
                    onRemove={() => onRemoveLocalServer(candidate)}
                    onRefresh={() => onRefreshLocalServer(candidate)}
                    onStart={() => onStartLocalServer(candidate)}
                    onStop={() => onStopLocalServer(candidate)}
                    onStartBattlegroup={() => onStartLocalBattlegroup(candidate)}
                    onStopBattlegroup={() => onStopLocalBattlegroup(candidate)}
                    onUpdateBattlegroup={() => onUpdateLocalBattlegroup(candidate)}
                    onStartTunnel={onStartTunnel}
                    onStopTunnel={onStopTunnel}
                    onOpenTunnel={onOpenTunnel}
                    onRefreshComponentLog={(component) => onRefreshLocalComponentLog(candidate, component)}
                    onRestartComponent={(component) => onRestartLocalComponent(candidate, component)}
                  />
                ))}
                {remoteServers.map((server) => (
                  <RemoteServerCard
                    key={server.id}
                    server={server}
                    compact
                    status={remoteStatuses[server.id]}
                    proxmoxVmStatus={proxmoxVmStatuses[server.id]}
                    components={remoteComponents[server.id] ?? []}
                    componentLogs={remoteComponentLogs}
                    componentLogBusy={remoteComponentLogBusy}
                    componentRestartBusy={remoteComponentRestartBusy}
                    statusError={remoteStatusErrors[server.id]}
                    packageStatus={serverPackageStatus}
                    busyLabel={remoteBusy[server.id]}
                    tunnels={tunnels}
                    tunnelBusy={tunnelBusy}
                    onRemove={() => onRemoveRemoteServer(server)}
                    onRefresh={() => onRefreshRemoteStatus(server)}
                    onStartBattlegroup={() => onStartRemoteBattlegroup(server)}
                    onStopBattlegroup={() => onStopRemoteBattlegroup(server)}
                    onUpdateBattlegroup={() => onUpdateRemoteBattlegroup(server)}
                    onRefreshProxmoxVm={() => onRefreshProxmoxVm(server)}
                    onStartProxmoxVm={() => onStartProxmoxVm(server)}
                    onStopProxmoxVm={() => onStopProxmoxVm(server)}
                    onStartTunnel={onStartTunnel}
                    onStopTunnel={onStopTunnel}
                    onOpenTunnel={onOpenTunnel}
                    onRefreshComponentLog={(component) => onRefreshRemoteComponentLog(server, component)}
                    onRestartComponent={(component) => onRestartRemoteComponent(server, component)}
                  />
                ))}
              </>
            ) : (
              <EmptyState
                title="No Dune servers detected"
                body="Create a new server or add a remote server profile."
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
  runtime,
  runtimeError,
  packageStatus,
  componentLogs,
  componentLogBusy,
  componentRestartBusy,
  busyLabel,
  tunnels,
  tunnelBusy,
  onRemove,
  onRefresh,
  onStart,
  onStop,
  onStartBattlegroup,
  onStopBattlegroup,
  onUpdateBattlegroup,
  onStartTunnel,
  onStopTunnel,
  onOpenTunnel,
  onRefreshComponentLog,
  onRestartComponent,
}: {
  candidate: DuneVmCandidate;
  compact?: boolean;
  runtime?: LocalHyperVRuntime;
  runtimeError?: string;
  packageStatus: ServerPackageStatus | null;
  componentLogs: Record<string, string>;
  componentLogBusy: Record<string, boolean>;
  componentRestartBusy: Record<string, boolean>;
  busyLabel?: string;
  tunnels: Record<string, ServerTunnelStatus>;
  tunnelBusy: Record<string, boolean>;
  onRemove?: () => void;
  onRefresh?: () => void;
  onStart?: () => void;
  onStop?: () => void;
  onStartBattlegroup?: () => void;
  onStopBattlegroup?: () => void;
  onUpdateBattlegroup?: () => void;
  onStartTunnel?: (request: ServerTunnelStartRequest) => void;
  onStopTunnel?: (tunnelId: string) => void;
  onOpenTunnel?: (tunnel: ServerTunnelStatus) => void;
  onRefreshComponentLog?: (component: RemoteServerComponent) => void;
  onRestartComponent?: (component: RemoteServerComponent) => void;
}) {
  const vm = candidate.vm;
  const primaryIp = vm.ipv4Addresses[0] ?? "No IPv4 reported";
  const diskLabel = vm.diskSizeBytes > 0 ? `${formatGiB(vm.diskSizeBytes)} disk` : "Disk size unknown";
  const usedDiskLabel = vm.diskFileSizeBytes > 0 ? `${formatGiB(vm.diskFileSizeBytes)} used` : "usage unknown";
  const busy = !!busyLabel;
  const canStart = vm.state === "off" || vm.state === "saved" || vm.state === "paused";
  const canStop = vm.state === "running" || vm.state === "starting" || vm.state === "paused";
  const battlegroup = runtime?.status?.battlegroup;
  const guestPackage = runtime?.status?.package;
  const serverUpdateRequired = serverPackageUpdateRequired(guestPackage, packageStatus);
  const battlegroupStarted = battlegroup ? isBattlegroupStarted(battlegroup) : false;
  const battlegroupStartRequested = battlegroup ? !battlegroup.stop : false;
  const battlegroupStopped = battlegroup ? battlegroup.stop : false;
  const runtimeComponents = Array.isArray(runtime?.components) ? runtime.components : [];
  const serverKey = localServerKey(candidate);
  const statusBadgeColor = runtimeError
    ? "red"
    : battlegroup
      ? battlegroupStarted
        ? "green"
        : battlegroupStartRequested
          ? "amber"
          : battlegroupStopped
            ? "gray"
            : "green"
      : vm.state === "running"
        ? "green"
        : vm.state === "off"
          ? "gray"
          : "amber";
  const statusBadgeLabel = runtimeError
    ? "Check failed"
    : battlegroup
      ? battlegroupStarted
        ? "Started"
        : battlegroupStartRequested
          ? "Starting"
          : "Stopped"
      : vm.state;

  return (
    <Box className="server-card">
      <Flex align="start" justify="between" gap="3">
        <Box>
          <Flex align="center" gap="2">
            <Heading size={compact ? "3" : "4"}>{vm.name}</Heading>
            <Badge color="bronze" variant="soft">
              Hyper-V
            </Badge>
            <Badge color={candidate.confidence === "high" ? "green" : candidate.confidence === "medium" ? "amber" : "gray"} variant="soft">
              {candidate.confidence}
            </Badge>
          </Flex>
          <Text as="div" size="2" color="gray">
            {primaryIp} · {runtime?.battlegroupName || "setup pending"}
          </Text>
        </Box>
        <Flex align="center" gap="2">
          <Button
            type="button"
            size="1"
            variant="surface"
            disabled={busy}
            onClick={(event) => {
              event.stopPropagation();
              onRefresh?.();
            }}
          >
            Refresh
          </Button>
          <Badge color={statusBadgeColor} variant="surface">
            {busy ? (
              <Flex align="center" gap="1">
                <BusySpinner /> {busyLabel}
              </Flex>
            ) : (
              statusBadgeLabel
            )}
          </Badge>
          <Button
            type="button"
            size="1"
            color="red"
            variant="soft"
            disabled={busy}
            onClick={(event) => {
              event.stopPropagation();
              onRemove?.();
            }}
          >
            Forget
          </Button>
        </Flex>
      </Flex>

      <Grid columns={compact ? "2" : "5"} gap="3" mt="3">
        <Metric label="Namespace" value={runtime?.namespace || "pending"} />
        <Metric label="BattleGroup" value={runtime?.battlegroupName || "pending"} />
        <Metric label="Type" value="Local Hyper-V VM" />
        <Metric label="Guest IP" value={primaryIp} />
        <Metric label="VM State" value={vm.rawState || vm.state} />
      </Grid>
      <Grid columns={compact ? "2" : "5"} gap="3" mt="3">
        <Metric label="Memory" value={formatGiB(vm.memoryAssignedBytes)} />
        <Metric label="CPU" value={vm.processorCount ? `${vm.processorCount} cores` : "unknown"} />
        <Metric label="Disk" value={`${diskLabel}; ${usedDiskLabel}`} />
        <Metric label="Switch" value={vm.switchNames.join(", ") || "none"} />
        <Metric label="Uptime" value={formatDuration(vm.uptimeSeconds)} />
      </Grid>
      <ServerPackageCardStatus guestPackage={guestPackage} packageStatus={packageStatus} />
      {runtimeError ? (
        <Box className="server-error" mt="3">
          <Text size="2">{runtimeError}</Text>
        </Box>
      ) : null}
      {packageStatus?.updateAvailable ? (
        <Box className="setup-guide" mt="3">
          <Text size="2">
            Server package build {packageStatus.latestBuildId || "latest"} is available. Stop the BattleGroup fully before updating this VM.
          </Text>
        </Box>
      ) : null}
      {runtime && battlegroup ? (
        <Box className="server-state" mt="3">
          <Grid columns="2" gap="3">
            <Metric
              label="BattleGroup State"
              value={`${battlegroup.phase || "unknown"}; stop=${battlegroup.stop ? "true" : "false"}`}
            />
            <Metric label="Director" value={battlegroup.directorPhase || "unknown"} />
            <Metric label="Server Group" value={battlegroup.serverGroupPhase || "unknown"} />
          </Grid>
          <ComponentHealthList
            serverKey={serverKey}
            components={runtimeComponents}
            logs={componentLogs}
            logBusy={componentLogBusy}
            restartBusy={componentRestartBusy}
            onRefreshLog={onRefreshComponentLog}
            onRestart={onRestartComponent}
          />
        </Box>
      ) : null}
      <ServerTunnelControls
        serverKey={serverKey}
        namespace={runtime?.namespace ?? ""}
        host={primaryLocalServerIp(candidate)}
        serverKind="hyperv"
        vmName={vm.name}
        canStartDirectorTunnel={!!battlegroup && !battlegroup.stop && isDirectorReadyPhase(battlegroup.directorPhase)}
        canStartFileBrowserTunnel={!!battlegroup && !battlegroup.stop}
        tunnels={tunnels}
        tunnelBusy={tunnelBusy}
        onStartTunnel={onStartTunnel}
        onStopTunnel={onStopTunnel}
        onOpenTunnel={onOpenTunnel}
      />
      <Flex align="center" justify="between" gap="2" mt="3" wrap="wrap">
        <Flex gap="2" wrap="wrap">
          <Button
            size="1"
            variant="surface"
            disabled={busy || !runtime || !battlegroupStopped}
            onClick={onStartBattlegroup}
          >
            Start BattleGroup
          </Button>
          <Button
            size="1"
            variant="surface"
            disabled={busy || !runtime || !battlegroupStartRequested}
            onClick={onStopBattlegroup}
          >
            Stop BattleGroup
          </Button>
          {serverUpdateRequired ? (
            <Button
              size="2"
              color="amber"
              variant="solid"
              disabled={busy || !runtime || !packageStatus?.complete}
              onClick={onUpdateBattlegroup}
            >
              Update Server
            </Button>
          ) : null}
          <Button size="1" variant="surface" disabled={busy || !canStart} onClick={onStart}>
            Start VM
          </Button>
          <Button size="1" variant="surface" disabled={busy || !canStop} onClick={onStop}>
            Stop VM
          </Button>
        </Flex>
        {busyLabel ? (
          <Text size="1" color="gray" className="mono">
            {busyLabel}
          </Text>
        ) : null}
      </Flex>
    </Box>
  );
}

function RemoteServerCard({
  server,
  compact = false,
  onRemove,
  status,
  proxmoxVmStatus,
  components,
  packageStatus,
  componentLogs,
  componentLogBusy,
  componentRestartBusy,
  statusError,
  busyLabel,
  tunnels,
  tunnelBusy,
  onRefresh,
  onStartBattlegroup,
  onStopBattlegroup,
  onUpdateBattlegroup,
  onRefreshProxmoxVm,
  onStartProxmoxVm,
  onStopProxmoxVm,
  onStartTunnel,
  onStopTunnel,
  onOpenTunnel,
  onRefreshComponentLog,
  onRestartComponent,
}: {
  server: RemoteServerRecord;
  compact?: boolean;
  onRemove?: () => void;
  status?: RemoteServerStatus;
  proxmoxVmStatus?: ProxmoxVmStatus;
  components: RemoteServerComponent[];
  packageStatus: ServerPackageStatus | null;
  componentLogs: Record<string, string>;
  componentLogBusy: Record<string, boolean>;
  componentRestartBusy: Record<string, boolean>;
  statusError?: string;
  busyLabel?: string;
  tunnels: Record<string, ServerTunnelStatus>;
  tunnelBusy: Record<string, boolean>;
  onRefresh?: () => void;
  onStartBattlegroup?: () => void;
  onStopBattlegroup?: () => void;
  onUpdateBattlegroup?: () => void;
  onRefreshProxmoxVm?: () => void;
  onStartProxmoxVm?: () => void;
  onStopProxmoxVm?: () => void;
  onStartTunnel?: (request: ServerTunnelStartRequest) => void;
  onStopTunnel?: (tunnelId: string) => void;
  onOpenTunnel?: (tunnel: ServerTunnelStatus) => void;
  onRefreshComponentLog?: (component: RemoteServerComponent) => void;
  onRestartComponent?: (component: RemoteServerComponent) => void;
}) {
  const liveStatus = statusError ? undefined : status;
  const guestPackage = liveStatus?.package;
  const serverUpdateRequired = serverPackageUpdateRequired(guestPackage, packageStatus);
  const liveComponents = liveStatus ? components : [];
  const battlegroupStarted = liveStatus ? isBattlegroupStarted(liveStatus.battlegroup) : false;
  const battlegroupStartRequested = liveStatus ? !liveStatus.battlegroup.stop : false;
  const battlegroupStopped = liveStatus ? liveStatus.battlegroup.stop : false;
  const busy = !!busyLabel;
  return (
    <Box className="server-card">
      <Flex align="start" justify="between" gap="3">
        <Box>
          <Flex align="center" gap="2">
            <Heading size={compact ? "3" : "4"}>{server.name}</Heading>
            <Badge color="bronze" variant="soft">
              {remoteServerKindLabel(server.type)}
            </Badge>
          </Flex>
          <Text as="div" size="2" color="gray">
            {server.host} · {server.battlegroupName || "setup pending"}
          </Text>
        </Box>
        <Flex align="center" gap="2">
          <Button
            type="button"
            size="1"
            variant="surface"
            disabled={busy}
            onClick={(event) => {
              event.stopPropagation();
              onRefresh?.();
            }}
          >
            Refresh
          </Button>
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
              : busyLabel
                ? "Retrieving"
                : liveStatus
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
        <Metric label="Namespace" value={server.namespace || "pending"} />
        <Metric label="BattleGroup" value={server.battlegroupName || "pending"} />
        <Metric label="Type" value={server.type === "alpine" ? "Alpine VM over SSH" : "Ubuntu over SSH"} />
        <Metric label="Created" value={new Date(server.createdAt).toLocaleString()} />
        {server.provisioner ? <Metric label="Proxmox VM" value={`${server.provisioner.node}/${server.provisioner.vmid}`} /> : null}
      </Grid>
      {server.provisioner ? (
        <Flex align="center" gap="2" mt="3" wrap="wrap">
          <Badge color={proxmoxVmStatus?.status === "running" ? "green" : "gray"} variant="surface">
            VM {proxmoxVmStatus?.status || "unknown"}
          </Badge>
          <Button size="1" variant="surface" disabled={busy} onClick={onRefreshProxmoxVm}>
            VM Status
          </Button>
          <Button size="1" variant="surface" disabled={busy || proxmoxVmStatus?.status === "running"} onClick={onStartProxmoxVm}>
            Start VM
          </Button>
          <Button size="1" variant="surface" disabled={busy || proxmoxVmStatus?.status === "stopped"} onClick={onStopProxmoxVm}>
            Stop VM
          </Button>
        </Flex>
      ) : null}
      <ServerPackageCardStatus guestPackage={guestPackage} packageStatus={packageStatus} />
      {busyLabel ? (
        <Flex align="center" gap="2" mt="3">
          <BusySpinner />
          <Text size="2" color="gray">
            {busyLabel}
          </Text>
        </Flex>
      ) : null}
      {statusError ? (
        <Box className="server-error" mt="3">
          <Text size="2">{statusError}</Text>
        </Box>
      ) : null}
      {packageStatus?.updateAvailable ? (
        <Box className="setup-guide" mt="3">
          <Text size="2">
            Server package build {packageStatus.latestBuildId || "latest"} is available. Stop the BattleGroup fully before updating this server.
          </Text>
        </Box>
      ) : null}
      <Box className="server-state" mt="3">
        <Grid columns="2" gap="3">
          <Metric
            label="BattleGroup State"
            value={
              liveStatus
                ? `${liveStatus.battlegroup.phase || "unknown"}; stop=${liveStatus.battlegroup.stop ? "true" : "false"}`
                : statusError || "Checking"
            }
          />
          <Metric
            label="Director"
            value={liveStatus ? liveStatus.battlegroup.directorPhase || "unknown" : statusError || "Checking"}
          />
          <Metric
            label="Server Group"
            value={liveStatus ? liveStatus.battlegroup.serverGroupPhase || "unknown" : statusError || "Checking"}
          />
        </Grid>
        <ComponentHealthList
          serverKey={server.id}
          components={liveComponents}
          logs={componentLogs}
          logBusy={componentLogBusy}
          restartBusy={componentRestartBusy}
          onRefreshLog={onRefreshComponentLog}
          onRestart={onRestartComponent}
        />
        <ServerTunnelControls
          serverKey={server.id}
          namespace={server.namespace}
          host={server.host}
          serverKind={server.type}
          user={server.user || remoteServerDefaultUser(server.type)}
          keyPath={server.type === "ubuntu" ? server.keyPath : undefined}
          canStartDirectorTunnel={!!liveStatus && !liveStatus.battlegroup.stop && isDirectorReadyPhase(liveStatus.battlegroup.directorPhase)}
          canStartFileBrowserTunnel={!!liveStatus && !liveStatus.battlegroup.stop}
          tunnels={tunnels}
          tunnelBusy={tunnelBusy}
          onStartTunnel={onStartTunnel}
          onStopTunnel={onStopTunnel}
          onOpenTunnel={onOpenTunnel}
        />
        <Flex align="center" justify="between" gap="2" mt="3" wrap="wrap">
          <Flex gap="2" wrap="wrap">
            <Button size="1" variant="surface" disabled={busy} onClick={onRefresh}>
              Refresh
            </Button>
            <Button
              size="1"
              variant="surface"
              disabled={busy || !liveStatus || !battlegroupStopped}
              onClick={onStartBattlegroup}
            >
              Start BattleGroup
            </Button>
            <Button
              size="1"
              variant="surface"
              disabled={busy || !liveStatus || !battlegroupStartRequested}
              onClick={onStopBattlegroup}
            >
              Stop BattleGroup
            </Button>
            {serverUpdateRequired ? (
              <Button
                size="2"
                color="amber"
                variant="solid"
                disabled={busy || !liveStatus}
                onClick={onUpdateBattlegroup}
              >
                Update Server
              </Button>
            ) : null}
          </Flex>
          {busyLabel ? (
            <Text size="1" color="gray" className="mono">
              {busyLabel}
            </Text>
          ) : null}
        </Flex>
      </Box>
    </Box>
  );
}

function ServerTunnelControls({
  serverKey,
  namespace,
  host,
  serverKind,
  vmName,
  user,
  keyPath,
  canStartDirectorTunnel,
  canStartFileBrowserTunnel,
  tunnels,
  tunnelBusy,
  onStartTunnel,
  onStopTunnel,
  onOpenTunnel,
}: {
  serverKey: string;
  namespace: string;
  host: string;
  serverKind: "hyperv" | "ubuntu" | "alpine";
  vmName?: string;
  user?: string;
  keyPath?: string;
  canStartDirectorTunnel: boolean;
  canStartFileBrowserTunnel: boolean;
  tunnels: Record<string, ServerTunnelStatus>;
  tunnelBusy: Record<string, boolean>;
  onStartTunnel?: (request: ServerTunnelStartRequest) => void;
  onStopTunnel?: (tunnelId: string) => void;
  onOpenTunnel?: (tunnel: ServerTunnelStatus) => void;
}) {
  const services: Array<{ service: TunnelService; label: string }> = [
    { service: "director", label: "Director UI" },
    { service: "fileBrowser", label: "File Browser" },
  ];
  return (
    <Box className="tunnel-controls" mt="3">
      <Flex direction="column" gap="2">
        {services.map(({ service, label }) => {
          const tunnelId = serverTunnelKey(serverKey, service);
          const active = tunnels[tunnelId];
          const busy = !!tunnelBusy[tunnelId];
          const serviceAvailable = service === "director" ? canStartDirectorTunnel : canStartFileBrowserTunnel;
          const disabled =
            busy || !onStopTunnel || (!active && (!serviceAvailable || !host.trim() || !namespace.trim() || !onStartTunnel));
          return (
            <Flex key={service} align="center" justify="between" gap="3" wrap="wrap" className="tunnel-row">
              <Flex direction="column" gap="1" minWidth="0">
                <Text size="2" weight="medium">
                  {label}
                </Text>
                <Text size="1" color="gray">
                  {active
                    ? `Forwarding remote port ${active.remotePort} to local port ${active.localPort}`
                    : !serviceAvailable
                      ? service === "director"
                        ? "Requires started BattleGroup and healthy Director"
                        : "Requires started BattleGroup"
                      : !host.trim() || !namespace.trim()
                        ? "Requires detected server namespace and IP"
                        : "Tunnel stopped"}
                </Text>
              </Flex>
              <Flex align="center" gap="2" wrap="wrap" justify="end">
                {active ? (
                  <Button
                    type="button"
                    size="1"
                    variant="surface"
                    onClick={() => onOpenTunnel?.(active)}
                  >
                    Open {label}
                  </Button>
                ) : null}
                <Button
                  type="button"
                  size="1"
                  variant={active ? "soft" : "surface"}
                  color={active ? "red" : undefined}
                  disabled={disabled}
                  onClick={() => {
                    if (active) {
                      onStopTunnel?.(tunnelId);
                      return;
                    }
                    onStartTunnel?.({
                      tunnelId,
                      serverKind,
                      service,
                      host,
                      user,
                      keyPath,
                      vmName,
                      namespace,
                    });
                  }}
                >
                  {busy ? (
                    <Flex align="center" gap="1">
                      <BusySpinner /> Working
                    </Flex>
                  ) : active ? (
                    `Stop Tunnel`
                  ) : (
                    `Start Tunnel`
                  )}
                </Button>
                {active ? (
                  <Link
                    size="1"
                    href="#"
                    className="mono tunnel-url"
                    onClick={(event) => {
                      event.preventDefault();
                      onOpenTunnel?.(active);
                    }}
                  >
                    {active.url}
                  </Link>
                ) : null}
              </Flex>
            </Flex>
          );
        })}
      </Flex>
    </Box>
  );
}

function BusySpinner() {
  return <Box className="inline-spinner" aria-hidden />;
}

function ComponentHealthList({
  serverKey,
  components,
  logs,
  logBusy,
  restartBusy,
  onRefreshLog,
  onRestart,
}: {
  serverKey: string;
  components: RemoteServerComponent[];
  logs: Record<string, string>;
  logBusy: Record<string, boolean>;
  restartBusy: Record<string, boolean>;
  onRefreshLog?: (component: RemoteServerComponent) => void;
  onRestart?: (component: RemoteServerComponent) => void;
}) {
  if (components.length === 0) return null;
  const systems = components.filter((component) => component.category !== "map");
  const maps = components.filter((component) => component.category === "map");
  return (
    <Box className="component-health" mt="3">
      <Flex direction="column" gap="3">
        <ComponentHealthGroup
          title="Systems"
          serverKey={serverKey}
          components={systems}
          logs={logs}
          logBusy={logBusy}
          restartBusy={restartBusy}
          onRefreshLog={onRefreshLog}
          onRestart={onRestart}
        />
        <ComponentHealthGroup
          title="Maps"
          serverKey={serverKey}
          components={maps}
          logs={logs}
          logBusy={logBusy}
          restartBusy={restartBusy}
          onRefreshLog={onRefreshLog}
          onRestart={onRestart}
        />
      </Flex>
    </Box>
  );
}

function ComponentHealthGroup({
  title,
  serverKey,
  components,
  logs,
  logBusy,
  restartBusy,
  onRefreshLog,
  onRestart,
}: {
  title: string;
  serverKey: string;
  components: RemoteServerComponent[];
  logs: Record<string, string>;
  logBusy: Record<string, boolean>;
  restartBusy: Record<string, boolean>;
  onRefreshLog?: (component: RemoteServerComponent) => void;
  onRestart?: (component: RemoteServerComponent) => void;
}) {
  if (components.length === 0) return null;
  return (
    <details className="component-group">
      <summary className="component-group-summary">
        <Flex align="center" justify="between" gap="2">
          <Text size="1" weight="medium" color="gray" className="component-group-title">
            {title}
          </Text>
          <Badge color="gray" variant="soft">
            {components.length}
          </Badge>
        </Flex>
      </summary>
      <Flex direction="column" gap="2" mt="2">
        {components.map((component) => {
          const logKey = componentLogStateKey(serverKey, component);
          const logText = logs[logKey];
          const busy = !!logBusy[logKey];
          const restarting = !!restartBusy[logKey];
          return (
            <details key={`${component.logKey}-${component.name}`} className="component-row">
              <summary className="component-summary">
                <Flex align="center" justify="between" gap="3" width="100%">
                  <Box minWidth="0">
                    <Flex align="center" gap="2" wrap="wrap">
                      <Text size="2" weight="medium">
                        {component.name}
                      </Text>
                      <Badge color={component.tone} variant="soft">
                        {component.state}
                      </Badge>
                    </Flex>
                    <Text as="div" size="2" color="gray" className="component-summary-text">
                      {component.summary}
                    </Text>
                  </Box>
                  <Flex gap="2" style={{ flexShrink: 0 }}>
                    <Button
                      type="button"
                      size="1"
                      variant="surface"
                      disabled={busy || restarting}
                      onClick={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        const row = event.currentTarget.closest("details");
                        if (row) row.open = true;
                        onRefreshLog?.(component);
                      }}
                    >
                      {busy ? "Loading logs" : logText ? "Refresh logs" : "View logs"}
                    </Button>
                    <Button
                      type="button"
                      size="1"
                      color={isCriticalRestartComponent(component) ? "amber" : "bronze"}
                      variant="soft"
                      disabled={busy || restarting}
                      onClick={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        const row = event.currentTarget.closest("details");
                        if (row) row.open = true;
                        onRestart?.(component);
                      }}
                    >
                      {restarting ? "Restarting" : "Restart"}
                    </Button>
                  </Flex>
                </Flex>
              </summary>
              <Box className="component-body">
                {component.details.length > 0 ? (
                  <ul className="component-details">
                    {component.details.map((detail) => (
                      <li key={detail}>{detail}</li>
                    ))}
                  </ul>
                ) : (
                  <Text as="div" size="1" color="gray">
                    No additional details reported.
                  </Text>
                )}
                {logText ? (
                  <>
                    <Flex justify="end" mt="2">
                      <Button
                        type="button"
                        size="1"
                        variant="soft"
                        onClick={() => void copyTextToClipboard(logText)}
                      >
                        Copy logs
                      </Button>
                    </Flex>
                    <Box className="component-log" mt="2">
                      {logText.split(/\r?\n/).map((line, index) => (
                        <Text as="div" size="1" className="mono" key={`${component.logKey}-${index}`}>
                          {line || "\u00a0"}
                        </Text>
                      ))}
                    </Box>
                  </>
                ) : null}
              </Box>
            </details>
          );
        })}
      </Flex>
    </details>
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

function ServerPackageCardStatus({
  guestPackage,
  packageStatus,
}: {
  guestPackage?: RemoteServerPackageStatus;
  packageStatus: ServerPackageStatus | null;
}) {
  if (!guestPackage && !packageStatus) return null;
  const installed = guestPackage?.installedBuildId || null;
  const latest = packageStatus?.latestBuildId || packageStatus?.installedBuildId || null;
  const downloadedImage = guestPackage?.battlegroupVersion || null;
  const liveImage = guestPackage?.liveBattlegroupVersion || null;
  const updateRequired = Boolean(installed && latest && installed !== latest);
  const tone = !installed ? "amber" : updateRequired ? "amber" : "green";
  const label = !installed ? "Build unknown" : updateRequired ? "Update required" : "Current";
  return (
    <Flex align="center" gap="2" mt="3" wrap="wrap">
      <Metric label="Server Package" value={installed || "unknown"} />
      <Badge color={tone} variant="surface">
        {label}
      </Badge>
      {latest ? (
        <Text size="1" color="gray" className="mono">
          latest {latest}
        </Text>
      ) : null}
      {downloadedImage ? (
        <Text size="1" color="gray" className="mono">
          images {downloadedImage}
        </Text>
      ) : null}
      {liveImage && liveImage !== downloadedImage ? (
        <Text size="1" color="gray" className="mono">
          live {liveImage}
        </Text>
      ) : null}
    </Flex>
  );
}

function serverPackageUpdateRequired(
  guestPackage: RemoteServerPackageStatus | undefined,
  packageStatus: ServerPackageStatus | null,
): boolean {
  const installed = guestPackage?.installedBuildId?.trim();
  const latest = (packageStatus?.latestBuildId || packageStatus?.installedBuildId || "").trim();
  return Boolean(installed && latest && installed !== latest);
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

function ToolsPage({
  generatedSshKey,
  sshKeyGenerationRunning,
  onGenerateUbuntuSshKey,
}: {
  generatedSshKey: GenerateSshKeyResult | null;
  sshKeyGenerationRunning: boolean;
  onGenerateUbuntuSshKey: () => void;
}) {
  return (
    <Card size="3" variant="surface" className="pane setup-pane">
      <Flex direction="column" gap="4" height="100%" minHeight="0">
        <Box>
          <Heading size="5">Tools</Heading>
          <Text as="p" size="2" color="gray">
            Utilities for preparing hosts before server setup.
          </Text>
        </Box>
        <Box className="setup-scroll">
          <SetupSection icon={DesktopIcon} title="Ubuntu SSH Key Pair">
            <Flex direction="column" gap="3">
              <Text size="2" color="gray">
                Generate an Ed25519 key pair for Ubuntu VPS setup. Upload the public key to your hosting provider,
                then use the private key path during Remote Ubuntu detection.
              </Text>
              <Button
                type="button"
                variant="surface"
                onClick={onGenerateUbuntuSshKey}
                disabled={sshKeyGenerationRunning}
              >
                {sshKeyGenerationRunning ? "Generating..." : "Generate SSH key pair"}
              </Button>
              {generatedSshKey ? (
                <Box className="generated-key-box">
                  <Text as="div" size="2" weight="medium">
                    Public key to upload to your host
                  </Text>
                  <TextArea readOnly value={generatedSshKey.publicKey} />
                  <Grid columns="140px 1fr" gap="2" mt="3">
                    <Text size="2" color="gray">
                      Private key
                    </Text>
                    <Text size="2" className="mono metric-value">
                      {generatedSshKey.privateKeyPath}
                    </Text>
                    <Text size="2" color="gray">
                      Public key
                    </Text>
                    <Text size="2" className="mono metric-value">
                      {generatedSshKey.publicKeyPath}
                    </Text>
                  </Grid>
                </Box>
              ) : null}
            </Flex>
          </SetupSection>
        </Box>
      </Flex>
    </Card>
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
  proxmoxDetection,
  proxmoxDetectionStatus,
  serverPackageStatus,
  serverPackageCheckStatus,
  update,
  onUpdateServerPackage,
  onLocalDetection,
  onRemotePreflight,
  onProxmoxDetection,
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
  proxmoxDetection: ProxmoxDetection | null;
  proxmoxDetectionStatus: DetectionState;
  serverPackageStatus: ServerPackageStatus | null;
  serverPackageCheckStatus: ServerPackageCheckStatus;
  update: <K extends keyof SetupForm>(key: K, value: SetupForm[K]) => void;
  onUpdateServerPackage: () => void;
  onLocalDetection: () => void;
  onRemotePreflight: () => void;
  onProxmoxDetection: () => void;
  onStart: () => void;
}) {
  const deepDesertEnabled = layoutPreview.deepDesertTotal > 0;
  const warmOptions = zeroTo(layoutPreview.deepDesertTotal);
  const vmMemoryGb =
    form.setupTarget === "proxmox"
      ? effectiveProxmoxVmMemoryGb(form, calculatedMemory, proxmoxDetection)
      : effectiveVmMemoryGb(form, calculatedMemory);
  const requirements =
    form.setupTarget === "ubuntu"
      ? remoteSetupRequirementStatus(
          calculatedMemory,
          form.diskGb,
          form.processorCount,
          remotePreflight,
          form.enableSwap,
        )
      : form.setupTarget === "proxmox"
        ? proxmoxSetupRequirementStatus(
            calculatedMemory,
            vmMemoryGb,
            form.diskGb,
            form.processorCount,
            form.proxmoxNode,
            form.proxmoxVmStorage,
            proxmoxDetection,
          )
      : setupRequirementStatus(
          calculatedMemory,
          form.vmMemoryGb,
          form.diskGb,
          form.processorCount,
          form.vmDestination,
          hostReadiness,
          driveCandidates,
        );
  const hasServiceToken = form.tokenSource.trim().length > 0;
  const setupNeedsServerPackage = form.setupTarget === "hyperv" || form.setupTarget === "proxmox";
  const serverPackageCurrent =
    !!serverPackageStatus?.complete &&
    !serverPackageStatus.updateAvailable &&
    serverPackageCheckStatus === "current";
  const serverPackageBusy =
    serverPackageCheckStatus === "checking" || serverPackageCheckStatus === "updating";
  const packageBlocksSetup =
    setupNeedsServerPackage &&
    !serverPackageCurrent &&
    (serverPackageCheckStatus === "idle" ||
      serverPackageCheckStatus === "failed" ||
      serverPackageBusy ||
      !serverPackageStatus?.complete ||
      !!serverPackageStatus.updateAvailable);
  const packageActionLabel =
    serverPackageCheckStatus === "checking"
      ? "Checking..."
      : serverPackageCheckStatus === "updating"
        ? "Updating..."
        : serverPackageCheckStatus === "available" || serverPackageCheckStatus === "missing"
          ? "Update package"
          : "Check package";
  const setupIssues =
    form.setupTarget === "ubuntu"
      ? remoteSetupBlockingIssues(requirements, hasServiceToken, form, remotePreflight)
      : form.setupTarget === "proxmox"
        ? proxmoxSetupBlockingIssues(requirements, hasServiceToken, form, proxmoxDetection)
      : setupBlockingIssues(environmentGate, requirements, hasServiceToken, vmDestinationHasVm, form);
  const visibleSetupIssues = setupIssueSummary(form.setupTarget, setupIssues, proxmoxDetection);
  const canStart = setupIssues.length === 0 && !packageBlocksSetup;
  const hypervDetectionReady = networkDetection === "ready" && environmentGate.canContinue;
  const ubuntuDetectionReady = remotePreflightStatus === "ready" && !!remotePreflight;
  const proxmoxDetectionReady = proxmoxDetectionStatus === "ready" && !!proxmoxDetection;
  const flowDetectionReady =
    form.setupTarget === "ubuntu"
      ? ubuntuDetectionReady
      : form.setupTarget === "proxmox"
        ? proxmoxDetectionReady
        : hypervDetectionReady;

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
                  onValueChange={(value) => {
                    const target = value as SetupTarget;
                    update("setupTarget", target);
                    if (target === "ubuntu") {
                      update("playerIpMode", "external");
                      update("playerIp", form.playerIp || form.remoteHost);
                    } else if (target === "proxmox") {
                      update("networkMode", "dhcp");
                      update("playerIpMode", "external");
                    }
                  }}
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
                    DO NOT USE AN EXISTING SERVER, ALWAYS CREATE A FRESH SERVER, WE ARE NOT RESPONSIBLE OF ANY DATA LOSS YOU MIGHT ENCOUNTER!
                  </Text>
                  <Text as="p" size="2" color="gray">
                    Remote setup installs packages, creates users, configures k3s, downloads server files, opens
                    service ports, and writes system configuration. Use a clean Ubuntu host dedicated to this Dune
                    server so setup cannot conflict with existing workloads or data.
                  </Text>
                </Box>
              ) : form.setupTarget === "proxmox" ? (
                <Box className="destructive-warning" mt="3">
                  <Text as="div" size="2" weight="medium">
                    Proxmox setup creates a new VM and uploads a converted vendor disk image.
                  </Text>
                  <Text as="p" size="2" color="gray">
                    Use a dedicated VMID and storage target. The guest boots with DHCP first, then optional static
                    networking is applied after SSH is reachable.
                  </Text>
                </Box>
              ) : null}
            </SetupSection>

            {packageBlocksSetup ? (
              <Box className="setup-package-gate">
                <Flex align="center" justify="between" gap="3" wrap="wrap">
                  <Box minWidth="0">
                    <Text as="div" size="2" weight="medium">
                      Server package update required
                    </Text>
                    <Text as="div" size="2" color="gray">
                      {serverPackageStatus?.message || "Check the Dune server package before continuing."}
                    </Text>
                  </Box>
                  <Button
                    size="2"
                    color={serverPackageCheckStatus === "failed" ? "red" : "amber"}
                    variant="surface"
                    disabled={serverPackageBusy}
                    onClick={onUpdateServerPackage}
                  >
                    {packageActionLabel}
                  </Button>
                </Flex>
              </Box>
            ) : null}

            <Box className={packageBlocksSetup ? "is-flow-disabled" : ""}>
              <Flex direction="column" gap="5">
            <SetupSection
              icon={GlobeIcon}
              title="World"
              className={form.setupTarget === "ubuntu" ? "setup-order-world-ubuntu" : "setup-order-world"}
              disabled={!flowDetectionReady}
            >
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
                  <Link
                    href="#"
                    onClick={(event) => {
                      event.preventDefault();
                      void openExternal("https://account-pts.duneawakening.com/account");
                    }}
                  >
                    account-pts.duneawakening.com/account
                  </Link>
                  .
                </Text>
              </Field>
            </SetupSection>

            <SetupSection
              icon={RocketIcon}
              title="World Layout"
              className={form.setupTarget === "ubuntu" ? "setup-order-layout-ubuntu" : "setup-order-layout"}
              disabled={!flowDetectionReady}
            >
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
                      {deepDesertEnabled ? "Required by Deep Desert" : "Enabled"}
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

            <Box
              className={[
                "memory-calculation",
                form.setupTarget === "ubuntu" ? "setup-order-layout-ubuntu" : "setup-order-layout",
                flowDetectionReady ? "" : "is-flow-disabled",
              ]
                .filter(Boolean)
                .join(" ")}
            >
              <Flex align="start" justify="between" gap="4">
                <Box>
                  <Text as="div" size="2" weight="medium">
                    Required memory
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
              {form.setupTarget !== "ubuntu" ? (
                <>
                  <Separator size="4" my="3" />
                  <FormRow label="VM Memory">
                    <TextField.Root
                      value={String(vmMemoryGb)}
                      onChange={(event) => update("vmMemoryGb", event.target.value)}
                    >
                      <TextField.Slot side="right">GB</TextField.Slot>
                    </TextField.Root>
                    <Text as="div" size="2" color="gray" mt="2">
                      {proxmoxMemoryLimitText(form, calculatedMemory, proxmoxDetection)}
                    </Text>
                  </FormRow>
                  <FormRow label="CPU Cores">
                    <TextField.Root
                      value={String(effectiveProcessorCount(form))}
                      onChange={(event) => update("processorCount", event.target.value)}
                    />
                    <InlineRequirement
                      ok={requirements.processorOk}
                      text={`${requirements.processorRequired}; ${requirements.processorAvailable}`}
                    />
                  </FormRow>
                  {form.setupTarget === "proxmox" ? (
                    <>
                      <Separator size="4" my="3" />
                      <Flex align="center" justify="between" gap="3">
                        <Box>
                          <Text as="div" size="2" weight="medium">
                            Experimental guest swap
                          </Text>
                          <Text as="div" size="2" color="gray">
                            Enable the existing Alpine swap profile after bootstrap.
                          </Text>
                        </Box>
                        <Switch checked={form.enableSwap} onCheckedChange={(value) => update("enableSwap", value)} />
                      </Flex>
                      <Separator size="4" my="3" />
                      <Flex align="center" justify="between" gap="3">
                        <Box>
                          <Text as="div" size="2" weight="medium">
                            QEMU guest agent
                          </Text>
                          <Text as="div" size="2" color="gray">
                            Install and start qemu-guest-agent inside the Proxmox Alpine VM.
                          </Text>
                        </Box>
                        <Switch
                          checked={form.proxmoxInstallQemuGuestAgent}
                          onCheckedChange={(value) => update("proxmoxInstallQemuGuestAgent", value)}
                        />
                      </Flex>
                    </>
                  ) : null}
                </>
              ) : (
                <>
                  <Separator size="4" my="3" />
                  <Flex align="center" justify="between" gap="3">
                    <Box>
                      <Text as="div" size="2" weight="medium">
                        Native Ubuntu swap
                      </Text>
                      <Text as="div" size="2" color="gray">
                        Create a swapfile during setup when the host memory is below the selected layout.
                      </Text>
                    </Box>
                    <Switch checked={form.enableSwap} onCheckedChange={(value) => update("enableSwap", value)} />
                  </Flex>
                  <UbuntuSwapNotice
                    calculatedMemory={calculatedMemory}
                    preflight={remotePreflight}
                    enabled={form.enableSwap}
                  />
                </>
              )}
            </Box>

            {form.setupTarget === "hyperv" ? (
            <SetupSection icon={DesktopIcon} title="Local Hyper-V Host" className="setup-order-vm">
              <Flex direction="column" gap="2">
                <Flex direction="column" gap="2">
                  <Button
                    type="button"
                    variant="surface"
                    className="setup-detect-button"
                    onClick={onLocalDetection}
                    disabled={networkDetection === "detecting"}
                  >
                    {networkDetection === "detecting" ? "Detecting..." : "Detect local resources"}
                  </Button>
                </Flex>
                {networkDetection !== "ready" ? (
                  <Box className="setup-guide">
                    <Text size="2">
                      Run local detection before setup so the app can verify Hyper-V, memory, disk, and network adapter support.
                    </Text>
                  </Box>
                ) : null}
                {networkDetection === "ready" && hostReadiness ? (
                  <LocalResourceSummary readiness={hostReadiness} requirements={requirements} />
                ) : null}
                <Flex
                  direction="column"
                  gap="2"
                  className={hypervDetectionReady ? "setup-dependent-fields" : "setup-dependent-fields is-flow-disabled"}
                >
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
                  <FormRow label="Save Server">
                    <Flex align="center" gap="3" className="checkbox-copy-row">
                      <Checkbox
                        checked={form.saveLocalServer}
                        onCheckedChange={(value) => update("saveLocalServer", value === true)}
                      />
                      <Text size="2" color="gray">
                        Add this Hyper-V server to Servers when setup completes
                      </Text>
                    </Flex>
                  </FormRow>
                </Flex>
              </Flex>
            </SetupSection>
            ) : form.setupTarget === "ubuntu" ? (
            <SetupSection icon={DesktopIcon} title="Remote Ubuntu Host" className="setup-order-remote-host">
              <Flex direction="column" gap="2">
                <UbuntuSetupGuide />
                <FormRow label="Server IP">
                  <TextField.Root
                    placeholder="IPv4 address, for example 203.0.113.10"
                    value={form.remoteHost}
                    onChange={(event) => {
                      update("remoteHost", event.target.value);
                      if (form.playerIpMode === "external" && !form.playerIp.trim()) {
                        update("playerIp", event.target.value);
                      }
                    }}
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
                  <Flex align="center" gap="3" className="checkbox-copy-row">
                    <Checkbox
                      checked={form.saveRemoteServer}
                      onCheckedChange={(value) => update("saveRemoteServer", value === true)}
                    />
                    <Text size="2" color="gray">
                      Add this remote Ubuntu server to Servers when setup starts
                    </Text>
                  </Flex>
                </FormRow>
              </Flex>

              <Button
                type="button"
                variant="surface"
                className="setup-detect-button"
                onClick={onRemotePreflight}
                disabled={
                  remotePreflightStatus === "detecting" ||
                  !form.remoteHost.trim() ||
                  !form.remoteUser.trim() ||
                  !form.remoteKeyPath.trim()
                }
              >
                {remotePreflightStatus === "detecting" ? "Detecting..." : remotePreflight ? "Refresh remote resources" : "Detect remote resources"}
              </Button>
              {remotePreflight ? (
                <RemotePreflightSummary preflight={remotePreflight} />
              ) : null}
            </SetupSection>
            ) : (
            <SetupSection icon={DesktopIcon} title="Proxmox Connection" className="setup-order-remote-host">
              <Flex direction="column" gap="2">
                <Grid columns="2" gap="3">
                  <Field label="Host URL">
                    <TextField.Root
                      placeholder="https://proxmox.example.local:8006"
                      value={form.proxmoxHostUrl}
                      onChange={(event) => update("proxmoxHostUrl", event.target.value)}
                    />
                  </Field>
                  <Field label="API Token ID">
                    <TextField.Root
                      placeholder="root@pam!dune-manager"
                      value={form.proxmoxTokenId}
                      onChange={(event) => update("proxmoxTokenId", event.target.value)}
                    />
                  </Field>
                </Grid>
                <Field label="API Token Secret">
                  <TextField.Root
                    type="password"
                    placeholder="Stored in the OS credential store after detection"
                    value={form.proxmoxTokenSecret}
                    onChange={(event) => update("proxmoxTokenSecret", event.target.value)}
                  />
                </Field>
                <Button
                  type="button"
                  variant="surface"
                  className="setup-detect-button"
                  onClick={onProxmoxDetection}
                  disabled={
                    proxmoxDetectionStatus === "detecting" ||
                    !form.proxmoxHostUrl.trim() ||
                    !form.proxmoxTokenId.trim() ||
                    (!form.proxmoxTokenSecret.trim() && !form.proxmoxAcceptedCertificateSha256.trim())
                  }
                >
                  {proxmoxDetectionStatus === "detecting" ? "Detecting..." : proxmoxDetection ? "Refresh Proxmox resources" : "Detect Proxmox resources"}
                </Button>
                {proxmoxDetection ? (
                  <ProxmoxDetectionSummary detection={proxmoxDetection} />
                ) : null}
                {!proxmoxDetectionReady ? (
                  <Box className="setup-guide">
                    <Text size="2">
                      Run Proxmox resource detection before selecting node, storage, bridge, and VMID.
                    </Text>
                  </Box>
                ) : null}
                <Box className={proxmoxDetectionReady ? "" : "is-flow-disabled"} aria-disabled={!proxmoxDetectionReady}>
                  <Flex direction="column" gap="3">
                    <Grid columns="2" gap="3">
                      <Field label="Node">
                        <Select.Root value={form.proxmoxNode} onValueChange={(value) => update("proxmoxNode", value)}>
                          <Select.Trigger />
                          <Select.Content>
                            {(proxmoxDetection?.nodes ?? []).map((node) => (
                              <Select.Item key={node.node} value={node.node}>{node.node}</Select.Item>
                            ))}
                          </Select.Content>
                        </Select.Root>
                      </Field>
                      <Field label="VMID">
                        <TextField.Root value={form.proxmoxVmid} onChange={(event) => update("proxmoxVmid", event.target.value)} />
                      </Field>
                      <Field label="VM storage">
                        <Select.Root value={form.proxmoxVmStorage} onValueChange={(value) => update("proxmoxVmStorage", value)}>
                          <Select.Trigger />
                          <Select.Content>
                            {(proxmoxDetection?.storages ?? []).filter((storage) => storage.content.includes("images")).map((storage) => (
                              <Select.Item key={storage.storage} value={storage.storage}>{storage.storage}</Select.Item>
                            ))}
                          </Select.Content>
                        </Select.Root>
                      </Field>
                      <Field label="Import storage">
                        <Select.Root value={form.proxmoxImportStorage} onValueChange={(value) => update("proxmoxImportStorage", value)}>
                          <Select.Trigger />
                          <Select.Content>
                            {(proxmoxDetection?.storages ?? []).map((storage) => (
                              <Select.Item key={storage.storage} value={storage.storage}>{storage.storage}</Select.Item>
                            ))}
                          </Select.Content>
                        </Select.Root>
                      </Field>
                    </Grid>
                    <Field label="Bridge">
                      <Select.Root
                        value={form.proxmoxBridge}
                        onValueChange={(value) => {
                          update("proxmoxBridge", value);
                          const bridge = proxmoxDetection?.bridges.find((item) => item.iface === value);
                          if (bridge?.cidr) update("proxmoxBridgeCidr", bridge.cidr);
                        }}
                      >
                        <Select.Trigger />
                        <Select.Content>
                          {(proxmoxDetection?.bridges ?? []).map((bridge) => (
                            <Select.Item key={bridge.iface} value={bridge.iface}>{bridge.iface}</Select.Item>
                          ))}
                        </Select.Content>
                      </Select.Root>
                    </Field>
                    <FormRow label="Save Server">
                      <Flex align="center" gap="3" className="checkbox-copy-row">
                        <Checkbox
                          checked={form.saveRemoteServer}
                          onCheckedChange={(value) => update("saveRemoteServer", value === true)}
                        />
                        <Text size="2" color="gray">
                          Add this Proxmox Alpine server to Servers when setup starts
                        </Text>
                      </Flex>
                    </FormRow>
                  </Flex>
                </Box>
              </Flex>
            </SetupSection>
            )}

            {form.setupTarget === "hyperv" ? (
            <SetupSection icon={MixIcon} title="Network" className="setup-order-network" disabled={!hypervDetectionReady}>
              {networkDetection !== "ready" ? (
                <Box className="setup-guide">
                  <Flex direction="column" gap="2">
                    <SetupWarningPills warnings={["Local detection required"]} />
                    <Text size="2" color="gray">
                      The app needs host adapter, switch, gateway, and subnet details before it can safely create the VM network.
                    </Text>
                  </Flex>
                </Box>
              ) : networkAdapters.length === 0 ? (
                <Box className="setup-guide">
                  <Flex direction="column" gap="2">
                    <SetupWarningPills warnings={["No supported adapter detected"]} />
                    <Text size="2" color="gray">
                      Setup cannot continue until an active physical IPv4 adapter with a gateway is available.
                    </Text>
                  </Flex>
                </Box>
              ) : null}
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
                  disabled={networkDetection !== "ready" || networkAdapters.length === 0}
                  value={form.adapterName}
                  onValueChange={(value) => {
                    const adapter = networkAdapters.find((candidate) => candidate.name === value);
                    if (!adapter) return;
                    update("adapterName", value);
                    update("switchName", adapter.existingExternalSwitch || defaultHyperVSwitchName);
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
            ) : form.setupTarget === "ubuntu" ? (
            <SetupSection icon={MixIcon} title="Network" className="setup-order-network" disabled={!ubuntuDetectionReady}>
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
            ) : (
            <SetupSection icon={MixIcon} title="Network" className="setup-order-network" disabled={!proxmoxDetectionReady}>
              <Field label="Guest network mode">
                <TextField.Root value="DHCP First Static Internal IP" readOnly />
              </Field>
              <Grid columns="3" gap="3">
                <Field label="Static IP">
                  <TextField.Root
                    placeholder="Required"
                    value={form.staticIp}
                    onChange={(event) => update("staticIp", event.target.value)}
                  />
                </Field>
                <Field label="Gateway">
                  <TextField.Root
                    placeholder="Required"
                    value={form.gateway}
                    onChange={(event) => update("gateway", event.target.value)}
                  />
                </Field>
                <Field label="DNS">
                  <TextField.Root
                    placeholder="Required"
                    value={form.dns}
                    onChange={(event) => update("dns", event.target.value)}
                  />
                </Field>
              </Grid>
              <Field label="Player-facing IP">
                <Grid columns="160px 1fr" gap="3">
                  <Select.Root
                    value={form.playerIpMode}
                    onValueChange={(value) => {
                      const mode = value as PlayerIpMode;
                      update("playerIpMode", mode);
                      update("playerIp", mode === "local" ? form.staticIp : form.playerIp);
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
          </Flex>
        </Box>

        <Separator size="4" />

        <Flex align="center" justify="between" gap="3">
          <Box className="setup-readiness">
            {setupRunning ? null : canStart ? (
              <Text size="2" color="gray">
                Ready to create one full setup plan.
              </Text>
            ) : packageBlocksSetup && visibleSetupIssues.length === 0 ? (
              <Text size="2" color="gray">
                Resolve the server package update before setup can continue.
              </Text>
            ) : (
              <ul className="setup-issues">
                {visibleSetupIssues.map((issue) => (
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
    processorCount: effectiveProcessorCount(form),
    enableSwap: false,
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

function proxmoxProfileId(form: Pick<SetupForm, "proxmoxHostUrl" | "proxmoxTokenId">): string {
  return `${form.proxmoxHostUrl.trim().toLowerCase()}|${form.proxmoxTokenId.trim().toLowerCase()}`;
}

function proxmoxConnectionRequest(form: SetupForm) {
  return {
    profileId: proxmoxProfileId(form),
    hostUrl: form.proxmoxHostUrl.trim(),
    tokenId: form.proxmoxTokenId.trim(),
    tokenSecret: form.proxmoxTokenSecret.trim() || undefined,
    acceptedCertificateSha256: form.proxmoxAcceptedCertificateSha256.trim() || undefined,
  };
}

function proxmoxVmActionRequest(provisioner: ProxmoxProvisioner) {
  return {
    profileId: provisioner.profileId,
    hostUrl: provisioner.hostUrl,
    tokenId: provisioner.tokenId,
    acceptedCertificateSha256: provisioner.acceptedCertificateSha256,
    node: provisioner.node,
    vmid: provisioner.vmid,
  };
}

function proxmoxSetupRunRequest(form: SetupForm, memoryGb: number) {
  return {
    ...proxmoxConnectionRequest(form),
    node: form.proxmoxNode.trim(),
    vmStorage: form.proxmoxVmStorage.trim(),
    importStorage: form.proxmoxImportStorage.trim(),
    bridge: form.proxmoxBridge.trim(),
    bridgeCidr: form.proxmoxBridgeCidr.trim() || undefined,
    vmid: parsePositiveInt(form.proxmoxVmid),
    vmName: form.vmName.trim(),
    diskGb: Math.max(1, parsePositiveInt(form.diskGb)),
    memoryGb,
    processorCount: effectiveProcessorCount(form),
    staticIp: form.staticIp.trim(),
    gateway: form.gateway.trim(),
    dns: form.dns.trim(),
    playerIp: form.playerIp.trim(),
    worldName: form.worldName,
    region: form.region,
    selfHostToken: form.tokenSource,
    survivalInstances: Math.max(1, parsePositiveInt(form.survivalInstances)),
    deepDesertPveInstances: parsePositiveInt(form.deepDesertPveInstances),
    deepDesertPvpInstances: parsePositiveInt(form.deepDesertPvpInstances),
    deepDesertWarmServers: parsePositiveInt(form.deepDesertWarmServers),
    enableSwap: form.enableSwap,
    installQemuGuestAgent: form.proxmoxInstallQemuGuestAgent,
  };
}

function remoteServerDraftFromForm(form: SetupForm): RemoteServerRecord {
  const host = form.remoteHost.trim();
  return remoteServerPlaceholder({
    type: "ubuntu",
    host,
    keyPath: form.remoteKeyPath.trim(),
    createdAt: new Date().toISOString(),
  }, form.worldName.trim() || undefined, "Setup running");
}

function proxmoxProvisionerFromForm(form: SetupForm, result?: Partial<ProxmoxAlpineSetupResult>): ProxmoxProvisioner {
  return {
    type: "proxmox",
    profileId: proxmoxProfileId(form),
    hostUrl: form.proxmoxHostUrl.trim(),
    tokenId: form.proxmoxTokenId.trim(),
    acceptedCertificateSha256: form.proxmoxAcceptedCertificateSha256.trim() || undefined,
    node: result?.node || form.proxmoxNode.trim(),
    vmid: result?.vmid || parsePositiveInt(form.proxmoxVmid),
    vmName: result?.vmName || form.vmName.trim(),
  };
}

function proxmoxServerDraftFromForm(form: SetupForm): RemoteServerRecord {
  const host = form.staticIp.trim() || form.proxmoxHostUrl.trim();
  return remoteServerPlaceholder({
    type: "alpine",
    host,
    createdAt: new Date().toISOString(),
    provisioner: proxmoxProvisionerFromForm(form),
  }, form.worldName.trim() || undefined, "Setup running");
}

function proxmoxServerRecordFromSetup(
  form: SetupForm,
  result: ProxmoxAlpineSetupResult,
  existingId?: string,
): RemoteServerRecord {
  const profile = remoteServerPlaceholder({
    type: "alpine",
    host: result.host,
    createdAt: new Date().toISOString(),
    provisioner: proxmoxProvisionerFromForm(form, result),
  });
  return {
    ...profile,
    id: existingId || profile.id,
    name: form.worldName.trim() || result.battlegroupName,
    host: result.host,
    user: result.user || "dune",
    keyPath: "",
    namespace: result.namespace,
    battlegroupName: result.battlegroupName,
    worldUniqueName: result.worldUniqueName,
    phase: "Ready",
  };
}

function remoteServerRecordFromSetup(
  form: SetupForm,
  result: RemoteSetupRunResult,
  existingId?: string,
): RemoteServerRecord {
  const host = form.remoteHost.trim();
  const profile = remoteServerPlaceholder({
    type: "ubuntu",
    host,
    keyPath: form.remoteKeyPath.trim(),
    createdAt: new Date().toISOString(),
  });
  return {
    ...profile,
    id: existingId || profile.id,
    name: form.worldName.trim() || result.battlegroupName,
    namespace: result.namespace,
    battlegroupName: result.battlegroupName,
    worldUniqueName: result.worldUniqueName,
    phase: "Ready",
  };
}

function remoteServerPlaceholder(
  profile: RemoteServerProfile,
  name?: string,
  phase = "Retrieving",
): RemoteServerRecord {
  return {
    type: profile.type,
    id: remoteServerId(profile.type, profile.host, profile.keyPath || ""),
    name: name || profile.host || remoteServerKindLabel(profile.type),
    host: profile.host,
    user: remoteServerDefaultUser(profile.type),
    keyPath: profile.keyPath || "",
    namespace: "",
    battlegroupName: "",
    worldUniqueName: "",
    phase,
    createdAt: profile.createdAt,
    provisioner: profile.provisioner,
  };
}

function remoteServerFromDetected(existing: RemoteServerRecord, detected: RemoteServerRecord): RemoteServerRecord {
  return {
    ...detected,
    type: existing.type,
    id: existing.id,
    host: existing.host,
    keyPath: existing.keyPath,
    user: existing.user || remoteServerDefaultUser(existing.type),
    createdAt: existing.createdAt,
    provisioner: existing.provisioner,
  };
}

function remoteServerId(type: RemoteServerKind, host: string, keyPath = ""): string {
  const normalizedHost = host.trim().toLowerCase();
  if (type === "alpine") return `alpine:${normalizedHost}`;
  return `ubuntu:${normalizedHost}:${keyPath.trim().toLowerCase()}`;
}

function remoteServerDefaultUser(type: RemoteServerKind): string {
  return type === "alpine" ? "dune" : "root";
}

function remoteServerKindLabel(type: RemoteServerKind): string {
  return type === "alpine" ? "Alpine VM" : "Ubuntu";
}

function remoteServerActionRequest(server: RemoteServerRecord) {
  return {
    serverType: server.type,
    host: server.host,
    user: server.user || remoteServerDefaultUser(server.type),
    keyPath: server.type === "ubuntu" ? server.keyPath : undefined,
    namespace: server.namespace,
    battlegroupName: server.battlegroupName,
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

function readRemoteServers(): RemoteServerRecord[] {
  const text = window.localStorage.getItem(remoteServersStorageKey);
  if (!text) return [];
  try {
    const value = JSON.parse(text);
    if (!Array.isArray(value)) return [];
    return value
      .map(remoteServerProfileFromStored)
      .filter((profile): profile is RemoteServerProfile => !!profile)
      .map((profile) => remoteServerPlaceholder(profile));
  } catch {
    window.localStorage.removeItem(remoteServersStorageKey);
    return [];
  }
}

function persistRemoteServers(servers: RemoteServerRecord[]): RemoteServerRecord[] {
  const profiles = uniqueBy(
    servers
      .filter((server) => server.host.trim() && (server.type === "alpine" || server.keyPath.trim()))
      .map((server): RemoteServerProfile => ({
        type: server.type,
        host: server.host,
        keyPath: server.type === "ubuntu" ? server.keyPath : undefined,
        createdAt: server.createdAt || new Date().toISOString(),
        provisioner: server.provisioner,
      })),
    (profile) => remoteServerId(profile.type, profile.host, profile.keyPath || ""),
  );
  window.localStorage.setItem(remoteServersStorageKey, JSON.stringify(profiles));
  return servers;
}

function persistProxmoxProfile(profile: {
  hostUrl: string;
  tokenId: string;
  acceptedCertificateSha256?: string;
}) {
  window.localStorage.setItem(
    proxmoxProfileStorageKey,
    JSON.stringify({
      proxmoxHostUrl: profile.hostUrl,
      proxmoxTokenId: profile.tokenId,
      proxmoxAcceptedCertificateSha256: profile.acceptedCertificateSha256 || "",
    }),
  );
}

function localServerKey(server: DuneVmCandidate): string {
  return `hyperv:${server.vm.name}`;
}

function componentLogStateKey(serverKey: string, component: RemoteServerComponent): string {
  return `${serverKey}:${component.logKey}`;
}

function serverTunnelKey(serverKey: string, service: TunnelService): string {
  return `${serverKey}:tunnel:${service}`;
}

function tunnelServiceLabel(service: TunnelService): string {
  return service === "director" ? "Director UI" : "File Browser";
}

function isCriticalRestartComponent(component: RemoteServerComponent): boolean {
  return ["database", "message-queue", "server-group"].includes(component.logKey);
}

async function copyTextToClipboard(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "true");
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  document.execCommand("copy");
  document.body.removeChild(textarea);
}

function upsertLocalServer(servers: DuneVmCandidate[], record: DuneVmCandidate): DuneVmCandidate[] {
  const index = servers.findIndex((server) => server.vm.name.toLowerCase() === record.vm.name.toLowerCase());
  if (index === -1) {
    return [...servers, record];
  }
  const next = [...servers];
  next[index] = mergeLocalServerAddress(next[index], record);
  return next;
}

function primaryLocalServerIp(server: DuneVmCandidate): string {
  return server.vm.ipv4Addresses[0] ?? "";
}

function mergeLocalServerAddress(existing: DuneVmCandidate, record: DuneVmCandidate): DuneVmCandidate {
  if (record.vm.ipv4Addresses.length > 0 || existing.vm.ipv4Addresses.length === 0) {
    return record;
  }
  return {
    ...record,
    vm: {
      ...record.vm,
      ipv4Addresses: existing.vm.ipv4Addresses,
    },
  };
}

function readLocalServers(): DuneVmCandidate[] {
  const text = window.localStorage.getItem(localServersStorageKey);
  if (!text) return [];
  try {
    const value = JSON.parse(text);
    if (!Array.isArray(value)) return [];
    return value
      .map(localServerProfileFromStored)
      .filter((profile): profile is LocalServerProfile => !!profile)
      .map((profile) => localServerPlaceholder(profile.vmName, profile.staticIp));
  } catch {
    window.localStorage.removeItem(localServersStorageKey);
    return [];
  }
}

function persistLocalServers(servers: DuneVmCandidate[]): DuneVmCandidate[] {
  const profiles = uniqueBy(
    servers
      .filter((server) => server.vm.name.trim())
      .map((server): LocalServerProfile => ({
        type: "hyperv",
        vmName: server.vm.name,
        staticIp: primaryLocalServerIp(server),
        createdAt: new Date().toISOString(),
      })),
    (profile) => profile.vmName.trim().toLowerCase(),
  );
  window.localStorage.setItem(localServersStorageKey, JSON.stringify(profiles));
  return servers;
}

function remoteServerProfileFromStored(value: unknown): RemoteServerProfile | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Partial<RemoteServerProfile & RemoteServerRecord>;
  if (typeof record.host !== "string") return null;
  const type = record.type === "alpine" ? "alpine" : "ubuntu";
  if (type === "ubuntu" && typeof record.keyPath !== "string") return null;
  return {
    type,
    host: record.host,
    keyPath: typeof record.keyPath === "string" ? record.keyPath : "",
    createdAt: typeof record.createdAt === "string" ? record.createdAt : new Date().toISOString(),
    provisioner: isProxmoxProvisioner(record.provisioner) ? record.provisioner : undefined,
  };
}

function isProxmoxProvisioner(value: unknown): value is ProxmoxProvisioner {
  if (!value || typeof value !== "object") return false;
  const provisioner = value as Partial<ProxmoxProvisioner>;
  return (
    provisioner.type === "proxmox" &&
    typeof provisioner.hostUrl === "string" &&
    typeof provisioner.tokenId === "string" &&
    typeof provisioner.node === "string" &&
    typeof provisioner.vmid === "number"
  );
}

function localServerProfileFromStored(value: unknown): LocalServerProfile | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Partial<LocalServerProfile & DuneVmCandidate>;
  const vmName =
    typeof record.vmName === "string"
      ? record.vmName
      : typeof record.vm?.name === "string"
        ? record.vm.name
        : "";
  if (!vmName.trim()) return null;
  return {
    type: "hyperv",
    vmName,
    staticIp: typeof record.staticIp === "string" ? record.staticIp : "",
    createdAt: typeof record.createdAt === "string" ? record.createdAt : new Date().toISOString(),
  };
}

function localServerPlaceholder(vmName: string, staticIp = ""): DuneVmCandidate {
  return {
    confidence: "low",
    reasons: ["saved profile"],
    vm: {
      name: vmName,
      state: "other",
      rawState: "Retrieving",
      configurationLocation: "",
      path: "",
      memoryAssignedBytes: 0,
      processorCount: 0,
      uptimeSeconds: 0,
      ipv4Addresses: staticIp.trim() ? [staticIp.trim()] : [],
      hardDiskPaths: [],
      diskSizeBytes: 0,
      diskFileSizeBytes: 0,
      switchNames: [],
    },
  };
}

function uniqueBy<T>(values: T[], keyOf: (value: T) => string): T[] {
  const seen = new Set<string>();
  const unique: T[] = [];
  for (const value of values) {
    const key = keyOf(value);
    if (seen.has(key)) continue;
    seen.add(key);
    unique.push(value);
  }
  return unique;
}

function isRemoteServerRecord(value: unknown): value is RemoteServerRecord {
  if (!value || typeof value !== "object") return false;
  const record = value as Partial<RemoteServerRecord>;
  return typeof record.id === "string" && typeof record.host === "string" && typeof record.name === "string";
}

function isDuneVmCandidate(value: unknown): value is DuneVmCandidate {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<DuneVmCandidate>;
  const vm = candidate.vm as Partial<VmInventoryRecord> | undefined;
  return (
    !!vm &&
    typeof vm.name === "string" &&
    typeof vm.rawState === "string" &&
    typeof vm.state === "string" &&
    Array.isArray(vm.ipv4Addresses) &&
    Array.isArray(vm.hardDiskPaths) &&
    Array.isArray(vm.switchNames)
  );
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

function omitPrefix<T>(record: Record<string, T>, prefix: string): Record<string, T> {
  return Object.fromEntries(Object.entries(record).filter(([key]) => !key.startsWith(prefix)));
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
  enableSwap: boolean,
): SetupRequirements {
  const requiredMemoryBytes = calculatedMemory.gb * 1024 * 1024 * 1024;
  const requiredProcessors = Math.max(0, parsePositiveInt(processorCount));
  const requiredDiskGb = Math.max(0, parsePositiveInt(diskGb));
  const requiredDiskBytes = requiredDiskGb * 1024 * 1024 * 1024;
  const memoryAvailable = preflight?.availableMemoryBytes ?? 0;
  const existingSwapBytes = preflight?.swapTotalBytes ?? 0;
  const plannedSwapBytes = preflight && enableSwap ? recommendedUbuntuSwapGb(calculatedMemory, preflight) * 1024 * 1024 * 1024 : 0;
  const usableSwapBytes = Math.max(existingSwapBytes, plannedSwapBytes);
  const processorsAvailable = preflight?.logicalProcessorCount ?? 0;
  const diskAvailable = preflight?.rootDiskAvailableBytes ?? 0;
  const memoryOk = !!preflight && (memoryAvailable >= requiredMemoryBytes || memoryAvailable + usableSwapBytes >= requiredMemoryBytes);
  const memoryAvailableLabel =
    preflight && usableSwapBytes > 0
      ? `${formatGiB(memoryAvailable)} RAM available plus ${formatGiB(usableSwapBytes)} ${plannedSwapBytes > existingSwapBytes ? "planned" : "existing"} swap`
      : preflight
        ? `${formatGiB(memoryAvailable)} available`
        : "Run remote detection";

  return {
    canContinue:
      !!preflight &&
      memoryOk &&
      requiredProcessors > 0 &&
      requiredProcessors <= processorsAvailable &&
      diskAvailable >= requiredDiskBytes,
    memoryOk,
    processorOk: !!preflight && requiredProcessors > 0 && requiredProcessors <= processorsAvailable,
    diskOk: !!preflight && diskAvailable >= requiredDiskBytes,
    memoryRequired: `${calculatedMemory.gb} GB required`,
    memoryAvailable: memoryAvailableLabel,
    processorRequired: `${requiredProcessors || "A positive number of"} logical cores recommended`,
    processorAvailable: preflight ? `${processorsAvailable} logical available` : "Run remote detection",
    diskRequired: `${requiredDiskGb} GB free space required`,
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
  if (form.remoteHost.includes(":")) issues.push("Use an IPv4 address for the remote Ubuntu server.");
  if (!form.remoteUser.trim()) issues.push("SSH user is required.");
  if (!form.remoteKeyPath.trim()) issues.push("SSH private key file is required.");
  if (!preflight) issues.push("Run remote resource detection before setup.");
  if (form.enableSwap && !preflight) issues.push("Run remote detection before enabling Ubuntu swap.");
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
    issues.push("Warm Deep Desert Instances are not supported yet; set them to 0 for this build.");
  }
  if (deepDesertInstanceCount(form) > 1) {
    issues.push("Only one Deep Desert instance is supported in this build.");
  }
  if (!hasServiceToken) issues.push("Self-Host Service Token is required.");
  return issues;
}

function proxmoxSetupRequirementStatus(
  calculatedMemory: CalculatedMemory,
  effectiveMemoryGb: number,
  diskGb: string,
  processorCount: string,
  nodeName: string,
  storageName: string,
  detection: ProxmoxDetection | null,
): SetupRequirements {
  const requiredMemoryBytes = effectiveMemoryGb * 1024 * 1024 * 1024;
  const requiredProcessors = Math.max(0, parsePositiveInt(processorCount));
  const requiredDiskGb = Math.max(0, parsePositiveInt(diskGb));
  const requiredDiskBytes = requiredDiskGb * 1024 * 1024 * 1024;
  const node = detection?.nodes.find((item) => item.node === nodeName.trim()) ?? detection?.nodes[0] ?? null;
  const storage =
    detection?.storages.find((item) => item.storage === storageName.trim()) ??
    detection?.storages.find((item) => item.content.includes("images")) ??
    detection?.storages[0] ??
    null;
  const availableMemoryBytes = node ? Math.max(0, node.maxmem - node.mem) : 0;
  const processorsAvailable = node?.maxcpu ?? 0;
  const storageAvailableBytes = storage?.avail ?? 0;
  const memoryOk = !!node && effectiveMemoryGb > 0 && availableMemoryBytes >= requiredMemoryBytes;
  const processorOk = !!node && requiredProcessors > 0 && requiredProcessors <= processorsAvailable;
  const diskOk = !!storage && requiredDiskGb > 0 && storageAvailableBytes >= requiredDiskBytes;
  const memoryRequired =
    effectiveMemoryGb < calculatedMemory.gb
      ? `${effectiveMemoryGb} GB required with swap profile (${calculatedMemory.gb} GB layout recommendation)`
      : `${effectiveMemoryGb} GB required`;
  return {
    canContinue: memoryOk && processorOk && diskOk,
    memoryOk,
    processorOk,
    diskOk,
    memoryRequired,
    memoryAvailable: node
      ? `${formatGiBFloor1(availableMemoryBytes)} available on ${node.node} (${formatGiBFloor1(node.maxmem)} total)`
      : "Run Proxmox detection",
    processorRequired: `${requiredProcessors || "A positive number of"} cores configured`,
    processorAvailable: node ? `${processorsAvailable} logical available on ${node.node}` : "Run Proxmox detection",
    diskRequired: `${requiredDiskGb} GB virtual disk`,
    diskAvailable: storage
      ? `${storage.storage} has ${formatGiBFloor1(storageAvailableBytes)} free`
      : "Run Proxmox detection and choose VM storage",
  };
}

function proxmoxSetupBlockingIssues(
  requirements: SetupRequirements,
  hasServiceToken: boolean,
  form: SetupForm,
  detection: ProxmoxDetection | null,
): string[] {
  const issues: string[] = [];
  if (!form.proxmoxHostUrl.trim()) issues.push("Proxmox host URL is required.");
  if (!form.proxmoxTokenId.trim()) issues.push("Proxmox API token id is required.");
  if (!form.proxmoxTokenSecret.trim() && !form.proxmoxAcceptedCertificateSha256.trim()) {
    issues.push("Proxmox API token secret is required the first time this profile is used.");
  }
  if (!detection) issues.push("Run Proxmox resource detection before setup.");
  if (!form.proxmoxNode.trim()) issues.push("Proxmox node is required.");
  if (!form.proxmoxVmStorage.trim()) issues.push("VM storage is required.");
  if (!form.proxmoxImportStorage.trim()) issues.push("Import storage is required.");
  if (!form.proxmoxBridge.trim()) issues.push("Proxmox bridge is required.");
  if (parsePositiveInt(form.proxmoxVmid) <= 0) issues.push("Proxmox VMID is required.");
  if (!form.vmName.trim()) issues.push("VM name is required.");
  if (!form.staticIp.trim()) issues.push("Static guest IP is required for Proxmox setup.");
  if (!form.gateway.trim()) issues.push("Static guest gateway is required for Proxmox setup.");
  if (!form.dns.trim()) issues.push("Static guest DNS is required for Proxmox setup.");
  if (!requirements.memoryOk) issues.push(`Memory: ${requirements.memoryRequired}; ${requirements.memoryAvailable}.`);
  if (!requirements.processorOk) issues.push(`CPU Cores: ${requirements.processorRequired}.`);
  if (!requirements.diskOk) issues.push(`Disk: ${requirements.diskRequired}.`);
  if (!form.playerIp.trim()) issues.push("Player-facing IP is required.");
  if (parsePositiveInt(form.deepDesertWarmServers) > 0) {
    issues.push("Warm Deep Desert Instances are not supported yet; set them to 0 for this build.");
  }
  if (deepDesertInstanceCount(form) > 1) {
    issues.push("Only one Deep Desert instance is supported in this build.");
  }
  if (!hasServiceToken) issues.push("Self-Host Service Token is required.");
  return issues;
}

function setupIssueSummary(
  setupTarget: SetupTarget,
  issues: string[],
  proxmoxDetection: ProxmoxDetection | null,
): string[] {
  if (setupTarget !== "proxmox") return issues.slice(0, 6);
  const summarized: string[] = [];
  if (issues.some((issue) => issue.startsWith("Proxmox host URL") || issue.startsWith("Proxmox API token"))) {
    summarized.push("Complete the Proxmox connection fields.");
  }
  if (issues.includes("Run Proxmox resource detection before setup.")) {
    summarized.push("Run Proxmox resource detection before choosing VM resources.");
  }
  if (proxmoxDetection) {
    summarized.push(
      ...issues.filter(
        (issue) =>
          !issue.startsWith("Proxmox host URL") &&
          !issue.startsWith("Proxmox API token") &&
          issue !== "Run Proxmox resource detection before setup.",
      ),
    );
  }
  if (!proxmoxDetection) {
    if (issues.includes("Self-Host Service Token is required.")) {
      summarized.push("Self-Host Service Token is required.");
    }
    return summarized;
  }
  return summarized.slice(0, 6);
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

function effectiveVmMemoryGb(form: SetupForm, calculatedMemory: CalculatedMemory): number {
  return Math.max(minimumVmMemoryGb(form, calculatedMemory), parsePositiveInt(form.vmMemoryGb));
}

function effectiveProxmoxVmMemoryGb(
  form: SetupForm,
  calculatedMemory: CalculatedMemory,
  detection: ProxmoxDetection | null,
): number {
  const requestedMemoryGb = effectiveVmMemoryGb(form, calculatedMemory);
  const availableMemoryGb = proxmoxAvailableMemoryWholeGb(form, detection);
  if (availableMemoryGb === null) return requestedMemoryGb;
  return Math.min(requestedMemoryGb, availableMemoryGb);
}

function minimumVmMemoryGb(form: SetupForm, calculatedMemory: CalculatedMemory): number {
  if (form.setupTarget !== "proxmox" || !form.enableSwap) {
    return calculatedMemory.gb;
  }
  const survivalInstances = Math.max(1, parsePositiveInt(form.survivalInstances));
  const deepDesertInstances = deepDesertInstanceCount(form);
  const socialGb = form.includeSocial || deepDesertInstances > 0 ? 5 : 0;
  const lowMemoryGb = survivalInstances * 15 + deepDesertInstances * 10 + socialGb;
  return Math.min(calculatedMemory.gb, Math.max(1, lowMemoryGb));
}

function proxmoxAvailableMemoryWholeGb(form: SetupForm, detection: ProxmoxDetection | null): number | null {
  const node = detection?.nodes.find((item) => item.node === form.proxmoxNode.trim()) ?? detection?.nodes[0] ?? null;
  if (!node) return null;
  const availableBytes = Math.max(0, node.maxmem - node.mem);
  return Math.floor(availableBytes / 1024 / 1024 / 1024);
}

function proxmoxMemoryLimitText(
  form: SetupForm,
  calculatedMemory: CalculatedMemory,
  detection: ProxmoxDetection | null,
): string {
  const minimumGb = minimumVmMemoryGb(form, calculatedMemory);
  const requestedGb = effectiveVmMemoryGb(form, calculatedMemory);
  if (form.setupTarget !== "proxmox") {
    return `Minimum for this profile is ${minimumGb} GB. You can increase it, but not decrease below the selected profile.`;
  }
  const availableGb = proxmoxAvailableMemoryWholeGb(form, detection);
  if (availableGb === null) {
    return `Minimum for this profile is ${minimumGb} GB. Run Proxmox detection to cap this value to available node memory.`;
  }
  if (availableGb < requestedGb) {
    return `Capped at ${availableGb} GB, the selected node's available whole-GB memory. Profile target is ${minimumGb} GB.`;
  }
  return `Minimum for this profile is ${minimumGb} GB. You can increase it up to ${availableGb} GB available on the selected node.`;
}

function effectiveProcessorCount(form: SetupForm): number {
  return Math.max(4, parsePositiveInt(form.processorCount));
}

function recommendedUbuntuSwapGb(calculatedMemory: CalculatedMemory, preflight: UbuntuSshPreflight): number {
  const gib = 1024 * 1024 * 1024;
  const requiredBytes = calculatedMemory.gb * gib;
  const shortfallBytes = Math.max(0, requiredBytes - preflight.availableMemoryBytes);
  const shortfallGb = Math.ceil(shortfallBytes / gib);
  return Math.min(64, Math.max(2, shortfallGb));
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
  if (status === "idle") {
    return [log.info("env", "Local environment detection has not run yet.")];
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
      ? log.info("env", "Detected external IP.")
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
        `Detected ${adapter.name} with IPv4 gateway and VM IP suggestion.`,
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
  vmMemoryGb: string,
  diskGb: string,
  processorCount: string,
  vmDestination: string,
  readiness: HostReadiness | null,
  drives: DriveCandidate[],
): SetupRequirements {
  const effectiveMemoryGb = Math.max(calculatedMemory.gb, parsePositiveInt(vmMemoryGb));
  const requiredMemoryBytes = effectiveMemoryGb * 1024 * 1024 * 1024;
  const requiredProcessors = Math.max(4, parsePositiveInt(processorCount));
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
    memoryRequired: `${effectiveMemoryGb} GB required`,
    memoryAvailable: readiness ? `${formatGiB(memoryAvailable)} available` : "Run local detection",
    processorRequired: `${requiredProcessors || "A positive number of"} cores requested`,
    processorAvailable: readiness
      ? processorsAvailable
        ? `${processorsAvailable} logical available`
        : "Host CPU count unavailable"
      : "Run local detection",
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
    issues.push("Warm Deep Desert Instances are not supported yet; set them to 0 for this build.");
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

function LocalResourceSummary({
  readiness,
  requirements,
}: {
  readiness: HostReadiness;
  requirements: SetupRequirements;
}) {
  const rows: Array<[string, string, "green" | "amber" | "red"]> = [
    [
      "Hyper-V",
      readiness.hypervAvailable && readiness.vmmsRunning ? "Available and running" : "Needs attention",
      readiness.hypervAvailable && readiness.vmmsRunning ? "green" : "red",
    ],
    [
      "Memory",
      `${requirements.memoryRequired}; ${requirements.memoryAvailable}`,
      requirements.memoryOk ? "green" : "amber",
    ],
    [
      "CPU",
      `${requirements.processorRequired}; ${requirements.processorAvailable}`,
      requirements.processorOk ? "green" : "amber",
    ],
  ];
  return (
    <Box className="info-card">
      {rows.map(([label, value, tone]) => (
        <InfoRow key={label} label={label} value={value} tone={tone} />
      ))}
    </Box>
  );
}

function UbuntuSwapNotice({
  calculatedMemory,
  preflight,
  enabled,
}: {
  calculatedMemory: CalculatedMemory;
  preflight: UbuntuSshPreflight | null;
  enabled: boolean;
}) {
  if (!preflight) {
    return (
      <Text as="div" size="2" color="gray" mt="2">
        Run remote detection to calculate a swap recommendation.
      </Text>
    );
  }
  const requiredBytes = calculatedMemory.gb * 1024 * 1024 * 1024;
  const recommendedSwapGb = recommendedUbuntuSwapGb(calculatedMemory, preflight);
  const totalMemory = preflight.totalMemoryBytes;
  const memoryShortfallIsLarge = totalMemory > 0 && totalMemory < requiredBytes * 0.8;
  const hasExistingSwap = preflight.swapTotalBytes > 0;
  if (!enabled && !hasExistingSwap && !memoryShortfallIsLarge) {
    return (
      <Text as="div" size="2" color="gray" mt="2">
        No swap will be created.
      </Text>
    );
  }
  return (
    <Box className={memoryShortfallIsLarge ? "destructive-warning" : "setup-guide"} mt="2">
      <Flex direction="column" gap="1">
        {hasExistingSwap ? (
          <Text size="2">
            Existing swap detected: {formatGiB(preflight.swapTotalBytes)}. Swap can reduce performance when it is used heavily.
          </Text>
        ) : null}
        {enabled ? (
          <Text size="2">
            Setup will create a native Ubuntu swapfile of about {recommendedSwapGb} GB.
          </Text>
        ) : null}
        {memoryShortfallIsLarge ? (
          <Text size="2" weight="medium">
            Physical memory is more than 20% below the selected layout recommendation. The server may run, but heavy swap use can cause stalls and disconnects.
          </Text>
        ) : null}
      </Flex>
    </Box>
  );
}

function RemotePreflightSummary({ preflight }: { preflight: UbuntuSshPreflight }) {
  const rows: Array<[string, string, "green" | "amber" | "red"]> = [
    ["Host", `${preflight.hostname} (${preflight.osPrettyName})`, "green"],
    ["Public IP", preflight.publicIp || "Not detected", "green"],
    ["Private IPs", preflight.ipv4Addresses.length ? preflight.ipv4Addresses.join(", ") : "None detected", "green"],
    ["Memory", `${formatGiB(preflight.availableMemoryBytes)} available of ${formatGiB(preflight.totalMemoryBytes)}`, "green"],
    ["Swap", preflight.swapTotalBytes > 0 ? `${formatGiB(preflight.swapTotalBytes)} configured` : "None configured", preflight.swapTotalBytes > 0 ? "amber" : "green"],
    ["Disk", `${formatGiB(preflight.rootDiskAvailableBytes)} free of ${formatGiB(preflight.rootDiskTotalBytes)} on /`, "green"],
    ["CPU", `${preflight.logicalProcessorCount} logical processors`, "green"],
    ["Access", preflight.uid === 0 ? "root" : preflight.passwordlessSudo ? "passwordless sudo" : "limited", preflight.uid === 0 || preflight.passwordlessSudo ? "green" : "red"],
    ["Existing tools", `SteamCMD ${preflight.steamcmdInstalled ? "present" : "missing"}, k3s ${preflight.k3sInstalled ? "present" : "missing"}`, preflight.k3sInstalled ? "amber" : "green"],
  ];
  return (
    <Box className="info-card">
      {rows.map(([label, value, tone]) => (
        <InfoRow key={label} label={label} value={value} tone={tone} />
      ))}
    </Box>
  );
}

function ProxmoxDetectionSummary({ detection }: { detection: ProxmoxDetection }) {
  const rows: Array<[string, string, "green" | "amber" | "red"]> = [
    ["Version", detection.version.version || "Unknown", "green"],
    ["Certificate", detection.certificateTrusted ? "Trusted fingerprint" : "Fingerprint captured", detection.certificateTrusted ? "green" : "amber"],
    ["Nodes", detection.nodes.map((node) => `${node.node} (${node.status || "unknown"})`).join(", ") || "None", detection.nodes.length ? "green" : "red"],
    ["Storage", detection.storages.map((storage) => `${storage.storage}: ${storage.content || "unknown"}`).join(", ") || "None", detection.storages.length ? "green" : "red"],
    ["Bridges", detection.bridges.map((bridge) => bridge.cidr ? `${bridge.iface} ${bridge.cidr}` : bridge.iface).join(", ") || "None", detection.bridges.length ? "green" : "red"],
    ["Next VMID", detection.nextVmid ? String(detection.nextVmid) : "Not detected", detection.nextVmid ? "green" : "amber"],
  ];
  return (
    <Box className="info-card">
      {rows.map(([label, value, tone]) => (
        <InfoRow key={label} label={label} value={value} tone={tone} />
      ))}
    </Box>
  );
}

function SetupWarningPills({ warnings }: { warnings: string[] }) {
  return (
    <Flex gap="2" wrap="wrap" className="setup-warning-pills">
      {warnings.map((warning) => (
        <Badge key={warning} color="amber" variant="soft">
          {warning}
        </Badge>
      ))}
    </Flex>
  );
}

function UbuntuSetupGuide() {
  const rows = [
    "Use an Ubuntu 24+ VPS or dedicated server with enough RAM and CPU for the selected layout.",
    "Add your SSH public key during host creation. Use IPv4 only for wider compatibility.",
    "Restrict SSH port 22 to your IP in the hosting firewall when possible.",
    "Allow UDP 7777-7810 and TCP 31982 from any IP for players.",
  ];
  return (
    <Box className="setup-guide">
      <Flex direction="column" gap="2">
        <ul className="setup-guide-list">
          {rows.map((row) => (
            <li key={row}>{row}</li>
          ))}
        </ul>
      </Flex>
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
    message: sanitizeLogMessage(message),
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

function limitLogRows(rows: LogRow[]): LogRow[] {
  if (rows.length <= maxStoredLogRows) return rows;
  return rows.slice(-maxStoredLogRows);
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

function serverPackageLabel(status: ServerPackageCheckStatus, packageStatus: ServerPackageStatus | null): string {
  if (status === "checking") return "Package checking";
  if (status === "updating") return "Package updating";
  if (status === "failed") return "Package check failed";
  if (status === "missing") return "Package missing";
  if (status === "available") {
    return packageStatus?.latestBuildId ? `Server ${packageStatus.latestBuildId} available` : "Server update available";
  }
  if (status === "current") {
    return packageStatus?.installedBuildId ? `Server ${packageStatus.installedBuildId}` : "Server package current";
  }
  return "Server package";
}

function serverPackageTone(status: ServerPackageCheckStatus): "green" | "amber" | "red" {
  if (status === "failed" || status === "missing") return "red";
  if (status === "current") return "green";
  return "amber";
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return "Operation failed.";
}

function sanitizeLogMessage(message: string): string {
  return message.replace(
    /\b(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(?::\d{1,5})?\b/g,
    "IP address",
  );
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
  if (status === "idle") return "Run local detection";
  if (status === "detecting") return "Detecting adapters...";
  if (status === "failed") return "Detection failed";
  return "Choose adapter";
}

function formatGiB(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "unknown";
  return `${Math.round(bytes / 1024 / 1024 / 1024)} GB`;
}

function formatGiBFloor1(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "unknown";
  const gib = bytes / 1024 / 1024 / 1024;
  return `${(Math.floor(gib * 10) / 10).toFixed(1)} GB`;
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
  disabled,
  icon: Icon,
  title,
  children,
}: {
  className?: string;
  disabled?: boolean;
  icon: ComponentType<{ width?: number | string; height?: number | string }>;
  title: string;
  children: ReactNode;
}) {
  return (
    <Box className={["setup-section", className, disabled ? "is-flow-disabled" : ""].filter(Boolean).join(" ")} aria-disabled={disabled}>
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
  collapsed,
  onLevelChange,
  onClear,
  onToggleCollapsed,
}: {
  rows: LogRow[];
  level: LogLevelFilter;
  collapsed: boolean;
  onLevelChange: (level: LogLevelFilter) => void;
  onClear: () => void;
  onToggleCollapsed: () => void;
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
    <Card size="3" variant="surface" className={`pane log-pane${collapsed ? " is-collapsed" : ""}`}>
      <Flex direction="column" height="100%" minHeight="0">
        <Flex align="center" justify="between" gap="3" mb={collapsed ? "0" : "3"}>
          <Box minWidth="0">
            <Text as="div" size="2" weight="medium">
              Logs
            </Text>
            <Text as="div" size="1" color="gray">
              {rows.length} entries
            </Text>
          </Box>
          <Flex align="center" gap="2">
            {collapsed ? null : (
              <>
                <Select.Root value={level} onValueChange={(value) => onLevelChange(value as LogLevelFilter)}>
                  <Select.Trigger aria-label="Minimum log level" />
                  <Select.Content>
                    <Select.Item value="debug">Debug</Select.Item>
                    <Select.Item value="info">Info</Select.Item>
                    <Select.Item value="warn">Warn</Select.Item>
                    <Select.Item value="error">Error</Select.Item>
                  </Select.Content>
                </Select.Root>
                <Button type="button" size="1" variant="surface" disabled={rows.length === 0} onClick={onClear}>
                  Clear
                </Button>
              </>
            )}
            <Button
              type="button"
              size="1"
              variant="surface"
              aria-label={collapsed ? "Expand logs" : "Collapse logs"}
              onClick={onToggleCollapsed}
            >
              {collapsed ? <ChevronUpIcon /> : <ChevronDownIcon />}
            </Button>
          </Flex>
        </Flex>
        {collapsed ? null : (
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
                  columns="96px 44px 1fr"
                  gap="2"
                  align="center"
                  className={`log-line log-${row.level}`}
                >
                  <Text color="gray" className="mono log-meta log-text">
                    {row.timestamp}
                  </Text>
                  <Text className="mono log-meta log-level log-text">
                    {row.level}
                  </Text>
                  <Text className="mono log-text">
                    {row.message}
                  </Text>
                </Grid>
              ))}
            </Flex>
          </Box>
        )}
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
          <AlertDialog.Cancel disabled={rollbackRunning}>
            <Button variant="soft" color="gray" disabled={rollbackRunning}>
              Keep artifacts
            </Button>
          </AlertDialog.Cancel>
          <AlertDialog.Action disabled={rollbackRunning}>
            <Button color="red" disabled={rollbackRunning} onClick={onRollback}>
              {rollbackRunning ? "Rolling back..." : "Rollback"}
            </Button>
          </AlertDialog.Action>
        </Flex>
      </AlertDialog.Content>
    </AlertDialog.Root>
  );
}

function ServerUpdateConfirmDialog({
  pending,
  onOpenChange,
  onConfirm,
}: {
  pending: PendingServerUpdate | null;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}) {
  const serverName =
    pending?.type === "remote"
      ? pending.server.name
      : pending?.type === "local"
        ? pending.server.vm.name
        : "";
  return (
    <AlertDialog.Root open={!!pending} onOpenChange={onOpenChange}>
      <AlertDialog.Content maxWidth="520px">
        <AlertDialog.Title>Update server?</AlertDialog.Title>
        <AlertDialog.Description size="2" color="gray">
          This operation will stop the BattleGroup, verify it is fully stopped, update the server,
          and start the BattleGroup again. Are you sure?
        </AlertDialog.Description>
        {serverName ? (
          <Text as="p" size="2" color="gray" mt="3">
            Target: <Text weight="medium">{serverName}</Text>
          </Text>
        ) : null}
        <Flex gap="3" justify="end" mt="5">
          <AlertDialog.Cancel>
            <Button variant="soft" color="gray">
              Cancel
            </Button>
          </AlertDialog.Cancel>
          <AlertDialog.Action>
            <Button color="amber" onClick={onConfirm}>
              Update Server
            </Button>
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
  const canAttach =
    form.host.trim().length > 0 &&
    (form.type === "alpine" || form.keyPath.trim().length > 0) &&
    !running;
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Content maxWidth="520px">
        <Dialog.Title>Add Remote Server</Dialog.Title>
        <Dialog.Description size="2" color="gray">
          Connect over SSH and detect existing Dune battlegroups. This does not provision or modify the server.
        </Dialog.Description>
        <Flex direction="column" gap="3" mt="4">
          <Field label="Server type">
            <Select.Root
              value={form.type}
              onValueChange={(value) => onChange({ ...form, type: value as RemoteServerKind })}
              disabled={running}
            >
              <Select.Trigger />
              <Select.Content>
                <Select.Item value="ubuntu">Remote Ubuntu over SSH</Select.Item>
                <Select.Item value="alpine">Remote Alpine VM over SSH</Select.Item>
              </Select.Content>
            </Select.Root>
          </Field>
          <Field label="Host or IP">
            <TextField.Root
              placeholder="203.0.113.10"
              disabled={running}
              value={form.host}
              onChange={(event) => onChange({ ...form, host: event.target.value })}
            />
          </Field>
          {form.type === "ubuntu" ? (
            <Field label="Private Key">
              <Grid columns="1fr auto" gap="2">
                <TextField.Root
                  placeholder="Choose SSH private key"
                  value={form.keyPath}
                  disabled={running}
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
          ) : (
            <Box className="setup-guide">
              <Text size="2">
                Alpine Dune guests use the stock server-package SSH key, so only the guest IP is needed.
              </Text>
            </Box>
          )}
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

function LocalHyperVAttachDialog({
  open,
  form,
  running,
  onOpenChange,
  onChange,
  onAttach,
}: {
  open: boolean;
  form: LocalHyperVAttachForm;
  running: boolean;
  onOpenChange: (open: boolean) => void;
  onChange: (form: LocalHyperVAttachForm) => void;
  onAttach: () => void;
}) {
  const canAttach = form.vmName.trim().length > 0 && !running;
  const update = <K extends keyof LocalHyperVAttachForm>(key: K, value: LocalHyperVAttachForm[K]) => {
    onChange({ ...form, [key]: value });
  };
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Content maxWidth="480px">
        <Dialog.Title>Add Local Hyper-V Server</Dialog.Title>
        <Dialog.Description size="2" color="gray">
          Detect the vendor Hyper-V VM and read its host and in-guest server details.
        </Dialog.Description>
        <Flex direction="column" gap="3" mt="4">
          <label>
            <Text as="div" size="2" weight="medium" mb="1">
              VM name
            </Text>
            <TextField.Root
              value={form.vmName}
              disabled={running}
              onChange={(event) => update("vmName", event.target.value)}
              placeholder={defaultHyperVVmName}
            />
          </label>
          <label>
            <Text as="div" size="2" weight="medium" mb="1">
              Static IP
            </Text>
            <TextField.Root
              value={form.staticIp}
              disabled={running}
              onChange={(event) => update("staticIp", event.target.value)}
              placeholder="Only needed if Hyper-V does not report the guest IP"
            />
          </label>
        </Flex>
        <Flex gap="3" justify="end" mt="5">
          <Dialog.Close>
            <Button variant="soft" color="gray" disabled={running}>
              Cancel
            </Button>
          </Dialog.Close>
          <Button disabled={!canAttach} onClick={onAttach}>
            {running ? "Registering..." : "Register Server"}
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
          This only removes the saved server entry from this app. The remote host and Dune battlegroup will not be changed.
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
          <AlertDialog.Cancel disabled={busy}>
            <Button variant="soft" color="gray" disabled={busy}>
              Later
            </Button>
          </AlertDialog.Cancel>
          <AlertDialog.Action disabled={!update || busy}>
            <Button disabled={!update || busy} onClick={onInstall}>
              {busy ? "Installing..." : "Install update"}
            </Button>
          </AlertDialog.Action>
        </Flex>
      </AlertDialog.Content>
    </AlertDialog.Root>
  );
}
