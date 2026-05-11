import { type ComponentType, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
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
  { id: "install", label: "Create New Server" },
] as const;

type PageId = (typeof pages)[number]["id"] | "server-detail";

type NetworkMode = "static" | "dhcp";
type PlayerIpMode = "local" | "external";

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
type UpdateStatus = "idle" | "checking" | "available" | "current" | "installing" | "relaunching" | "failed";

type LogRow = {
  timestamp: string;
  level: LogLevel;
  scope: string;
  message: string;
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

type SetupForm = {
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
};

const defaultForm: SetupForm = {
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
  deepDesertPveInstances: "1",
  deepDesertPvpInstances: "0",
  deepDesertWarmServers: "0",
};

const zeroToFour = ["0", "1", "2", "3", "4"];
const oneToFour = ["1", "2", "3", "4"];
const playerPortForwards = [
  { ports: "7777-7810", protocol: "UDP", purpose: "Game servers" },
  { ports: "31982", protocol: "TCP", purpose: "RMQ" },
];

export function App() {
  const [activePage, setActivePage] = useState<PageId>("home");
  const [selectedServerName, setSelectedServerName] = useState<string | null>(null);
  const [form, setForm] = useState<SetupForm>(defaultForm);
  const [started, setStarted] = useState(false);
  const [setupRunning, setSetupRunning] = useState(false);
  const [setupRows, setSetupRows] = useState<LogRow[]>([]);
  const [initRows, setInitRows] = useState<LogRow[]>([]);
  const [rollbackOpen, setRollbackOpen] = useState(false);
  const [rollbackRunning, setRollbackRunning] = useState(false);
  const [failedRollbackRequest, setFailedRollbackRequest] = useState<RollbackRequest | null>(null);
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
  const calculatedMemory = useMemo(() => calculateRequiredMemory(form), [form]);
  const environmentGate = useMemo(
    () => setupEnvironmentGate(networkDetection, hostReadiness, networkAdapters),
    [hostReadiness, networkAdapters, networkDetection],
  );
  const environmentRows = useMemo(
    () =>
      environmentLogRows(
        networkDetection,
        hostReadiness,
        networkAdapters,
        driveCandidates,
        externalIp,
        environmentGate,
      ),
    [driveCandidates, environmentGate, externalIp, hostReadiness, networkAdapters, networkDetection],
  );
  const layoutPreview = useMemo(() => setupLayoutPreview(form), [form]);
  const selectedServer = useMemo(
    () => duneVms.find((candidate) => candidate.vm.name === selectedServerName) ?? null,
    [duneVms, selectedServerName],
  );
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
    if (!selectedServerName) return;
    if (duneVms.some((candidate) => candidate.vm.name === selectedServerName)) return;
    setSelectedServerName(null);
    if (activePage === "server-detail") setActivePage("home");
  }, [activePage, duneVms, selectedServerName]);

  useEffect(() => {
    appendInitRow(log.debug("updates", "Automatic update checks are disabled; use the manual check on Home."));
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

  const logRows = useMemo(
    () =>
      started
        ? [...initRows, ...environmentRows, ...setupRows]
        : [...initRows, ...environmentRows],
    [environmentRows, initRows, setupRows, started],
  );

  const startSetup = async () => {
    const request = setupRunRequest(form, calculatedMemory.gb);
    setStarted(true);
    setSetupRunning(true);
    setSetupRows([]);
    setFailedRollbackRequest(null);
    try {
      await invoke("start_full_setup", {
        request,
      });
    } catch (err) {
      console.error(err);
      setFailedRollbackRequest(rollbackRequestFromSetup(request));
      setRollbackOpen(true);
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
      <Flex direction="column" className="app-root">
        <Header
          activePage={activePage}
          selectedServerName={selectedServerName}
          servers={duneVms}
          onNavigate={setActivePage}
          onOpenServer={(name) => {
            setSelectedServerName(name);
            setActivePage("server-detail");
          }}
        />
        <Separator size="4" />
        <Box className="app-main has-log">
          {activePage === "home" ? (
            <HomePage
              environmentGate={environmentGate}
              networkDetection={networkDetection}
              vmDetection={vmDetection}
              hostReadiness={hostReadiness}
              networkAdapters={networkAdapters}
              externalIp={externalIp}
              duneVms={duneVms}
              updateStatus={updateStatus}
              availableUpdate={availableUpdate}
              updateProgress={updateProgress}
              onCheckUpdate={() => void checkForAppUpdate("manual")}
              onInstallUpdate={() => setUpdateDialogOpen(true)}
              onOpenServer={(name) => {
                setSelectedServerName(name);
                setActivePage("server-detail");
              }}
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
              update={update}
              onStart={startSetup}
            />
          ) : null}
          {activePage === "server-detail" && selectedServer ? (
            <ServerDetailsPage candidate={selectedServer} />
          ) : null}
          <LogWindow rows={logRows} />
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
      </Flex>
    </Theme>
  );
}

function Header({
  activePage,
  selectedServerName,
  servers,
  onNavigate,
  onOpenServer,
}: {
  activePage: PageId;
  selectedServerName: string | null;
  servers: DuneVmCandidate[];
  onNavigate: (page: PageId) => void;
  onOpenServer: (name: string) => void;
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
            selectedServerName={selectedServerName}
            servers={servers}
            onNavigate={onNavigate}
            onOpenServer={onOpenServer}
          />
        </Flex>
      </header>
    </Flex>
  );
}

function TopNav({
  activePage,
  onNavigate,
  selectedServerName,
  servers,
  onOpenServer,
}: {
  activePage: PageId;
  onNavigate: (page: PageId) => void;
  selectedServerName: string | null;
  servers: DuneVmCandidate[];
  onOpenServer: (name: string) => void;
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
          {servers.map((candidate) => (
            <TabNav.Link
              key={candidate.vm.name}
              href="#"
              active={activePage === "server-detail" && selectedServerName === candidate.vm.name}
              onClick={(event) => {
                event.preventDefault();
                onOpenServer(candidate.vm.name);
              }}
            >
              {candidate.vm.name}
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
  updateStatus,
  availableUpdate,
  updateProgress,
  onCheckUpdate,
  onInstallUpdate,
  onOpenServer,
}: {
  environmentGate: EnvironmentGate;
  networkDetection: DetectionState;
  vmDetection: DetectionState;
  hostReadiness: HostReadiness | null;
  networkAdapters: NetworkAdapterCandidate[];
  externalIp: string | null;
  duneVms: DuneVmCandidate[];
  updateStatus: UpdateStatus;
  availableUpdate: Update | null;
  updateProgress: string | null;
  onCheckUpdate: () => void;
  onInstallUpdate: () => void;
  onOpenServer: (name: string) => void;
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
            label="Dune VMs"
            value={vmDetectionReady ? `${duneVms.length} found` : vmDetection === "failed" ? "Failed" : "Checking"}
            tone={vmDetectionReady ? "green" : vmDetection === "failed" ? "red" : "amber"}
          />
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

        <Box className="setup-scroll">
          <Flex direction="column" gap="3">
            {duneVms.length > 0 ? (
              duneVms.map((candidate) => (
                <ServerCard
                  key={candidate.vm.name}
                  candidate={candidate}
                  compact
                  onOpen={() => onOpenServer(candidate.vm.name)}
                />
              ))
            ) : (
              <EmptyState
                title={vmDetection === "failed" ? "VM detection failed" : "No Dune servers detected"}
                body="Create a new server from the Create New Server tab, or keep this page open while detection completes."
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
  onOpen,
}: {
  candidate: DuneVmCandidate;
  compact?: boolean;
  onOpen?: () => void;
}) {
  const vm = candidate.vm;
  const primaryIp = vm.ipv4Addresses[0] ?? "No IPv4 reported";
  const diskLabel = vm.diskSizeBytes > 0 ? `${formatGiB(vm.diskSizeBytes)} disk` : "Disk size unknown";
  const usedDiskLabel = vm.diskFileSizeBytes > 0 ? `${formatGiB(vm.diskFileSizeBytes)} used` : "usage unknown";

  return (
    <Box
      className={onOpen ? "server-card is-clickable" : "server-card"}
      role={onOpen ? "button" : undefined}
      tabIndex={onOpen ? 0 : undefined}
      onClick={onOpen}
      onKeyDown={(event) => {
        if (!onOpen) return;
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpen();
        }
      }}
    >
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

    </Box>
  );
}

function ServerDetailsPage({ candidate }: { candidate: DuneVmCandidate }) {
  const vm = candidate.vm;
  const primaryIp = vm.ipv4Addresses[0] ?? "";
  const diskLabel =
    vm.diskSizeBytes > 0
      ? `${formatGiB(vm.diskSizeBytes)} disk; ${formatGiB(vm.diskFileSizeBytes)} used`
      : "Unknown";

  return (
    <Card size="3" variant="surface" className="pane setup-pane">
      <Flex direction="column" gap="4" height="100%" minHeight="0">
        <Box>
          <Heading size="5">{vm.name}</Heading>
          <Text as="p" size="2" color="gray">
            Server details and future update controls.
          </Text>
        </Box>

        <Box className="setup-scroll">
          <Flex direction="column" gap="5">
            <SetupSection icon={DesktopIcon} title="Host and VM">
              <Flex direction="column" gap="2">
                <FormRow label="VM Name">
                  <TextField.Root value={vm.name} readOnly />
                </FormRow>
                <FormRow label="VM Location">
                  <TextField.Root value={vm.path || vm.configurationLocation} readOnly />
                </FormRow>
                <FormRow label="VM State">
                  <TextField.Root value={vm.rawState} readOnly />
                </FormRow>
                <FormRow label="Memory">
                  <TextField.Root value={formatGiB(vm.memoryAssignedBytes)} readOnly />
                </FormRow>
                <FormRow label="CPU Cores">
                  <TextField.Root value={vm.processorCount ? String(vm.processorCount) : "Unknown"} readOnly />
                </FormRow>
                <FormRow label="Disk">
                  <TextField.Root value={diskLabel} readOnly />
                </FormRow>
                <FormRow label="VHD">
                  <TextField.Root value={vm.hardDiskPaths[0] ?? "No disk reported"} readOnly />
                </FormRow>
              </Flex>
            </SetupSection>

            <SetupSection icon={RocketIcon} title="World Layout">
              <Grid columns="2" gap="3">
                <Field label="Hagga Basin">
                  <Select.Root value="unknown" disabled>
                    <Select.Trigger />
                    <Select.Content>
                      <Select.Item value="unknown">Detect from manager API</Select.Item>
                    </Select.Content>
                  </Select.Root>
                </Field>
                <Field label="Social Hubs">
                  <TextField.Root value="Detect from manager API" readOnly />
                </Field>
                <Field label="Deep Desert PvE">
                  <Select.Root value="unknown" disabled>
                    <Select.Trigger />
                    <Select.Content>
                      <Select.Item value="unknown">Detect from manager API</Select.Item>
                    </Select.Content>
                  </Select.Root>
                </Field>
                <Field label="Deep Desert PvP">
                  <Select.Root value="unknown" disabled>
                    <Select.Trigger />
                    <Select.Content>
                      <Select.Item value="unknown">Detect from manager API</Select.Item>
                    </Select.Content>
                  </Select.Root>
                </Field>
              </Grid>
            </SetupSection>

            <SetupSection icon={MixIcon} title="Network">
              <Flex direction="column" gap="2">
                <FormRow label="VM IP">
                  <TextField.Root value={primaryIp || "No IPv4 reported"} readOnly />
                </FormRow>
                <FormRow label="Switch">
                  <TextField.Root value={vm.switchNames.join(", ") || "No switch reported"} readOnly />
                </FormRow>
                <FormRow label="Reported IPs">
                  <TextArea value={vm.ipv4Addresses.join("\n") || "No IPv4 reported"} readOnly />
                </FormRow>
              </Flex>
              <Text as="p" size="2" color="gray">
                IP changes are read-only here for now. Changing VM IP can interrupt SSH and Manager API access,
                invalidate port-forwarding rules, and require player-facing IP settings to be rewritten.
              </Text>
            </SetupSection>

            <SetupSection icon={GlobeIcon} title="Self-Host">
              <FormRow label="Service Token">
                <TextField.Root value="Stored only during setup; not displayed here" readOnly />
              </FormRow>
            </SetupSection>
          </Flex>
        </Box>
      </Flex>
    </Card>
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
  update,
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
  update: <K extends keyof SetupForm>(key: K, value: SetupForm[K]) => void;
  onStart: () => void;
}) {
  const deepDesertEnabled = layoutPreview.deepDesertTotal > 0;
  const warmOptions = zeroTo(layoutPreview.deepDesertTotal);
  const requirements = setupRequirementStatus(
    calculatedMemory,
    form.diskGb,
    form.processorCount,
    form.vmDestination,
    hostReadiness,
    driveCandidates,
  );
  const hasServiceToken = form.tokenSource.trim().length > 0;
  const setupIssues = setupBlockingIssues(
    environmentGate,
    requirements,
    hasServiceToken,
    vmDestinationHasVm,
  );
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
                      {zeroToFour.map((value) => (
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
                      {zeroToFour.map((value) => (
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

function rollbackRequestFromSetup(request: SetupRunRequest): RollbackRequest {
  return {
    vmName: request.vmName,
    vmDestination: request.vmDestination,
    switchName: request.switchName,
  };
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
  const deepDesertInstances =
    parsePositiveInt(form.deepDesertPveInstances) + parsePositiveInt(form.deepDesertPvpInstances);
  const warmServers = Math.min(parsePositiveInt(form.deepDesertWarmServers), deepDesertInstances);
  const normalized = {
    ...form,
    includeSocial: deepDesertInstances > 0 ? true : form.includeSocial,
    deepDesertWarmServers: String(warmServers),
  };
  if (normalized.playerIpMode === "local" && normalized.staticIp && normalized.playerIp !== normalized.staticIp) {
    return { ...normalized, playerIp: normalized.staticIp };
  }
  return normalized;
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

function LogWindow({ rows }: { rows: LogRow[] }) {
  const bodyRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const body = bodyRef.current;
    if (!body) return;
    const distanceFromBottom = body.scrollHeight - body.scrollTop - body.clientHeight;
    if (distanceFromBottom < 80) {
      body.scrollTop = body.scrollHeight;
    }
  }, [rows]);

  return (
    <Card size="3" variant="surface" className="pane">
      <Flex direction="column" height="100%" minHeight="0">
        <Box className="log-body" ref={bodyRef}>
          <Flex direction="column" gap="0">
            {rows.map((row, index) => (
              <Grid
                key={`${row.timestamp}-${row.scope}-${row.level}-${index}`}
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
