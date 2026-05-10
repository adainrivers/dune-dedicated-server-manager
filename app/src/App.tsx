import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  Database,
  HardDrive,
  Map,
  RadioTower,
  Server,
  SlidersHorizontal,
  Terminal,
  Users
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { AppHeader, AppSidebar, StatusStrip } from "./components/appShell";
import { useManagerTelemetry } from "./hooks/useManagerTelemetry";
import { BattleGroupsPanel } from "./views/battlegroups";
import { ConfigView } from "./views/config";
import { DirectorView } from "./views/director";
import { EnvironmentPanel } from "./views/environment";
import { HostVmPanels, VmRequiredNotice } from "./views/hostVm";
import { DirectorUnavailableNotice, ManagerApiPanel, ManagerToolsRequiredNotice } from "./views/managerApi";
import { LogsPanel } from "./views/logs";
import { PlayersPanel } from "./views/players";
import { WorkloadsPanel } from "./views/workloads";
import type {
  AppConfig,
  BattleGroupDetail,
  BattleGroupSummary,
  CommandFailure,
  DirectorMapSummary,
  DirectorPlayerSummary,
  FlsDraft,
  GuestConnection,
  HostStatus,
  ManagerApiInstallResult,
  ManagerApiStatus,
  ManagerWorkloads,
  MapOverrideDraft,
  NavItem,
  TransferDraft,
  ViewKey,
  VmStatus,
  Workloads
} from "./types";
import {
  asError,
  boolAt,
  defaultConfig,
  delay,
  generateToken,
  managerWorkloadsToUi,
  nullableNumber,
  numberAt
} from "./utils";

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
  const {
    managerStatus,
    setManagerStatus,
    managerTelemetry,
    setManagerTelemetry,
    managerSocketState,
    managerError
  } = useManagerTelemetry(configLoaded, config);

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
  const navItems: NavItem[] = [
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
    if (nextVm && nextVm.state.toLowerCase() !== "off") {
      setVm(nextVm);
    }
    await pollVmLifecycle("running", "Starting", "off");
    setBusy(false);
  }

  async function stopVm() {
    setBusy(true);
    if (vm) {
      setVm({ ...vm, state: "Stopping", status: "Stopping" });
    }
    const nextVm = await capture("Stop VM", () => invoke<VmStatus>("stop_vm", { vmName: config.vmName }));
    if (nextVm && nextVm.state.toLowerCase() !== "running") {
      setVm(nextVm);
    }
    await pollVmLifecycle("off", "Stopping", "running");
    setBusy(false);
  }

  async function pollVmLifecycle(targetState: string, transitionState: string, staleState: string) {
    let lastVm: VmStatus | null = null;
    for (let index = 0; index < 20; index += 1) {
      await delay(1500);
      const nextVm = await capture("VM lifecycle", () => invoke<VmStatus>("get_vm_status", { vmName: config.vmName }));
      if (!nextVm) return;
      lastVm = nextVm;
      const nextState = nextVm.state.toLowerCase();
      if (nextState === targetState) {
        setVm(nextVm);
        return;
      }
      setVm(nextState === staleState ? { ...nextVm, state: transitionState, status: transitionState } : nextVm);
    }
    if (lastVm) setVm(lastVm);
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
      <AppSidebar navItems={navItems} activeView={activeView} onSelect={setActiveView} />

      <section className="content">
        <AppHeader title={pageTitle} subtitle={pageSubtitle} busy={busy} onRefresh={refresh} />

        <StatusStrip
          admin={host?.isElevated ?? false}
          vmState={vm?.state}
          sshConnected={guest?.connected ?? false}
          kubectlReady={guest?.kubectl ?? false}
          battleGroupPhase={selectedBattleGroup?.phase}
          managerReadiness={managerReadiness}
        />

        {(activeView === "overview" || activeView === "config") && (
          <EnvironmentPanel
            config={config}
            vm={vm}
            managerInstallNamespace={managerInstallNamespace}
            configSaved={configSaved}
            busy={busy}
            onDetect={detectEnvironment}
          />
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
          <VmRequiredNotice
            vm={vm}
            busy={busy}
            canControlVm={canControlVm}
            vmIsRunning={vmIsRunning}
            vmIsChanging={vmIsChanging}
            vmIsStarting={vmIsStarting}
            onStart={startVm}
          />
        )}

        {(activeView === "overview" || activeView === "host") && (
          <HostVmPanels
            host={host}
            vm={vm}
            guest={guest}
            busy={busy}
            canControlVm={canControlVm}
            vmIsRunning={vmIsRunning}
            vmIsChanging={vmIsChanging}
            startVmDisabledReason={startVmDisabledReason}
            stopVmDisabledReason={stopVmDisabledReason}
            onStart={startVm}
            onStop={stopVm}
          />
        )}

        {(activeView === "overview" || activeView === "manager") && (
          <ManagerApiPanel
            config={config}
            managerInstallNamespace={managerInstallNamespace}
            managerReadiness={managerReadiness}
            managerTelemetryState={managerTelemetryState}
            managerStatus={managerStatus}
            managerTelemetry={managerTelemetry}
            managerInstall={managerInstall}
            managerError={managerError}
            busy={busy}
            canInstallManagerApi={canInstallManagerApi}
            onInstall={installManagerApi}
          />
        )}

        {!managerToolsInstalled && (activeView === "overview" || activeView === "manager" || activeViewRequiresManager) && (
          <ManagerToolsRequiredNotice
            busy={busy}
            canInstallManagerApi={canInstallManagerApi}
            onInstall={installManagerApi}
          />
        )}

        {managerToolsInstalled && !directorAvailable && (activeView === "overview" || activeView === "manager") && (
          <DirectorUnavailableNotice busy={busy} onRefresh={refresh} />
        )}

        {directorAvailable && (activeView === "overview" || activeView === "players") && (
          <PlayersPanel players={directorPlayers} />
        )}

        {managerToolsInstalled && (activeView === "overview" || activeView === "battlegroups") && (
          <BattleGroupsPanel
            battleGroups={battleGroups}
            selectedBattleGroup={selectedBattleGroup}
            busy={busy}
            canUseManager={canUseManager}
            battleGroupIsStopped={battleGroupIsStopped}
            battleGroupIsRunning={battleGroupIsRunning}
            snapshotPath={snapshotPath}
            onStart={() => setBattleGroupRunning(true)}
            onStop={() => setBattleGroupRunning(false)}
            onRestart={restartBattleGroup}
            onExport={exportLiveConfig}
            onSelect={(group) => {
              setSelectedNamespace(group.namespace);
              void loadBattleGroupDetail(group);
              void loadWorkloads(group.namespace);
            }}
          />
        )}

        {managerToolsInstalled && activeView === "config" && (
          <ConfigView
            battleGroupDetail={battleGroupDetail}
            directorAvailable={directorAvailable}
            directorFlsConfig={directorFlsConfig}
            directorTransferConfig={directorTransferConfig}
            directorMaps={directorMaps}
            selectedDirectorMapSummary={selectedDirectorMapSummary}
            flsDraft={flsDraft}
            transferDraft={transferDraft}
            mapOverrideDraft={mapOverrideDraft}
            busy={busy}
            onFlsDraftChange={setFlsDraft}
            onTransferDraftChange={setTransferDraft}
            onMapOverrideDraftChange={setMapOverrideDraft}
            onSaveFlsConfig={saveFlsConfig}
            onClearFlsConfig={clearFlsConfig}
            onSaveTransferConfig={saveTransferConfig}
            onClearTransferConfig={clearTransferConfig}
            onSelectMap={setSelectedDirectorMap}
            onSaveMapOverride={saveMapOverride}
            onClearMapOverride={clearMapOverride}
          />
        )}

        {directorAvailable && activeView === "director" && (
          <DirectorView
            directorMaps={directorMaps}
            busy={busy}
            onReload={loadDirectorData}
            onEditMap={(mapName) => {
              setSelectedDirectorMap(mapName);
              setActiveView("config");
            }}
            onClearMapOverride={clearMapOverride}
          />
        )}

        {managerToolsInstalled && (activeView === "overview" || activeView === "workloads") && (
          <WorkloadsPanel pods={pods} services={services} />
        )}

        {managerToolsInstalled && activeView === "logs" && (
          <LogsPanel />
        )}
      </section>
    </main>
  );
}
