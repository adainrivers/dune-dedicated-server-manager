<script lang="ts">
  import { onMount, tick } from "svelte";
  import Card from "./Card.svelte";
  import PlayerBucket from "./PlayerBucket.svelte";
  import {
    ApiError,
    api,
    type BattlegroupSummary,
    type DatabaseMaintenanceItem,
    type DatabaseMaintenanceResponse,
    type DatabasePlayerStatisticsResponse,
    type DatabasePlayerTagsUpdateResponse,
    type DatabasePlayerSummary,
    type DatabasePlayersResponse,
    type DatabaseWorldPartition,
    type DatabaseWorldPartitionUpdateResponse,
    type DatabaseWorldPartitionsResponse,
    type DirectorCapabilities,
    type DirectorMapConfigDetail,
    type DirectorPathCapability,
    type DirectorPlayerLists,
    type EventsResponse,
    type EventSummary,
    type IniSection,
    type LogExportResponse,
    type LogsResponse,
    type ManagerLogResponse,
    type ManagerSelf,
    type Overview,
    type PersistentVolumeClaimSummary,
    type Session,
    type StorageResponse,
    type TelemetryEnvelope,
    type TelemetrySnapshot,
    type UserSettingsBackupCreateResponse,
    type UserSettingsBackupsResponse,
    type UserSettingsCatalog,
    type UserSettingsFile,
    type UserSettingsPreviewResponse,
    type UserSettingsRestoreResponse,
    type UserSettingsUpdateResponse,
    type WorldLayout,
    type WorldLayoutUpdateResponse,
  } from "./api";

  type Page =
    | "dashboard"
    | "battlegroup"
    | "workloads"
    | "storage"
    | "database"
    | "layout"
    | "config"
    | "director"
    | "players"
    | "logs"
    | "settings";

  type NavItem = {
    page: Page;
    label: string;
  };

  type JsonField = {
    pointer: string;
    label: string;
    value: string;
    kind: "boolean" | "number" | "string" | "null";
  };

  type PlayerActivityRow = {
    id: string;
    status: string;
    buckets: string[];
  };

  const navItems: NavItem[] = [
    { page: "dashboard", label: "Command Center" },
    { page: "players", label: "Players" },
    { page: "layout", label: "World Layout" },
    { page: "config", label: "Game Settings" },
    { page: "director", label: "Director Rules" },
    { page: "database", label: "Backups" },
    { page: "battlegroup", label: "Server Control" },
    { page: "workloads", label: "Workloads" },
    { page: "storage", label: "Storage" },
    { page: "logs", label: "Logs" },
    { page: "settings", label: "Manager" },
  ];

  let session: Session | null = null;
  let token = "";
  let loading = true;
  let signingIn = false;
  let error = "";
  let uiErrors: string[] = [];
  let page: Page = "dashboard";
  let overview: Overview | null = null;
  let layout: WorldLayout | null = null;
  let layoutSaving = false;
  let layoutNotice = "";
  let selectedPod = "";
  let selectedContainer = "";
  let logLines: string[] = [];
  let logViewer: HTMLDivElement | null = null;
  let logStreamSocket: WebSocket | null = null;
  let logStreaming = false;
  let logExporting = false;
  let logStreamError = "";
  let titleDraft = "";
  let lifecycleBusy = "";
  let settingsCatalog: UserSettingsCatalog | null = null;
  let selectedSettingsFile = "game";
  let settingsFile: UserSettingsFile | null = null;
  let settingsDraft = "";
  let settingsSaving = false;
  let settingsPreviewBusy = false;
  let settingsPreview: UserSettingsPreviewResponse | null = null;
  let settingsBackupBusy = "";
  let settingsAutoLoading = false;
  let settingsBackups: UserSettingsBackupsResponse | null = null;
  let settingsNotice = "";
  let settingsFilter = "";
  let directorFlsDraft = "";
  let directorTransferDraft = "";
  let selectedDirectorMap = "";
  let directorMapDetail: DirectorMapConfigDetail | null = null;
  let directorMapDraft = "";
  let directorAutoLoading = false;
  let directorCapabilities: DirectorCapabilities | null = null;
  let directorApiSelection = "";
  let directorApiBody = "{}";
  let directorApiResult = "";
  let directorNotice = "";
  let directorBusy = false;
  let directorApiBusy = false;
  let telemetrySocket: WebSocket | null = null;
  let telemetryConnected = false;
  let telemetrySnapshots = 0;
  let telemetryLastAt = "";
  let telemetryError = "";
  let playerLists: DirectorPlayerLists | null = null;
  let playersBusy = false;
  let playersFull = false;
  let playerFilter = "";
  let databasePlayers: DatabasePlayersResponse | null = null;
  let databasePlayersBusy = false;
  let playerTagDrafts: Record<number, string> = {};
  let playerTagBusy: Record<number, boolean> = {};
  let playerStatistics: DatabasePlayerStatisticsResponse | null = null;
  let playerStatisticsBusy = false;
  let workloadFilter = "";
  let events: EventSummary[] = [];
  let eventsBusy = false;
  let storageClaims: PersistentVolumeClaimSummary[] = [];
  let storageBusy = false;
  let storageFilter = "";
  let databaseMaintenance: DatabaseMaintenanceResponse | null = null;
  let databaseWorldPartitions: DatabaseWorldPartitionsResponse | null = null;
  let databaseBusy = false;
  let databaseTablesBusy = false;
  let partitionSaving: Record<number, boolean> = {};
  let partitionLabelDrafts: Record<number, string> = {};
  let databaseActionBusy = false;
  let databaseFilter = "";
  let databaseNotice = "";
  let managerSelf: ManagerSelf | null = null;
  let managerLogs: ManagerLogResponse | null = null;
  let managerBusy = "";

  $: battlegroup = overview?.battlegroups[0] ?? null;
  $: pods = overview?.workloads.pods ?? [];
  $: services = overview?.workloads.services ?? [];
  $: selectedPodSummary = pods.find((pod) => pod.name === selectedPod);
  $: battlegroupStopped = battlegroup?.stop ?? true;
  $: settingsDraftSections = settingsDraft ? parseIniSections(settingsDraft) : [];
  $: visibleSettingsSections = filterIniSections(settingsDraftSections, settingsFilter).slice(0, 16);
  $: visiblePods = filterPods(pods, workloadFilter);
  $: visibleServices = filterServices(services, workloadFilter);
  $: visibleEvents = filterEvents(events, workloadFilter);
  $: visibleStorageClaims = filterStorageClaims(storageClaims, storageFilter);
  $: visibleDatabaseItems = filterDatabaseMaintenance(databaseMaintenanceItems(databaseMaintenance), databaseFilter);
  $: visibleWorldPartitions = filterWorldPartitions(databaseWorldPartitions?.rows ?? [], databaseFilter);
  $: layoutMemory = layout ? estimateLayoutMemory(layout) : null;
  $: layoutDeepDesertMode = layout
    ? layout.deepDesertPvpInstances > 0
      ? "pvp"
      : layout.deepDesertPveInstances > 0
        ? "pve"
        : "off"
    : "off";
  $: notReadyPods = pods.filter((pod) => !pod.ready);
  $: runningPods = pods.filter((pod) => pod.ready).length;
  $: onlineMaps = (overview?.maps ?? []).filter((map) => map.servers.some((server) => server.status === "Running")).length;
  $: dashboardMaps = selectDashboardMaps(overview);
  $: directorFlsFields = jsonPrimitiveFields(directorFlsDraft).slice(0, 80);
  $: directorTransferFields = jsonPrimitiveFields(directorTransferDraft).slice(0, 80);
  $: directorMapFields = jsonPrimitiveFields(directorMapDraft).slice(0, 80);
  $: playerRows = playerActivityRows(playerLists);
  $: visiblePlayerRows = filterPlayerRows(playerRows, playerFilter);
  $: visibleDatabasePlayers = filterDatabasePlayers(databasePlayers?.rows ?? [], playerFilter);
  $: serverHealth = deriveServerHealth(overview, battlegroup, notReadyPods);
  $: nextActions = deriveNextActions(battlegroup, overview, databaseMaintenance, lifecycleBusy);
  $: if (selectedContainer && selectedPodSummary && !selectedPodSummary.containers.includes(selectedContainer)) {
    selectedContainer = "";
  }

  onMount(() => {
    const reportError = (value: unknown) => {
      const text = value instanceof Error ? value.message : String(value || "Unexpected UI error");
      uiErrors = [text, ...uiErrors].slice(0, 4);
    };
    const errorHandler = (event: ErrorEvent) => reportError(event.error || event.message);
    const rejectionHandler = (event: PromiseRejectionEvent) => reportError(event.reason);

    window.addEventListener("error", errorHandler);
    window.addEventListener("unhandledrejection", rejectionHandler);
    void initialize();
    const timer = window.setInterval(() => {
      if (session) void refresh(false);
    }, 10000);

    return () => {
      window.clearInterval(timer);
      window.removeEventListener("error", errorHandler);
      window.removeEventListener("unhandledrejection", rejectionHandler);
      stopTelemetry();
      stopLogStream();
    };
  });

  async function initialize() {
    await loadSession();
    if (session) {
      await refresh();
      startTelemetry();
    }
    loading = false;
  }

  async function loadSession() {
    try {
      const current = await api<Session>("/api/auth/session");
      session = current.authenticated ? current : null;
    } catch {
      session = null;
    }
  }

  function openPage(nextPage: Page) {
    page = nextPage;
    if (nextPage === "config" && !settingsFile && !settingsAutoLoading) {
      settingsAutoLoading = true;
      void loadSettingsFile().finally(() => {
        settingsAutoLoading = false;
      });
    }
    if (nextPage === "database" && !databaseMaintenance && !databaseBusy) {
      void loadDatabaseMaintenance(false);
    }
    if (nextPage === "director" && !directorFlsDraft && !directorTransferDraft && !directorAutoLoading) {
      directorAutoLoading = true;
      void loadDirectorConfig().finally(() => {
        directorAutoLoading = false;
      });
    }
    if (nextPage === "players" && !playerLists && !playersBusy) {
      void loadPlayerLists(false);
    }
    if (nextPage === "players" && !databasePlayers && !databasePlayersBusy) {
      void loadDatabasePlayers(false);
    }
    if ((nextPage === "dashboard" || nextPage === "players") && !playerStatistics && !playerStatisticsBusy) {
      void loadPlayerStatistics(false);
    }
  }

  async function signIn() {
    signingIn = true;
    error = "";
    try {
      session = await api<Session>("/api/auth/login", {
        method: "POST",
        body: JSON.stringify({ token }),
      });
      token = "";
      await refresh();
      startTelemetry();
    } catch (err) {
      error = message(err);
    } finally {
      signingIn = false;
    }
  }

  async function logout() {
    await api("/api/auth/logout", { method: "POST" });
    stopTelemetry();
    stopLogStream();
    session = null;
    overview = null;
    layout = null;
    settingsCatalog = null;
    settingsFile = null;
    settingsBackups = null;
    managerSelf = null;
    managerLogs = null;
  }

  async function refresh(showError = true) {
    try {
      overview = await api<Overview>("/api/overview");
      if (!selectedPod && overview.workloads.pods[0]) selectedPod = overview.workloads.pods[0].name;
      if (!selectedDirectorMap && overview.maps[0]) selectedDirectorMap = overview.maps[0].name;
      if (battlegroup) {
        titleDraft = titleDraft || battlegroup.title;
        layout = await api<WorldLayout>(`/api/battlegroups/${battlegroup.namespace}/${battlegroup.name}/layout`);
      }
      if (!settingsCatalog) settingsCatalog = await api<UserSettingsCatalog>("/api/config/user-settings");
      if (!databaseMaintenance) void loadDatabaseMaintenance(false);
      if (!playerStatistics) void loadPlayerStatistics(false);
    } catch (err) {
      if (err instanceof ApiError && err.status === 401) {
        session = null;
        stopTelemetry();
      }
      if (showError) error = message(err);
    }
  }

  function startTelemetry() {
    if (!session || telemetrySocket) return;
    telemetryError = "";
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const socket = new WebSocket(`${protocol}//${window.location.host}/api/telemetry`);
    telemetrySocket = socket;

    socket.onopen = () => {
      telemetryConnected = true;
      telemetryError = "";
    };
    socket.onclose = () => {
      if (telemetrySocket === socket) telemetrySocket = null;
      telemetryConnected = false;
    };
    socket.onerror = () => {
      telemetryError = "Telemetry stream failed.";
    };
    socket.onmessage = (event) => {
      try {
        const envelope = JSON.parse(event.data) as TelemetryEnvelope;
        telemetryLastAt = new Date(envelope.timeUnixMs).toLocaleTimeString();
        if (envelope.eventType === "snapshot") {
          telemetrySnapshots += 1;
          applyTelemetrySnapshot(envelope.payload as TelemetrySnapshot);
        } else {
          const payload = envelope.payload as { message?: string };
          telemetryError = payload.message || "Telemetry stream reported an error.";
        }
      } catch {
        telemetryError = "Telemetry payload was not valid JSON.";
      }
    };
  }

  function stopTelemetry() {
    const socket = telemetrySocket;
    telemetrySocket = null;
    telemetryConnected = false;
    if (socket && socket.readyState <= WebSocket.OPEN) socket.close();
  }

  function applyTelemetrySnapshot(snapshot: TelemetrySnapshot) {
    if (!overview) return;
    overview = {
      ...overview,
      battlegroups: snapshot.battlegroups,
      workloads: {
        ...overview.workloads,
        pods: snapshot.pods,
        services: snapshot.services,
      },
      status: {
        ...overview.status,
        battlegroups: snapshot.battlegroups.length,
        pods: snapshot.pods.length,
        services: snapshot.services.length,
      },
    };
  }

  async function lifecycle(action: "start" | "stop" | "restart") {
    if (!battlegroup) return;
    if (action === "stop" && !window.confirm("Stop the battlegroup now? Connected players may be disconnected.")) {
      return;
    }
    error = "";
    lifecycleBusy = action;
    try {
      await api(`/api/battlegroups/${battlegroup.namespace}/${battlegroup.name}/${action}`, { method: "POST" });
      await refresh();
    } catch (err) {
      error = message(err);
    } finally {
      lifecycleBusy = "";
    }
  }

  async function saveLayout() {
    if (!battlegroup || !layout) return;
    error = "";
    layoutNotice = "";
    layoutSaving = true;
    try {
      const result = await api<WorldLayoutUpdateResponse>(
        `/api/battlegroups/${battlegroup.namespace}/${battlegroup.name}/layout`,
        {
          method: "PUT",
          body: JSON.stringify({
            haggaBasinInstances: layout.haggaBasinInstances,
            socialHubsEnabled: layout.socialHubsEnabled,
            deepDesertPveInstances: layout.deepDesertPveInstances,
            deepDesertPvpInstances: layout.deepDesertPvpInstances,
          }),
        },
      );
      layout = result.layout;
      layoutNotice = result.restartRequired
        ? "Layout saved. Restart the battlegroup for every runtime component to converge."
        : "Layout saved.";
      if (result.warnings.length) error = result.warnings.join(" ");
      await refresh(false);
    } catch (err) {
      error = message(err);
    } finally {
      layoutSaving = false;
    }
  }

  function setDeepDesertMode(mode: "off" | "pve" | "pvp") {
    if (!layout) return;
    layout = {
      ...layout,
      deepDesertPveInstances: mode === "pve" ? 1 : 0,
      deepDesertPvpInstances: mode === "pvp" ? 1 : 0,
    };
    layoutNotice = "";
  }

  async function saveTitle() {
    if (!battlegroup) return;
    error = "";
    try {
      await api(`/api/battlegroups/${battlegroup.namespace}/${battlegroup.name}/settings`, {
        method: "PATCH",
        body: JSON.stringify({ title: titleDraft }),
      });
      await refresh();
    } catch (err) {
      error = message(err);
    }
  }

  async function loadLogs() {
    if (!selectedPod) return;
    stopLogStream();
    const query = new URLSearchParams({ pod: selectedPod, tail: "400" });
    if (selectedContainer) query.set("container", selectedContainer);
    try {
      const logs = await api<LogsResponse>(`/api/logs?${query}`);
      logLines = logs.lines;
    } catch (err) {
      error = message(err);
    }
  }

  function openPodLogs(podName: string, container = "") {
    selectedPod = podName;
    selectedContainer = container;
    page = "logs";
    void loadLogs();
  }

  async function exportAllLogs() {
    logExporting = true;
    logStreamError = "";
    error = "";
    try {
      const result = await api<LogExportResponse>("/api/logs/export?tail=500");
      const createdAt = new Date(result.generatedAtUnixMs).toISOString().replace(/[:.]/g, "-");
      const fileName = `dune-log-export-${createdAt}.json`;
      const blob = new Blob([JSON.stringify(result, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = fileName;
      link.click();
      URL.revokeObjectURL(url);
      const containerCount = result.pods.reduce((count, pod) => count + pod.containers.length, 0);
      logLines = [
        `Exported ${result.pods.length} pods and ${containerCount} containers.`,
        `Tail: ${result.tailLines} lines per container.`,
        result.errors.length ? `Errors: ${result.errors.length}` : "Errors: 0",
      ];
    } catch (err) {
      error = message(err);
    } finally {
      logExporting = false;
    }
  }

  function startLogStream() {
    if (!selectedPod) return;
    stopLogStream();
    logStreamError = "";
    logLines = [];

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const query = new URLSearchParams({ pod: selectedPod, tail: "150" });
    if (selectedContainer) query.set("container", selectedContainer);
    const socket = new WebSocket(`${protocol}//${window.location.host}/api/logs/stream?${query}`);
    logStreamSocket = socket;

    socket.onopen = () => {
      logStreaming = true;
    };
    socket.onclose = () => {
      if (logStreamSocket === socket) logStreamSocket = null;
      logStreaming = false;
    };
    socket.onerror = () => {
      logStreamError = "Log stream failed.";
    };
    socket.onmessage = (event) => {
      try {
        const payload = JSON.parse(event.data) as { type: "line" | "error"; line?: string; message?: string };
        if (payload.type === "line") appendLogLine(payload.line || "");
        if (payload.type === "error") logStreamError = payload.message || "Log stream reported an error.";
      } catch {
        appendLogLine(String(event.data));
      }
    };
  }

  function stopLogStream() {
    const socket = logStreamSocket;
    logStreamSocket = null;
    logStreaming = false;
    if (socket && socket.readyState <= WebSocket.OPEN) socket.close();
  }

  async function appendLogLine(line: string) {
    const nearEnd = !logViewer || logViewer.scrollTop + logViewer.clientHeight >= logViewer.scrollHeight - 24;
    logLines = [...logLines.slice(-999), line];
    await tick();
    if (nearEnd && logViewer) logViewer.scrollTop = logViewer.scrollHeight;
  }

  async function loadPlayerLists(full = playersFull) {
    playersBusy = true;
    playersFull = full;
    error = "";
    try {
      const query = new URLSearchParams({ full: full ? "true" : "false" });
      playerLists = await api<DirectorPlayerLists>(`/api/director/players?${query}`);
    } catch (err) {
      error = message(err);
    } finally {
      playersBusy = false;
    }
  }

  async function loadDatabasePlayers(showError = true) {
    databasePlayersBusy = true;
    if (showError) error = "";
    try {
      databasePlayers = await api<DatabasePlayersResponse>("/api/database/players");
    } catch (err) {
      if (showError) error = message(err);
    } finally {
      databasePlayersBusy = false;
    }
  }

  function updatePlayerTagDraft(accountId: number, value: string) {
    playerTagDrafts = { ...playerTagDrafts, [accountId]: value };
  }

  async function savePlayerTag(player: DatabasePlayerSummary) {
    const tag = (playerTagDrafts[player.accountId] || "").trim();
    if (!tag) {
      error = "Enter a tag before adding it.";
      return;
    }
    await updatePlayerTags(player.accountId, tag, "POST");
    playerTagDrafts = { ...playerTagDrafts, [player.accountId]: "" };
  }

  async function removePlayerTag(player: DatabasePlayerSummary, tag: string) {
    await updatePlayerTags(player.accountId, tag, "DELETE");
  }

  async function updatePlayerTags(accountId: number, tag: string, method: "POST" | "DELETE") {
    playerTagBusy = { ...playerTagBusy, [accountId]: true };
    error = "";
    try {
      const result = await api<DatabasePlayerTagsUpdateResponse>(`/api/database/players/${accountId}/tags`, {
        method,
        body: JSON.stringify({ tag }),
      });
      if (databasePlayers) {
        databasePlayers = {
          ...databasePlayers,
          rows: databasePlayers.rows.map((player) =>
            player.accountId === result.result.accountId ? { ...player, tags: result.result.tags } : player,
          ),
        };
      }
      void loadPlayerStatistics(false);
    } catch (err) {
      error = message(err);
    } finally {
      playerTagBusy = { ...playerTagBusy, [accountId]: false };
    }
  }

  async function loadPlayerStatistics(showError = true) {
    playerStatisticsBusy = true;
    if (showError) error = "";
    try {
      playerStatistics = await api<DatabasePlayerStatisticsResponse>("/api/database/player-statistics");
    } catch (err) {
      if (showError) error = message(err);
    } finally {
      playerStatisticsBusy = false;
    }
  }

  async function loadEvents() {
    eventsBusy = true;
    error = "";
    try {
      const result = await api<EventsResponse>("/api/events?tail=120");
      events = result.events;
    } catch (err) {
      error = message(err);
    } finally {
      eventsBusy = false;
    }
  }

  async function loadStorage() {
    storageBusy = true;
    error = "";
    try {
      const result = await api<StorageResponse>("/api/storage");
      storageClaims = result.claims;
    } catch (err) {
      error = message(err);
    } finally {
      storageBusy = false;
    }
  }

  async function loadDatabaseMaintenance(showError = true) {
    databaseBusy = true;
    if (showError) error = "";
    try {
      databaseMaintenance = await api<DatabaseMaintenanceResponse>("/api/database-maintenance");
    } catch (err) {
      if (showError) error = message(err);
    } finally {
      databaseBusy = false;
    }
  }

  async function loadDatabaseWorldPartitions() {
    databaseTablesBusy = true;
    error = "";
    try {
      databaseWorldPartitions = await api<DatabaseWorldPartitionsResponse>("/api/database/world-partitions");
      syncPartitionLabelDrafts(databaseWorldPartitions.rows);
    } catch (err) {
      error = message(err);
    } finally {
      databaseTablesBusy = false;
    }
  }

  function syncPartitionLabelDrafts(rows: DatabaseWorldPartition[]) {
    const next = { ...partitionLabelDrafts };
    for (const row of rows) {
      if (next[row.partitionId] === undefined) next[row.partitionId] = row.label || "";
    }
    partitionLabelDrafts = next;
  }

  function updatePartitionLabelDraft(partitionId: number, value: string) {
    partitionLabelDrafts = { ...partitionLabelDrafts, [partitionId]: value };
  }

  async function saveWorldPartition(partition: DatabaseWorldPartition, blocked = partition.blocked) {
    partitionSaving = { ...partitionSaving, [partition.partitionId]: true };
    databaseNotice = "";
    error = "";
    try {
      const result = await api<DatabaseWorldPartitionUpdateResponse>(
        `/api/database/world-partitions/${partition.partitionId}`,
        {
          method: "PATCH",
          body: JSON.stringify({
            blocked,
            label: partitionLabelDrafts[partition.partitionId] || null,
          }),
        },
      );
      if (databaseWorldPartitions) {
        databaseWorldPartitions = {
          ...databaseWorldPartitions,
          rows: databaseWorldPartitions.rows.map((row) =>
            row.partitionId === result.row.partitionId ? result.row : row,
          ),
        };
      }
      partitionLabelDrafts = {
        ...partitionLabelDrafts,
        [result.row.partitionId]: result.row.label || "",
      };
      databaseNotice = `Updated ${result.row.map} partition #${result.row.partitionId}.`;
    } catch (err) {
      error = message(err);
    } finally {
      partitionSaving = { ...partitionSaving, [partition.partitionId]: false };
    }
  }

  async function createDatabaseBackup() {
    if (databaseMaintenance && !databaseMaintenance.backupsReady) {
      databaseNotice = databaseMaintenance.physicalBackupsEnabled
        ? databaseMaintenance.backupStorageMessage
        : databaseMaintenance.physicalBackupsMessage;
      return;
    }
    const label = battlegroup?.title || battlegroup?.name || "this server";
    const ok = window.confirm(`Create a manual database backup for ${label}?`);
    if (!ok) return;
    databaseActionBusy = true;
    databaseNotice = "";
    error = "";
    try {
      const created = await api<DatabaseMaintenanceItem>("/api/database-maintenance/backups", {
        method: "POST",
        body: JSON.stringify({ battleGroup: battlegroup?.name }),
      });
      databaseNotice = `Backup requested: ${created.name}`;
      await loadDatabaseMaintenance();
    } catch (err) {
      error = message(err);
    } finally {
      databaseActionBusy = false;
    }
  }

  async function enablePhysicalBackups() {
    const ok = window.confirm(
      "Enable physical database backups for this battlegroup? Kubernetes will reconcile the database deployment before manual backups can run.",
    );
    if (!ok) return;
    databaseActionBusy = true;
    databaseNotice = "";
    error = "";
    try {
      databaseMaintenance = await api<DatabaseMaintenanceResponse>(
        "/api/database-maintenance/physical-backups/enable",
        {
          method: "POST",
          body: JSON.stringify({ battleGroup: battlegroup?.name }),
        },
      );
      databaseNotice = databaseMaintenance.physicalBackupsEnabled
        ? databaseMaintenance.backupsReady
          ? "Physical backups are enabled. Wait for database reconciliation, then create a manual backup."
          : databaseMaintenance.backupStorageMessage
        : databaseMaintenance.physicalBackupsMessage;
    } catch (err) {
      error = message(err);
    } finally {
      databaseActionBusy = false;
    }
  }

  async function loadSettingsFile(file = selectedSettingsFile) {
    error = "";
    settingsNotice = "";
    selectedSettingsFile = file;
    try {
      settingsFile = await api<UserSettingsFile>(`/api/config/user-settings/${file}`);
      settingsDraft = settingsFile.content;
      settingsPreview = null;
      await loadSettingsBackups(file);
    } catch (err) {
      error = message(err);
    }
  }

  async function saveSettingsFile() {
    if (!settingsFile) return;
    settingsSaving = true;
    error = "";
    settingsNotice = "";
    try {
      const result = await api<UserSettingsUpdateResponse>(`/api/config/user-settings/${settingsFile.id}`, {
        method: "PUT",
        body: JSON.stringify({ content: settingsDraft }),
      });
      settingsFile = result.file;
      settingsDraft = result.file.content;
      settingsPreview = null;
      settingsNotice = result.restartRecommended
        ? "Saved. Restart the battlegroup for every runtime system to pick up the change."
        : "Saved.";
      await loadSettingsBackups(settingsFile.id);
    } catch (err) {
      error = message(err);
    } finally {
      settingsSaving = false;
    }
  }

  async function previewSettingsFile() {
    if (!settingsFile) return;
    settingsPreviewBusy = true;
    error = "";
    settingsNotice = "";
    try {
      settingsPreview = await api<UserSettingsPreviewResponse>(
        `/api/config/user-settings/${settingsFile.id}/preview`,
        {
          method: "POST",
          body: JSON.stringify({ content: settingsDraft }),
        },
      );
      if (!settingsPreview.changed) settingsNotice = "No changes compared to the live file.";
    } catch (err) {
      error = message(err);
    } finally {
      settingsPreviewBusy = false;
    }
  }

  async function loadSettingsBackups(file = selectedSettingsFile) {
    settingsBackupBusy = "load";
    try {
      settingsBackups = await api<UserSettingsBackupsResponse>(`/api/config/user-settings/${file}/backups`);
    } catch (err) {
      error = message(err);
    } finally {
      settingsBackupBusy = "";
    }
  }

  async function createSettingsBackup() {
    if (!settingsFile) return;
    settingsBackupBusy = "create";
    error = "";
    settingsNotice = "";
    try {
      const result = await api<UserSettingsBackupCreateResponse>(
        `/api/config/user-settings/${settingsFile.id}/backups`,
        { method: "POST" },
      );
      settingsNotice = `Backup created: ${result.backup.id}`;
      await loadSettingsBackups(settingsFile.id);
    } catch (err) {
      error = message(err);
    } finally {
      settingsBackupBusy = "";
    }
  }

  async function restoreSettingsBackup(backupId: string) {
    if (!settingsFile) return;
    const ok = window.confirm("Restore this settings backup? Current contents will be backed up first.");
    if (!ok) return;
    settingsBackupBusy = backupId;
    error = "";
    settingsNotice = "";
    try {
      const result = await api<UserSettingsRestoreResponse>(
        `/api/config/user-settings/${settingsFile.id}/backups/${encodeURIComponent(backupId)}/restore`,
        { method: "POST" },
      );
      settingsFile = result.file;
      settingsDraft = result.file.content;
      settingsPreview = null;
      settingsNotice = result.restartRecommended
        ? "Backup restored. Restart the battlegroup for every runtime system to pick up the change."
        : "Backup restored.";
      await loadSettingsBackups(settingsFile.id);
    } catch (err) {
      error = message(err);
    } finally {
      settingsBackupBusy = "";
    }
  }

  async function loadManagerSelf() {
    managerBusy = "self";
    error = "";
    try {
      managerSelf = await api<ManagerSelf>("/api/manager/self");
    } catch (err) {
      error = message(err);
    } finally {
      managerBusy = "";
    }
  }

  async function loadManagerLogs() {
    managerBusy = "logs";
    error = "";
    try {
      managerLogs = await api<ManagerLogResponse>("/api/manager/logs?tail=180");
    } catch (err) {
      error = message(err);
    } finally {
      managerBusy = "";
    }
  }

  function updateSettingsEntry(line: number, value: string) {
    const lines = settingsDraft.split(/\r?\n/);
    const index = line - 1;
    const current = lines[index] ?? "";
    const match = current.match(/^(\s*[^=]+?)(\s*=\s*)(.*)$/);
    if (!match) return;
    lines[index] = `${match[1]}${match[2]}${value}`;
    settingsDraft = lines.join("\n");
    settingsPreview = null;
  }

  function markSettingsDraftChanged() {
    settingsPreview = null;
  }

  function settingFieldKind(value: string) {
    const text = value.trim();
    if (/^(true|false)$/i.test(text)) return "boolean";
    if (/^-?\d+(\.\d+)?$/.test(text)) return "number";
    return "text";
  }

  function settingDisplayName(key: string) {
    return key
      .replace(/^m_/, "")
      .replace(/_/g, " ")
      .replace(/([a-z])([A-Z])/g, "$1 $2")
      .trim();
  }

  function parseIniSections(content: string): IniSection[] {
    const sections: IniSection[] = [];
    let current: IniSection = { name: "Global", entries: [] };
    content.split(/\r?\n/).forEach((line, index) => {
      const trimmed = line.trim();
      if (trimmed.startsWith("[") && trimmed.endsWith("]") && trimmed.length > 2) {
        if (current.entries.length || current.name !== "Global") sections.push(current);
        current = { name: trimmed.slice(1, -1).trim(), entries: [] };
        return;
      }
      if (!trimmed || trimmed.startsWith(";") || trimmed.startsWith("#")) return;
      const splitAt = trimmed.indexOf("=");
      if (splitAt < 0) return;
      current.entries.push({
        key: trimmed.slice(0, splitAt).trim().replace(/^\+/, ""),
        value: trimmed.slice(splitAt + 1).trim(),
        line: index + 1,
      });
    });
    if (current.entries.length || current.name !== "Global") sections.push(current);
    return sections;
  }

  function filterIniSections(sections: IniSection[], filter: string): IniSection[] {
    const text = filter.trim().toLowerCase();
    if (!text) return sections;
    return sections
      .map((section) => ({
        ...section,
        entries: section.entries.filter(
          (entry) =>
            section.name.toLowerCase().includes(text) ||
            entry.key.toLowerCase().includes(text) ||
            entry.value.toLowerCase().includes(text),
        ),
      }))
      .filter((section) => section.entries.length);
  }

  function filterPods(items: typeof pods, filter: string) {
    const text = filter.trim().toLowerCase();
    if (!text) return items;
    return items.filter(
      (pod) =>
        pod.name.toLowerCase().includes(text) ||
        pod.phase.toLowerCase().includes(text) ||
        pod.containers.some((container) => container.toLowerCase().includes(text)),
    );
  }

  function filterServices(items: typeof services, filter: string) {
    const text = filter.trim().toLowerCase();
    if (!text) return items;
    return items.filter(
      (service) =>
        service.name.toLowerCase().includes(text) ||
        (service.serviceType || "").toLowerCase().includes(text) ||
        service.ports.some((port) => `${port.port} ${port.nodePort || ""} ${port.protocol || ""}`.toLowerCase().includes(text)),
    );
  }

  function filterEvents(items: EventSummary[], filter: string) {
    const text = filter.trim().toLowerCase();
    if (!text) return items;
    return items.filter((event) =>
      [event.eventType, event.reason, event.message, event.involvedKind, event.involvedName]
        .join(" ")
        .toLowerCase()
        .includes(text),
    );
  }

  function filterStorageClaims(items: PersistentVolumeClaimSummary[], filter: string) {
    const text = filter.trim().toLowerCase();
    if (!text) return items;
    return items.filter((claim) =>
      [claim.name, claim.phase, claim.requestedStorage, claim.capacityStorage, claim.storageClass, claim.volumeName]
        .join(" ")
        .toLowerCase()
        .includes(text),
    );
  }

  function databaseMaintenanceItems(value: DatabaseMaintenanceResponse | null): DatabaseMaintenanceItem[] {
    if (!value) return [];
    return [
      ...value.schedules,
      ...value.backups,
      ...value.restores,
      ...value.migrations,
      ...value.operations,
    ];
  }

  function filterDatabaseMaintenance(items: DatabaseMaintenanceItem[], filter: string) {
    const text = filter.trim().toLowerCase();
    if (!text) return items;
    return items.filter((item) =>
      [
        item.name,
        item.kind,
        item.phase,
        item.battleGroup,
        item.identifier,
        item.schedule,
        item.backup,
        item.action,
        item.originator,
        item.latestEvent?.reason,
        item.latestEvent?.message,
      ]
        .join(" ")
        .toLowerCase()
        .includes(text),
    );
  }

  function filterWorldPartitions(items: DatabaseWorldPartition[], filter: string) {
    const text = filter.trim().toLowerCase();
    if (!text) return items;
    return items.filter((item) =>
      [item.partitionId, item.serverId, item.map, item.dimensionIndex, item.blocked, item.label]
        .join(" ")
        .toLowerCase()
        .includes(text),
    );
  }

  function playerActivityRows(lists: DirectorPlayerLists | null): PlayerActivityRow[] {
    if (!lists) return [];
    const bucketMap = [
      ["Online", lists.online],
      ["In transit", lists.inTransit],
      ["Grace", lists.gracePeriod],
      ["Completion", lists.completion],
      ["Queued", lists.queued],
      ["Active", lists.all],
    ] as const;
    const ids = new Set<string>();
    bucketMap.forEach(([, values]) => values.forEach((id) => ids.add(id)));
    return [...ids]
      .map((id) => {
        const buckets = bucketMap.filter(([, values]) => values.includes(id)).map(([name]) => name);
        const status =
          buckets.find((bucket) => bucket !== "Active") ?? (buckets.includes("Active") ? "Active" : "Observed");
        return { id, status, buckets };
      })
      .sort((left, right) => left.status.localeCompare(right.status) || left.id.localeCompare(right.id));
  }

  function filterPlayerRows(items: PlayerActivityRow[], filter: string) {
    const text = filter.trim().toLowerCase();
    if (!text) return items;
    return items.filter((item) => [item.id, item.status, ...item.buckets].join(" ").toLowerCase().includes(text));
  }

  function filterDatabasePlayers(items: DatabasePlayerSummary[], filter: string) {
    const text = filter.trim().toLowerCase();
    if (!text) return items;
    return items.filter((item) =>
      [
        item.accountId,
        item.characterName,
        item.onlineStatus,
        item.lifeState,
        item.serverId,
        item.previousServerPartitionId,
        item.guildName,
        ...(item.tags || []),
      ]
        .join(" ")
        .toLowerCase()
        .includes(text),
    );
  }

  function servicePorts(service: (typeof services)[number]) {
    return service.ports
      .map((port) => {
        const node = port.nodePort ? `:${port.nodePort}` : "";
        return `${port.protocol || "TCP"} ${port.port}${node}`;
      })
      .join(", ");
  }

  function resourceLine(pod: (typeof pods)[number]) {
    const resources = pod.containerResources || [];
    const memory = resources.map((item) => item.memoryLimit || item.memoryRequest).filter(Boolean);
    const cpu = resources.map((item) => item.cpuLimit || item.cpuRequest).filter(Boolean);
    const parts = [];
    if (memory.length) parts.push(`mem ${memory.join(" + ")}`);
    if (cpu.length) parts.push(`cpu ${cpu.join(" + ")}`);
    return parts.join(" · ");
  }

  function imageLine(pod: (typeof pods)[number]) {
    return (pod.containerResources || [])
      .map((item) => item.image)
      .filter(Boolean)
      .map((image) => image?.split("/").pop() || image)
      .join(", ");
  }

  async function loadDirectorConfig() {
    directorBusy = true;
    directorNotice = "";
    error = "";
    try {
      const [fls, transfer] = await Promise.all([
        api<unknown>("/api/director/config/fls"),
        api<unknown>("/api/director/config/character-transfer"),
      ]);
      directorFlsDraft = formatJson(fls);
      directorTransferDraft = formatJson(transfer);
      if (!directorCapabilities) await loadDirectorCapabilities(false);
      const mapName = selectedDirectorMap || overview?.maps[0]?.name;
      if (mapName) await loadDirectorMapOverride(mapName, false);
    } catch (err) {
      error = message(err);
    } finally {
      directorBusy = false;
    }
  }

  async function loadDirectorCapabilities(manageBusy = true) {
    if (manageBusy) directorApiBusy = true;
    error = "";
    try {
      directorCapabilities = await api<DirectorCapabilities>("/api/director/capabilities");
      if (!directorApiSelection) {
        const preferred =
          directorCapabilities.apiPaths.find((item) => item.method === "GET" && item.path === "/v0/players/online") ??
          directorCapabilities.apiPaths.find((item) => item.method === "GET") ??
          directorCapabilities.apiPaths[0];
        if (preferred) directorApiSelection = capabilityKey(preferred);
      }
    } catch (err) {
      error = message(err);
    } finally {
      if (manageBusy) directorApiBusy = false;
    }
  }

  async function runDirectorApiCall() {
    const capability = selectedDirectorCapability();
    if (!capability) return;
    if (capability.method !== "GET" && !window.confirm("Run this Director write operation?")) return;
    directorApiBusy = true;
    directorApiResult = "";
    error = "";
    try {
      const proxyPath = `/api/director${capability.path}`;
      const init: RequestInit =
        capability.method === "GET"
          ? {}
          : {
              method: capability.method,
              body: JSON.stringify(parseJsonDraft(directorApiBody || "{}")),
            };
      directorApiResult = formatJson(await api<unknown>(proxyPath, init));
    } catch (err) {
      error = message(err);
    } finally {
      directorApiBusy = false;
    }
  }

  async function saveDirectorConfig(kind: "fls" | "transfer") {
    directorBusy = true;
    directorNotice = "";
    error = "";
    try {
      const path =
        kind === "fls" ? "/api/director/config/fls" : "/api/director/config/character-transfer";
      const draft = kind === "fls" ? directorFlsDraft : directorTransferDraft;
      await api(path, {
        method: "POST",
        body: JSON.stringify(parseJsonDraft(draft)),
      });
      directorNotice = "Director override saved.";
      await loadDirectorConfig();
    } catch (err) {
      error = message(err);
    } finally {
      directorBusy = false;
    }
  }

  async function clearDirectorConfig(kind: "fls" | "transfer") {
    directorBusy = true;
    directorNotice = "";
    error = "";
    try {
      const path =
        kind === "fls" ? "/api/director/config/fls" : "/api/director/config/character-transfer";
      await api(path, { method: "DELETE" });
      directorNotice = "Director override cleared.";
      await loadDirectorConfig();
    } catch (err) {
      error = message(err);
    } finally {
      directorBusy = false;
    }
  }

  async function loadDirectorMapOverride(mapName = selectedDirectorMap, manageBusy = true) {
    if (!mapName) return;
    if (manageBusy) {
      directorBusy = true;
      directorNotice = "";
      error = "";
    }
    selectedDirectorMap = mapName;
    try {
      directorMapDetail = await api<DirectorMapConfigDetail>(
        `/api/director/config/maps/${encodeURIComponent(mapName)}/override`,
      );
      directorMapDraft = formatJson(directorMapDetail.updatePayloadTemplate);
    } catch (err) {
      error = message(err);
    } finally {
      if (manageBusy) directorBusy = false;
    }
  }

  async function saveDirectorMapOverride() {
    if (!directorMapDetail) return;
    directorBusy = true;
    directorNotice = "";
    error = "";
    try {
      await api(`/api/director/config/maps/${encodeURIComponent(directorMapDetail.name)}/override`, {
        method: "POST",
        body: JSON.stringify(parseJsonDraft(directorMapDraft)),
      });
      directorNotice = "Map override saved.";
      await loadDirectorMapOverride(directorMapDetail.name, false);
      await refresh(false);
    } catch (err) {
      error = message(err);
    } finally {
      directorBusy = false;
    }
  }

  async function clearDirectorMapOverride() {
    if (!directorMapDetail) return;
    directorBusy = true;
    directorNotice = "";
    error = "";
    try {
      await api(`/api/director/config/maps/${encodeURIComponent(directorMapDetail.name)}/override`, {
        method: "DELETE",
      });
      directorNotice = "Map override cleared.";
      await loadDirectorMapOverride(directorMapDetail.name, false);
      await refresh(false);
    } catch (err) {
      error = message(err);
    } finally {
      directorBusy = false;
    }
  }

  function estimateLayoutMemory(value: WorldLayout) {
    const hagga = Math.max(1, Number(value.haggaBasinInstances) || 1);
    const deepDesert = Math.min(1, Math.max(0, Number(value.deepDesertPveInstances) || 0) + Math.max(0, Number(value.deepDesertPvpInstances) || 0));
    const social = value.socialHubsEnabled || deepDesert > 0;
    const lines = [`${hagga} Hagga Basin ${hagga === 1 ? "instance" : "instances"} x 20 GB`];
    if (social) lines.push("Social Hubs x 10 GB");
    if (deepDesert) lines.push("Deep Desert x 10 GB");
    return {
      gb: hagga * 20 + (social ? 10 : 0) + deepDesert * 10,
      lines,
    };
  }

  function message(err: unknown) {
    return err instanceof Error ? err.message : "Operation failed.";
  }

  function formatJson(value: unknown) {
    return JSON.stringify(value, null, 2);
  }

  function parseJsonDraft(value: string) {
    try {
      return JSON.parse(value);
    } catch {
      throw new Error("JSON is not valid.");
    }
  }

  function jsonPrimitiveFields(draft: string): JsonField[] {
    if (!draft.trim()) return [];
    try {
      const root = JSON.parse(draft);
      const fields: JsonField[] = [];
      collectJsonFields(root, "", [], fields);
      return fields;
    } catch {
      return [];
    }
  }

  function collectJsonFields(value: unknown, pointer: string, labels: string[], fields: JsonField[]) {
    if (fields.length >= 160) return;
    if (typeof value === "boolean" || typeof value === "number" || typeof value === "string" || value === null) {
      fields.push({
        pointer,
        label: labels.length ? labels.join(" / ") : "Root",
        value: value === null ? "null" : String(value),
        kind: value === null ? "null" : typeof value,
      });
      return;
    }
    if (Array.isArray(value)) {
      value.forEach((item, index) => collectJsonFields(item, `${pointer}/${index}`, [...labels, `[${index}]`], fields));
      return;
    }
    if (value && typeof value === "object") {
      Object.entries(value as Record<string, unknown>).forEach(([key, item]) =>
        collectJsonFields(item, `${pointer}/${escapeJsonPointer(key)}`, [...labels, settingDisplayName(key)], fields),
      );
    }
  }

  function escapeJsonPointer(value: string) {
    return value.replace(/~/g, "~0").replace(/\//g, "~1");
  }

  function unescapeJsonPointer(value: string) {
    return value.replace(/~1/g, "/").replace(/~0/g, "~");
  }

  function updateDirectorJsonDraft(kind: "fls" | "transfer" | "map", pointer: string, value: string, fieldKind: JsonField["kind"]) {
    const draft = kind === "fls" ? directorFlsDraft : kind === "transfer" ? directorTransferDraft : directorMapDraft;
    const root = parseJsonDraft(draft);
    setJsonPointerValue(root, pointer, coerceJsonFieldValue(value, fieldKind));
    const next = formatJson(root);
    if (kind === "fls") directorFlsDraft = next;
    if (kind === "transfer") directorTransferDraft = next;
    if (kind === "map") directorMapDraft = next;
  }

  function setJsonPointerValue(root: unknown, pointer: string, value: unknown) {
    if (!pointer) return;
    const parts = pointer.split("/").slice(1).map(unescapeJsonPointer);
    let current = root as Record<string, unknown> | unknown[];
    parts.slice(0, -1).forEach((part) => {
      current = Array.isArray(current) ? (current[Number(part)] as Record<string, unknown> | unknown[]) : (current[part] as Record<string, unknown> | unknown[]);
    });
    const last = parts[parts.length - 1];
    if (Array.isArray(current)) current[Number(last)] = value;
    else current[last] = value;
  }

  function coerceJsonFieldValue(value: string, fieldKind: JsonField["kind"]) {
    if (fieldKind === "boolean") return value === "true";
    if (fieldKind === "number") {
      const parsed = Number(value);
      if (Number.isNaN(parsed)) throw new Error("Number field is invalid.");
      return parsed;
    }
    if (fieldKind === "null" && value.trim().toLowerCase() === "null") return null;
    return value;
  }

  function formatBackupTime(value?: string) {
    if (!value) return "Unknown time";
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }

  function formatEventTime(value?: string) {
    if (!value) return "No timestamp";
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleTimeString();
  }

  function formatDuration(totalSeconds: number) {
    const seconds = Math.max(0, Math.floor(totalSeconds));
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const rest = seconds % 60;
    return `${hours}h ${minutes}m ${rest}s`;
  }

  function capabilityKey(value: DirectorPathCapability) {
    return `${value.method} ${value.path}`;
  }

  function selectedDirectorCapability() {
    return directorCapabilities?.apiPaths.find((item) => capabilityKey(item) === directorApiSelection) ?? null;
  }

  function deriveServerHealth(
    overviewValue: Overview | null,
    battlegroupValue: BattlegroupSummary | null,
    notReady: PodSummary[],
  ) {
    if (!overviewValue) {
      return { label: "Loading", tone: "neutral", summary: "Waiting for the manager API." };
    }
    if (!battlegroupValue) {
      return { label: "No server", tone: "danger", summary: "No battlegroup is available in this namespace." };
    }
    if (battlegroupValue.stop) {
      return { label: "Stopped", tone: "danger", summary: "The battlegroup is intentionally stopped." };
    }
    if (notReady.length > 0) {
      return {
        label: "Needs attention",
        tone: "warn",
        summary: `${notReady.length} workload${notReady.length === 1 ? " is" : "s are"} not ready.`,
      };
    }
    if (!overviewValue.directorAvailable) {
      return { label: "Director offline", tone: "warn", summary: "Game services may be up, but Director telemetry is unavailable." };
    }
    return { label: "Healthy", tone: "good", summary: "Game services, Director, and workloads look ready." };
  }

  function deriveNextActions(
    battlegroupValue: BattlegroupSummary | null,
    overviewValue: Overview | null,
    databaseValue: DatabaseMaintenanceResponse | null,
    busyAction: string,
  ) {
    const actions: Array<{ label: string; hint: string; page?: Page; action?: "start" | "stop" | "restart"; danger?: boolean; disabled?: boolean }> = [];
    if (!battlegroupValue) {
      actions.push({ label: "Refresh", hint: "Look for a battlegroup", disabled: false });
      return actions;
    }
    if (battlegroupValue.stop) {
      actions.push({ label: "Start server", hint: "Bring the world online", action: "start", disabled: !!busyAction });
    } else {
      actions.push({ label: "Manage players", hint: `${overviewValue?.players?.active ?? 0} active now`, page: "players" });
      actions.push({ label: "Edit game settings", hint: "INI-backed runtime settings", page: "config" });
      actions.push({ label: "Restart server", hint: "Apply pending runtime changes", action: "restart", disabled: !!busyAction });
      actions.push({ label: "Stop server", hint: "Disconnects online players", action: "stop", danger: true, disabled: !!busyAction });
    }
    actions.push({
      label: databaseValue?.backupsReady ? "Create backup" : "Check backups",
      hint: databaseValue
        ? databaseValue.backupsReady
          ? "Manual database backup is available"
          : "Backup storage needs attention"
        : "Load backup readiness",
      page: "database",
    });
    return actions.slice(0, 5);
  }

  function mapHealthLabel(map: { servers: Array<{ status: string }> }) {
    if (!map.servers.length) return "No servers";
    const running = map.servers.filter((server) => server.status === "Running").length;
    return running === map.servers.length ? "Running" : `${running}/${map.servers.length} running`;
  }

  function selectDashboardMaps(overviewValue: Overview | null) {
    const maps = overviewValue?.maps ?? [];
    const relevant = maps.filter((map) => map.players > 0 || map.servers.some((server) => server.status === "Running"));
    return (relevant.length ? relevant : maps).slice(0, 8);
  }
</script>

{#if loading}
  <main class="boot">Opening the control room...</main>
{:else if !session}
  <main class="login">
    <section class="login-panel">
      <p class="eyebrow">Dune Dedicated Server Manager</p>
      <h1>Sign in</h1>
      <p>Use your Self-Host Service Token to manage this server.</p>
      <form on:submit|preventDefault={signIn}>
        <input class="sr-only" type="text" autocomplete="username" value="self-host-token" tabindex="-1" aria-hidden="true" />
        <label>
          Self-Host Service Token
          <input bind:value={token} type="password" autocomplete="current-password" placeholder="Paste token" />
        </label>
        <button disabled={signingIn || !token.trim()}>{signingIn ? "Signing in..." : "Sign in"}</button>
      </form>
      {#if error}<p class="error">{error}</p>{/if}
    </section>
  </main>
{:else}
  <main class="shell">
    <aside>
      <div class="brand">
        <strong>Dune Manager</strong>
        <span>{session.namespace}</span>
      </div>
      {#each navItems as item}
        <button class:active={page === item.page} on:click={() => openPage(item.page)}>
          {item.label}
        </button>
      {/each}
      <button class="ghost" on:click={logout}>Sign out</button>
    </aside>
    <section class="content">
      <header>
        <div>
          <p class="eyebrow">Manager API {session.apiVersion}</p>
          <h1>{battlegroup?.title || "Server Manager"}</h1>
        </div>
        <button on:click={() => refresh()}>Refresh</button>
      </header>
      {#if uiErrors.length}
        <section class="error">
          <div class="split-heading">
            <strong>UI Error</strong>
            <button class="ghost inline" on:click={() => (uiErrors = [])}>Dismiss</button>
          </div>
          {#each uiErrors as item}<p>{item}</p>{/each}
        </section>
      {/if}
      {#if error}<p class="error">{error}</p>{/if}

      {#if page === "dashboard"}
        <section class="dashboard-hero">
          <div class="hero-copy">
            <p class="eyebrow">Server health</p>
            <h2>{serverHealth.label}</h2>
            <p>{serverHealth.summary}</p>
          </div>
          <div class:good={serverHealth.tone === "good"} class:warning={serverHealth.tone === "warn"} class:danger-state={serverHealth.tone === "danger"} class="health-badge">
            <span>BattleGroup {battlegroup?.phase || "Unknown"}</span>
            <strong>{battlegroup?.stop ? "Offline" : "Online"}</strong>
          </div>
        </section>
        <div class="grid dashboard-metrics">
          <Card label="Players online" value={`${overview?.players?.online ?? 0}`} />
          <Card label="Queued" value={`${overview?.players?.queued ?? 0}`} />
          <Card label="Maps online" value={`${onlineMaps}/${overview?.maps.length ?? 0}`} />
          <Card label="Workloads ready" value={`${runningPods}/${pods.length}`} />
        </div>
        <section class="dashboard-grid">
          <section class="panel action-panel">
            <div class="split-heading">
              <div>
                <h2>Next Actions</h2>
                <p class="muted">Common server-operator tasks for the current state.</p>
              </div>
              <b class:good={telemetryConnected}>{telemetryConnected ? "Live" : "Polling"}</b>
            </div>
            <div class="quick-actions">
              {#each nextActions as action}
                <button
                  class:danger={action.danger}
                  disabled={action.disabled}
                  on:click={() => action.action ? lifecycle(action.action) : action.page ? openPage(action.page) : refresh()}
                >
                  <strong>{action.label}</strong>
                  <span>{action.hint}</span>
                </button>
              {/each}
            </div>
            {#if telemetryError}<p class="warn">{telemetryError}</p>{/if}
          </section>
          <section class="panel player-panel">
            <div class="split-heading">
              <div>
                <h2>Players</h2>
                <p class="muted">Current Director player buckets.</p>
              </div>
              <button class="inline" on:click={() => openPage("players")}>Open</button>
            </div>
            <div class="player-summary">
              <div><span>Active</span><b>{overview?.players?.active ?? 0}</b></div>
              <div><span>Online</span><b>{overview?.players?.online ?? 0}</b></div>
              <div><span>Traveling</span><b>{overview?.players?.inTransit ?? 0}</b></div>
              <div><span>Queued</span><b>{overview?.players?.queued ?? 0}</b></div>
              <div><span>Characters</span><b>{playerStatistics?.statistics.totalPlayers ?? "..."}</b></div>
              <div><span>Guilds</span><b>{playerStatistics?.statistics.guilds ?? "..."}</b></div>
            </div>
          </section>
        </section>
        <section class="dashboard-grid lower">
          <section class="panel">
            <div class="split-heading">
              <div>
                <h2>Map Population</h2>
                <p class="muted">Player load and shard status from Director.</p>
              </div>
              <button class="inline" on:click={() => openPage("director")}>Rules</button>
            </div>
            <div class="map-list">
              {#each dashboardMaps as map}
                <article>
                  <div>
                    <strong>{map.name}</strong>
                    <span>{map.kind} · {mapHealthLabel(map)}</span>
                  </div>
                  <b>{map.players} players</b>
                </article>
              {/each}
            </div>
            {#if (overview?.maps.length ?? 0) > dashboardMaps.length}
              <p class="muted map-note">Showing {dashboardMaps.length} of {overview?.maps.length ?? 0} maps. Open Director Rules for the full list.</p>
            {/if}
          </section>
          <section class="panel">
            <div class="split-heading">
              <div>
                <h2>Backups</h2>
                <p class="muted">Database protection state for this battlegroup.</p>
              </div>
              <button class="inline" on:click={() => openPage("database")}>Open</button>
            </div>
            {#if databaseMaintenance}
              <div class="rows compact">
                <div class="row"><span>Physical backups</span><b class:good={databaseMaintenance.physicalBackupsEnabled}>{databaseMaintenance.physicalBackupsEnabled ? "Enabled" : "Disabled"}</b></div>
                <div class="row"><span>Backup storage</span><b class:good={databaseMaintenance.backupStorageConfigured}>{databaseMaintenance.backupStorageConfigured ? "Configured" : "Missing"}</b></div>
                <div class="row"><span>Backup runs</span><b>{databaseMaintenance.backups.length}</b></div>
              </div>
              {#if !databaseMaintenance.backupsReady}<p class="warn">{databaseMaintenance.physicalBackupsEnabled ? databaseMaintenance.backupStorageMessage : databaseMaintenance.physicalBackupsMessage}</p>{/if}
            {:else}
              <p class="muted">Backup readiness is loading.</p>
            {/if}
          </section>
        </section>
      {:else if page === "workloads"}
        <section class="panel form">
          <div class="split-heading">
            <div>
              <h2>Workloads</h2>
              <p class="muted">Inspect Kubernetes pods and services in the managed namespace.</p>
            </div>
            <input bind:value={workloadFilter} placeholder="Filter workloads" />
          </div>
          <div class="workload-grid">
            <section>
              <h3>Pods</h3>
              <div class="workload-list">
                {#each visiblePods as pod}
                  <article class="workload-card">
                    <div>
                      <strong>{pod.name}</strong>
                      <span>{pod.phase} · {pod.containers.join(", ")}</span>
                      {#if resourceLine(pod)}<em>{resourceLine(pod)}</em>{/if}
                      {#if imageLine(pod)}<small>{imageLine(pod)}</small>{/if}
                    </div>
                    <div class="workload-meta">
                      <b class:good={pod.ready}>{pod.ready ? "Ready" : "Not ready"}</b>
                      <span>{pod.restarts} restarts</span>
                      <button on:click={() => openPodLogs(pod.name, pod.containers[0] || "")}>Logs</button>
                    </div>
                  </article>
                {/each}
              </div>
            </section>
            <section>
              <h3>Services</h3>
              <div class="workload-list">
                {#each visibleServices as service}
                  <article class="workload-card">
                    <div>
                      <strong>{service.name}</strong>
                      <span>{service.serviceType || "Service"} · {service.clusterIp || "no cluster IP"}</span>
                    </div>
                    <div class="workload-meta">
                      <b>{servicePorts(service) || "No ports"}</b>
                    </div>
                  </article>
                {/each}
              </div>
            </section>
          </div>
          <section class="events-panel">
            <div class="editor-title">
              <div>
                <h3>Cluster Events</h3>
                <p class="muted">Recent Kubernetes scheduling, readiness, image, and warning events for this server.</p>
              </div>
              <button disabled={eventsBusy} on:click={loadEvents}>
                {eventsBusy ? "Loading..." : events.length ? "Refresh events" : "Load events"}
              </button>
            </div>
            {#if visibleEvents.length}
              <div class="event-list">
                {#each visibleEvents.slice(0, 80) as event}
                  <article class:warning={event.eventType === "Warning"}>
                    <div>
                      <strong>{event.reason || event.eventType || "Event"}</strong>
                      <span>{event.involvedKind}/{event.involvedName} · {formatEventTime(event.lastSeen || event.firstSeen)}</span>
                    </div>
                    <p>{event.message}</p>
                    {#if event.count > 1}<b>{event.count}x</b>{/if}
                  </article>
                {/each}
              </div>
            {:else}
              <p class="muted">Load events to inspect recent cluster activity. The workload filter also applies here.</p>
            {/if}
          </section>
        </section>
      {:else if page === "storage"}
        <section class="panel form">
          <div class="split-heading">
            <div>
              <h2>Storage</h2>
              <p class="muted">Inspect persistent volume claims used by database, queues, and runtime services.</p>
            </div>
            <div class="actions">
              <input bind:value={storageFilter} placeholder="Filter claims" />
              <button disabled={storageBusy} on:click={loadStorage}>
                {storageBusy ? "Loading..." : storageClaims.length ? "Refresh" : "Load storage"}
              </button>
            </div>
          </div>
          {#if visibleStorageClaims.length}
            <div class="storage-grid">
              {#each visibleStorageClaims as claim}
                <article class="storage-card">
                  <div>
                    <strong>{claim.name}</strong>
                    <span>{claim.storageClass || "default storage"} · {claim.accessModes.join(", ") || "no access mode"}</span>
                  </div>
                  <div class="storage-meter">
                    <b class:good={claim.phase === "Bound"}>{claim.phase || "Unknown"}</b>
                    <span>{claim.capacityStorage || claim.requestedStorage || "unknown size"}</span>
                  </div>
                  <div class="storage-detail">
                    <span>Requested</span><b>{claim.requestedStorage || "unknown"}</b>
                    <span>Volume</span><b>{claim.volumeName || "not bound"}</b>
                    <span>Created</span><b>{formatBackupTime(claim.createdAt)}</b>
                  </div>
                </article>
              {/each}
            </div>
          {:else}
            <p class="muted">Load storage to inspect PVC capacity and binding state. The filter applies after loading.</p>
          {/if}
        </section>
      {:else if page === "database"}
        <section class="panel form">
          <div class="split-heading">
            <div>
              <h2>Backups and Database</h2>
              <p class="muted">Manage backup readiness and inspect approved game database data without arbitrary table editing.</p>
            </div>
            <div class="actions">
              <input bind:value={databaseFilter} placeholder="Filter backups or partitions" />
              {#if databaseMaintenance && !databaseMaintenance.physicalBackupsEnabled}
                <button disabled={databaseActionBusy || !battlegroup} on:click={enablePhysicalBackups}>
                  {databaseActionBusy ? "Enabling..." : "Enable backups"}
                </button>
              {/if}
              <button
                disabled={databaseActionBusy || !battlegroup || !databaseMaintenance || !databaseMaintenance.backupsReady}
                on:click={createDatabaseBackup}
              >
                {databaseActionBusy ? "Requesting..." : "Create backup"}
              </button>
              <button disabled={databaseBusy} on:click={loadDatabaseMaintenance}>
                {databaseBusy ? "Loading..." : databaseMaintenance ? "Refresh" : "Load database"}
              </button>
            </div>
          </div>
          {#if databaseMaintenance && !databaseMaintenance.physicalBackupsEnabled}
            <p class="warn">{databaseMaintenance.physicalBackupsMessage}</p>
          {/if}
          {#if databaseMaintenance && databaseMaintenance.physicalBackupsEnabled && !databaseMaintenance.backupStorageConfigured}
            <p class="warn">{databaseMaintenance.backupStorageMessage}</p>
          {/if}
          {#if databaseNotice}<p class="notice">{databaseNotice}</p>{/if}
          {#if databaseMaintenance}
            <div class="database-ribbon">
              <Card label="Schedules" value={`${databaseMaintenance.schedules.length}`} />
              <Card label="Backups" value={`${databaseMaintenance.backups.length}`} />
              <Card label="Restores" value={`${databaseMaintenance.restores.length}`} />
              <Card label="Operations" value={`${databaseMaintenance.operations.length + databaseMaintenance.migrations.length}`} />
            </div>
          {/if}
          <section class="controlled-db-panel">
            <div class="editor-title">
              <div>
                <h3>World partitions</h3>
                <p class="muted">Controlled access to partition labels and open/blocked state. Raw SQL is never exposed.</p>
              </div>
              <button disabled={databaseTablesBusy} on:click={loadDatabaseWorldPartitions}>
                {databaseTablesBusy ? "Loading..." : databaseWorldPartitions ? "Refresh" : "Load"}
              </button>
            </div>
            {#if visibleWorldPartitions.length}
              <div class="partition-list">
                {#each visibleWorldPartitions as partition}
                  <article>
                    <div>
                      <strong>{partition.map}</strong>
                      <span>{partition.label || "No label"} · dimension {partition.dimensionIndex}</span>
                    </div>
                    <div class="partition-meta">
                      <b>#{partition.partitionId}</b>
                      <span>{partition.serverId || "No server"}</span>
                      <em class:warning={partition.blocked}>{partition.blocked ? "Blocked" : "Open"}</em>
                    </div>
                    <div class="partition-controls">
                      <input
                        value={partitionLabelDrafts[partition.partitionId] ?? partition.label ?? ""}
                        maxlength="80"
                        placeholder="Operator label"
                        on:input={(event) =>
                          updatePartitionLabelDraft(partition.partitionId, event.currentTarget.value)}
                      />
                      <button
                        class="inline"
                        disabled={partitionSaving[partition.partitionId]}
                        on:click={() => saveWorldPartition(partition)}
                      >
                        {partitionSaving[partition.partitionId] ? "Saving..." : "Save label"}
                      </button>
                      <button
                        class:danger={!partition.blocked}
                        class="inline"
                        disabled={partitionSaving[partition.partitionId]}
                        on:click={() => saveWorldPartition(partition, !partition.blocked)}
                      >
                        {partition.blocked ? "Open partition" : "Block partition"}
                      </button>
                    </div>
                  </article>
                {/each}
              </div>
            {:else if databaseWorldPartitions}
              <p class="muted">No world partitions match the current filter.</p>
            {:else}
              <p class="muted">Load world partitions to inspect the game database through the controlled Manager API query.</p>
            {/if}
          </section>
          {#if visibleDatabaseItems.length}
            <div class="database-list">
              {#each visibleDatabaseItems as item}
                <article class="database-card">
                  <div>
                    <span>{item.kind.replace("Database", "")}</span>
                    <strong>{item.name}</strong>
                    <small>{item.battleGroup || "No battlegroup"}{item.originator ? ` · ${item.originator}` : ""}</small>
                  </div>
                  <div class="database-fields">
                    <span>Phase</span><b class:good={item.phase === "Completed" || item.phase === "Ready"}>{item.phase || (item.suspended ? "Suspended" : "Observed")}</b>
                    {#if item.schedule}<span>Schedule</span><b>{item.schedule}</b>{/if}
                    {#if item.identifier}<span>Identifier</span><b>{item.identifier}</b>{/if}
                    {#if item.backup}<span>Backup</span><b>{item.backup}</b>{/if}
                    {#if item.action}<span>Action</span><b>{item.action}</b>{/if}
                    <span>Time</span><b>{formatBackupTime(item.finishTime || item.startTime || item.createdAt)}</b>
                    {#if item.duration}<span>Duration</span><b>{item.duration}</b>{/if}
                  </div>
                  {#if item.latestEvent}
                    <p class:warning={item.latestEvent.eventType === "Warning"} class="database-event">
                      <strong>{item.latestEvent.reason || item.latestEvent.eventType}</strong>
                      {item.latestEvent.message}
                    </p>
                  {/if}
                </article>
              {/each}
            </div>
          {:else}
            <p class="muted">
              {databaseMaintenance
                ? "No backup, restore, migration, or operation resources match the current filter."
                : "Load database maintenance to inspect operator-managed backup and restore resources."}
            </p>
          {/if}
        </section>
      {:else if page === "battlegroup"}
        <section class="panel">
          <h2>Battlegroup</h2>
          <div class="actions">
            <button disabled={!battlegroup || !battlegroupStopped || !!lifecycleBusy} on:click={() => lifecycle("start")}>
              {lifecycleBusy === "start" ? "Starting..." : "Start"}
            </button>
            <button disabled={!battlegroup || battlegroupStopped || !!lifecycleBusy} on:click={() => lifecycle("restart")}>
              {lifecycleBusy === "restart" ? "Restarting..." : "Restart"}
            </button>
            <button disabled={!battlegroup || battlegroupStopped || !!lifecycleBusy} class="danger" on:click={() => lifecycle("stop")}>
              {lifecycleBusy === "stop" ? "Stopping..." : "Stop"}
            </button>
          </div>
          <div class="rows">
            <div class="row"><span>Name</span><b>{battlegroup?.name}</b></div>
            <div class="row"><span>Namespace</span><b>{battlegroup?.namespace}</b></div>
            <div class="row"><span>Stopped</span><b>{battlegroup?.stop ? "Yes" : "No"}</b></div>
            <div class="row"><span>Image</span><b>{battlegroup?.serverImage}</b></div>
          </div>
        </section>
      {:else if page === "layout" && layout}
        <section class="panel layout-panel">
          <div class="split-heading">
            <div>
              <p class="eyebrow">Runtime topology</p>
              <h2>World Layout</h2>
              <p class="muted">Scale map families and mark the single supported Deep Desert shard as PvE or PvP.</p>
            </div>
            <div class="layout-actions">
              {#if layout.restartRequired}
                <button disabled={!battlegroup || battlegroupStopped || !!lifecycleBusy} on:click={() => lifecycle("restart")}>
                  {lifecycleBusy === "restart" ? "Restarting..." : "Restart to apply"}
                </button>
              {/if}
              <button disabled={layoutSaving} on:click={saveLayout}>
                {layoutSaving ? "Applying..." : "Apply layout"}
              </button>
            </div>
          </div>

          <div class="layout-board">
            <article class="layout-control primary">
              <div>
                <span>Hagga Basin</span>
                <strong>{layout.haggaBasinInstances}</strong>
              </div>
              <p>Survival map dimensions. Each instance adds another dimension for the same map family.</p>
              <input
                aria-label="Hagga Basin instances"
                type="number"
                min="1"
                max="64"
                bind:value={layout.haggaBasinInstances}
                on:input={() => (layoutNotice = "")}
              />
            </article>

            <article class="layout-control">
              <div>
                <span>Social Hubs</span>
                <strong>{layout.socialHubsEnabled ? "Online" : "Off"}</strong>
              </div>
              <p>Arrakeen and HarkoVillage social services. Required when Deep Desert is enabled.</p>
              <label class="switch-row">
                <input
                  type="checkbox"
                  bind:checked={layout.socialHubsEnabled}
                  disabled={layoutDeepDesertMode !== "off"}
                  on:change={() => (layoutNotice = "")}
                />
                <span>{layoutDeepDesertMode !== "off" ? "Required by Deep Desert" : "Enable Social Hubs"}</span>
              </label>
            </article>

            <article class="layout-control span-2">
              <div>
                <span>Deep Desert</span>
                <strong>{layoutDeepDesertMode === "off" ? "Disabled" : layoutDeepDesertMode.toUpperCase()}</strong>
              </div>
              <p>Current builds support one Deep Desert instance total. More shards stay blocked until routing is fully understood.</p>
              <div class="segmented">
                <button class:active={layoutDeepDesertMode === "off"} on:click={() => setDeepDesertMode("off")}>Off</button>
                <button class:active={layoutDeepDesertMode === "pve"} on:click={() => setDeepDesertMode("pve")}>PvE</button>
                <button class:active={layoutDeepDesertMode === "pvp"} on:click={() => setDeepDesertMode("pvp")}>PvP</button>
              </div>
            </article>
          </div>

          <div class="layout-summary">
            <div>
              <span>Estimated memory</span>
              <strong>{layoutMemory?.gb ?? 0} GB</strong>
            </div>
            <ul>
              {#each layoutMemory?.lines ?? [] as line}
                <li>{line}</li>
              {/each}
            </ul>
          </div>

          {#if layoutNotice}<p class="warn">{layoutNotice}</p>{/if}
          {#if layout.restartRequired && !layoutNotice}<p class="warn">Restart required for all changes to converge.</p>{/if}
          {#if layout.warnings.length}
            <div class="warning-list">
              {#each layout.warnings as warning}
                <p>{warning}</p>
              {/each}
            </div>
          {/if}
        </section>
      {:else if page === "config"}
        <section class="panel form">
          <div class="split-heading">
            <div>
              <h2>Game Settings</h2>
              <p class="muted">Tune the player-facing INI settings through controlled fields. Raw file access stays tucked away for diagnostics.</p>
            </div>
            {#if settingsFile}
              <div class="actions">
                <button disabled={settingsPreviewBusy || settingsDraft === settingsFile.content} on:click={previewSettingsFile}>
                  {settingsPreviewBusy ? "Previewing..." : "Preview changes"}
                </button>
                <button disabled={settingsSaving || settingsDraft === settingsFile.content} on:click={saveSettingsFile}>
                  {settingsSaving ? "Saving..." : "Save settings"}
                </button>
              </div>
            {/if}
          </div>
          <div class="file-tabs">
            {#each settingsCatalog?.files ?? [] as file}
              <button class:active={selectedSettingsFile === file.id} on:click={() => loadSettingsFile(file.id)}>
                {file.fileName}
              </button>
            {/each}
          </div>
          {#if !settingsFile}
            <button disabled={settingsAutoLoading} on:click={() => loadSettingsFile()}>
              {settingsAutoLoading ? "Loading settings..." : "Load settings"}
            </button>
          {:else}
            <div class="settings-status">
              <div>
                <span>Editing</span>
                <strong>{settingsFile.fileName}</strong>
              </div>
              <div>
                <span>Fields</span>
                <strong>{settingsDraftSections.reduce((count, section) => count + section.entries.length, 0)}</strong>
              </div>
              <div>
                <span>Sections</span>
                <strong>{settingsDraftSections.length}</strong>
              </div>
              <div>
                <span>Status</span>
                <strong>{settingsDraft === settingsFile.content ? "Saved" : "Unsaved changes"}</strong>
              </div>
            </div>
            <div class="structured-editor">
              <div class="editor-title">
                <div>
                  <h3>Controlled fields</h3>
                  <p class="muted">Values are written back into the original INI while preserving comments, ordering, and duplicate keys.</p>
                </div>
                <input bind:value={settingsFilter} placeholder="Filter section, key, or value" />
              </div>
              <div class="settings-section-list">
                {#each visibleSettingsSections as section}
                  <details open>
                    <summary>{section.name} <span>{section.entries.length}</span></summary>
                    <div class="setting-grid">
                      {#each section.entries.slice(0, 32) as entry}
                        <label class="setting-field">
                          <span>{settingDisplayName(entry.key)}</span>
                          <small>{entry.key}</small>
                          {#if settingFieldKind(entry.value) === "boolean"}
                            <select value={entry.value.toLowerCase() === "true" ? "True" : "False"} on:change={(event) => updateSettingsEntry(entry.line, event.currentTarget.value)}>
                              <option>True</option>
                              <option>False</option>
                            </select>
                          {:else if settingFieldKind(entry.value) === "number"}
                            <input type="number" value={entry.value} on:input={(event) => updateSettingsEntry(entry.line, event.currentTarget.value)} />
                          {:else}
                            <input value={entry.value} on:input={(event) => updateSettingsEntry(entry.line, event.currentTarget.value)} />
                          {/if}
                        </label>
                      {/each}
                    </div>
                    {#if section.entries.length > 32}<p class="muted setting-note">Showing first 32 matching entries in this section.</p>{/if}
                  </details>
                {/each}
              </div>
            </div>
            <details class="advanced-ini">
              <summary>Advanced raw INI view <span>{settingsFile.path}</span></summary>
              <textarea bind:value={settingsDraft} on:input={markSettingsDraftChanged} spellcheck="false"></textarea>
            </details>
            {#if settingsPreview}
              <section class="diff-panel">
                <div class="editor-title">
                  <div>
                    <h3>Change preview</h3>
                    <p class="muted">
                      {settingsPreview.changed
                        ? `${settingsPreview.addedLines} added, ${settingsPreview.removedLines} removed`
                        : "No changes compared to the live file."}
                    </p>
                  </div>
                </div>
                {#if settingsPreview.hunks.length}
                  <div class="diff-list">
                    {#each settingsPreview.hunks as hunk}
                      <article class="diff-hunk">
                        <header>
                          -{hunk.oldStart},{hunk.oldLines} +{hunk.newStart},{hunk.newLines}
                        </header>
                        {#each hunk.lines as line}
                          <div class:insert={line.kind === "insert"} class:delete={line.kind === "delete"} class:equal={line.kind === "equal"}>
                            <span>{line.kind === "insert" ? "+" : line.kind === "delete" ? "-" : " "}</span>
                            <code>{line.text || " "}</code>
                          </div>
                        {/each}
                      </article>
                    {/each}
                  </div>
                {:else}
                  <p class="muted">The current draft matches the live file.</p>
                {/if}
              </section>
            {/if}
            {#if settingsNotice}<p class="warn">{settingsNotice}</p>{/if}
            <section class="backup-panel">
              <div class="editor-title">
                <div>
                  <h3>Backups</h3>
                  <p class="muted">A backup is created automatically before every save and restore.</p>
                </div>
                <div class="actions">
                  <button disabled={!!settingsBackupBusy} on:click={() => loadSettingsBackups()}>
                    {settingsBackupBusy === "load" ? "Loading..." : "Refresh"}
                  </button>
                  <button disabled={!!settingsBackupBusy} on:click={createSettingsBackup}>
                    {settingsBackupBusy === "create" ? "Creating..." : "Create backup"}
                  </button>
                </div>
              </div>
              {#if settingsBackups?.backups.length}
                <div class="backup-list">
                  {#each settingsBackups.backups.slice(0, 8) as backup}
                    <article class="backup-row">
                      <div>
                        <strong>{formatBackupTime(backup.modifiedAt)}</strong>
                        <span>{backup.id} · {backup.sizeBytes} bytes</span>
                      </div>
                      <button
                        class="danger"
                        disabled={!!settingsBackupBusy}
                        on:click={() => restoreSettingsBackup(backup.id)}
                      >
                        {settingsBackupBusy === backup.id ? "Restoring..." : "Restore"}
                      </button>
                    </article>
                  {/each}
                </div>
              {:else}
                <p class="muted">No backups found for this file yet.</p>
              {/if}
            </section>
          {/if}
        </section>
      {:else if page === "director"}
        <section class="panel form">
          <div class="split-heading">
            <div>
              <h2>Director Rules</h2>
              <p class="muted">Edit authenticated Director overrides as controlled fields without exposing the internal Director service.</p>
            </div>
            <button disabled={directorBusy} on:click={loadDirectorConfig}>
              {directorBusy || directorAutoLoading ? "Working..." : directorFlsDraft || directorTransferDraft ? "Refresh rules" : "Load rules"}
            </button>
          </div>
          {#if directorNotice}<p class="warn">{directorNotice}</p>{/if}
          <div class="editor-grid">
            <section>
              <div class="editor-title">
                <div>
                  <h3>FLS report settings</h3>
                  <p class="muted">Telemetry and reporting override values.</p>
                </div>
                <div class="actions">
                  <button disabled={directorBusy || !directorFlsDraft} on:click={() => saveDirectorConfig("fls")}>Save</button>
                  <button disabled={directorBusy} class="danger" on:click={() => clearDirectorConfig("fls")}>Clear</button>
                </div>
              </div>
              {#if directorFlsFields.length}
                <div class="json-field-list">
                  {#each directorFlsFields as field}
                    <label class="json-field">
                      <span>{field.label}</span>
                      <small>{field.pointer || "/"}</small>
                      {#if field.kind === "boolean"}
                        <select value={field.value} on:change={(event) => updateDirectorJsonDraft("fls", field.pointer, event.currentTarget.value, field.kind)}>
                          <option value="true">True</option>
                          <option value="false">False</option>
                        </select>
                      {:else if field.kind === "number"}
                        <input type="number" value={field.value} on:input={(event) => updateDirectorJsonDraft("fls", field.pointer, event.currentTarget.value, field.kind)} />
                      {:else}
                        <input value={field.value} on:input={(event) => updateDirectorJsonDraft("fls", field.pointer, event.currentTarget.value, field.kind)} />
                      {/if}
                    </label>
                  {/each}
                </div>
              {:else}
                <p class="muted">Load rules to edit FLS report settings.</p>
              {/if}
              <details class="advanced-ini">
                <summary>Advanced raw JSON</summary>
                <textarea bind:value={directorFlsDraft} spellcheck="false" placeholder="Load config to edit JSON"></textarea>
              </details>
            </section>
            <section>
              <div class="editor-title">
                <div>
                  <h3>Character transfer</h3>
                  <p class="muted">Travel and transfer override values.</p>
                </div>
                <div class="actions">
                  <button disabled={directorBusy || !directorTransferDraft} on:click={() => saveDirectorConfig("transfer")}>Save</button>
                  <button disabled={directorBusy} class="danger" on:click={() => clearDirectorConfig("transfer")}>Clear</button>
                </div>
              </div>
              {#if directorTransferFields.length}
                <div class="json-field-list">
                  {#each directorTransferFields as field}
                    <label class="json-field">
                      <span>{field.label}</span>
                      <small>{field.pointer || "/"}</small>
                      {#if field.kind === "boolean"}
                        <select value={field.value} on:change={(event) => updateDirectorJsonDraft("transfer", field.pointer, event.currentTarget.value, field.kind)}>
                          <option value="true">True</option>
                          <option value="false">False</option>
                        </select>
                      {:else if field.kind === "number"}
                        <input type="number" value={field.value} on:input={(event) => updateDirectorJsonDraft("transfer", field.pointer, event.currentTarget.value, field.kind)} />
                      {:else}
                        <input value={field.value} on:input={(event) => updateDirectorJsonDraft("transfer", field.pointer, event.currentTarget.value, field.kind)} />
                      {/if}
                    </label>
                  {/each}
                </div>
              {:else}
                <p class="muted">Load rules to edit character transfer settings.</p>
              {/if}
              <details class="advanced-ini">
                <summary>Advanced raw JSON</summary>
                <textarea bind:value={directorTransferDraft} spellcheck="false" placeholder="Load config to edit JSON"></textarea>
              </details>
            </section>
          </div>
          <section class="map-editor">
            <div class="editor-title">
              <div>
                <h3>Map override</h3>
                <p class="muted">Tune per-map caps, scaling, and dimension overrides through Director.</p>
              </div>
              <div class="actions">
                <button disabled={directorBusy || !directorMapDetail} on:click={saveDirectorMapOverride}>Save</button>
                <button disabled={directorBusy || !directorMapDetail} class="danger" on:click={clearDirectorMapOverride}>Clear</button>
              </div>
            </div>
            <div class="map-select-row">
              <select bind:value={selectedDirectorMap} on:change={() => loadDirectorMapOverride(selectedDirectorMap)}>
                {#each overview?.maps ?? [] as map}
                  <option value={map.name}>{map.name} - {map.kind}{map.hasOverride ? " - override" : ""}</option>
                {/each}
              </select>
              <button disabled={directorBusy || !selectedDirectorMap} on:click={() => loadDirectorMapOverride()}>
                {directorBusy ? "Working..." : "Load map"}
              </button>
            </div>
            {#if directorMapDetail}
              <div class="rows compact">
                <div class="row"><span>Map</span><b>{directorMapDetail.name}</b></div>
                <div class="row"><span>Kind</span><b>{directorMapDetail.kind}</b></div>
                <div class="row"><span>Override</span><b>{directorMapDetail.hasOverride ? "Active" : "None"}</b></div>
                <div class="row"><span>Payload key</span><b>{directorMapDetail.configKey}</b></div>
              </div>
              {#if directorMapFields.length}
                <div class="json-field-list map-json-fields">
                  {#each directorMapFields as field}
                    <label class="json-field">
                      <span>{field.label}</span>
                      <small>{field.pointer || "/"}</small>
                      {#if field.kind === "boolean"}
                        <select value={field.value} on:change={(event) => updateDirectorJsonDraft("map", field.pointer, event.currentTarget.value, field.kind)}>
                          <option value="true">True</option>
                          <option value="false">False</option>
                        </select>
                      {:else if field.kind === "number"}
                        <input type="number" value={field.value} on:input={(event) => updateDirectorJsonDraft("map", field.pointer, event.currentTarget.value, field.kind)} />
                      {:else}
                        <input value={field.value} on:input={(event) => updateDirectorJsonDraft("map", field.pointer, event.currentTarget.value, field.kind)} />
                      {/if}
                    </label>
                  {/each}
                </div>
              {:else}
                <p class="muted">This map override payload has no primitive fields to edit.</p>
              {/if}
              <details class="advanced-ini">
                <summary>Advanced raw JSON</summary>
                <textarea bind:value={directorMapDraft} spellcheck="false" placeholder="Load a map to edit override JSON"></textarea>
              </details>
              <details>
                <summary>Effective config</summary>
                <pre class="json-preview">{formatJson(directorMapDetail.effectiveConfig)}</pre>
              </details>
              <details>
                <summary>Runtime servers <span>{directorMapDetail.servers.length}</span></summary>
                <div class="rows compact">
                  {#each directorMapDetail.servers as server}
                    <div class="row">
                      <span>{server.label || "Unnamed"} dim {server.dimensionIndex ?? "?"}</span>
                      <b>{server.status} - {server.players} players</b>
                    </div>
                  {/each}
                </div>
              </details>
            {:else}
              <p class="muted">Load a map to edit its Director override payload.</p>
            {/if}
          </section>
          <details class="api-console">
            <summary>Advanced Director API console</summary>
            <div class="editor-title api-console-title">
              <div>
                <h3>Allowlisted API calls</h3>
                <p class="muted">Run allowlisted Director calls through the authenticated Manager API proxy.</p>
              </div>
              <button disabled={directorApiBusy} on:click={() => loadDirectorCapabilities()}>
                {directorApiBusy ? "Working..." : "Load paths"}
              </button>
            </div>
            {#if directorCapabilities}
              <div class="rows compact">
                <div class="row"><span>Director</span><b>{directorCapabilities.configured ? "Reachable" : "Unavailable"}</b></div>
                <div class="row"><span>Allowlisted paths</span><b>{directorCapabilities.apiPaths.length}</b></div>
              </div>
              <div class="api-console-grid">
                <label>
                  Path
                  <select bind:value={directorApiSelection}>
                    {#each directorCapabilities.apiPaths as item}
                      <option value={capabilityKey(item)}>{capabilityKey(item)}</option>
                    {/each}
                  </select>
                </label>
                <button disabled={directorApiBusy || !selectedDirectorCapability()} on:click={runDirectorApiCall}>
                  {directorApiBusy ? "Running..." : "Run"}
                </button>
              </div>
              {#if selectedDirectorCapability()?.method !== "GET"}
                <label>
                  JSON body
                  <textarea bind:value={directorApiBody} spellcheck="false"></textarea>
                </label>
              {/if}
              {#if directorApiResult}
                <textarea readonly value={directorApiResult} spellcheck="false"></textarea>
              {/if}
            {:else}
              <p class="muted">Load paths to inspect the current Director proxy coverage.</p>
            {/if}
          </details>
        </section>
      {:else if page === "players"}
        <div class="grid">
          <Card label="Active" value={`${overview?.players?.active ?? 0}`} />
          <Card label="Online" value={`${overview?.players?.online ?? 0}`} />
          <Card label="Queued" value={`${overview?.players?.queued ?? 0}`} />
          <Card label="Travel" value={`${overview?.players?.inTransit ?? 0}`} />
        </div>
        <section class="panel player-stats-panel">
          <div class="split-heading">
            <div>
              <h2>Player Statistics</h2>
              <p class="muted">Controlled database statistics for characters, accounts, guilds, tags, and recent logins.</p>
            </div>
            <button disabled={playerStatisticsBusy} on:click={() => loadPlayerStatistics()}>
              {playerStatisticsBusy ? "Loading..." : playerStatistics ? "Refresh statistics" : "Load statistics"}
            </button>
          </div>
          {#if playerStatistics}
            <div class="stats-strip">
              <div><span>Characters</span><b>{playerStatistics.statistics.totalPlayers}</b></div>
              <div><span>Accounts</span><b>{playerStatistics.statistics.totalAccounts}</b></div>
              <div><span>Guilds</span><b>{playerStatistics.statistics.guilds}</b></div>
              <div><span>Guild members</span><b>{playerStatistics.statistics.guildMembers}</b></div>
              <div><span>Tagged players</span><b>{playerStatistics.statistics.taggedPlayers}</b></div>
            </div>
            <div class="stats-columns">
              <div>
                <h3>Online states</h3>
                <div class="rows compact">
                  {#each playerStatistics.statistics.onlineStatuses as item}
                    <div class="row"><span>{item.name}</span><b>{item.count}</b></div>
                  {/each}
                  {#if !playerStatistics.statistics.onlineStatuses.length}<p class="muted">No player state rows yet.</p>{/if}
                </div>
              </div>
              <div>
                <h3>Life states</h3>
                <div class="rows compact">
                  {#each playerStatistics.statistics.lifeStates as item}
                    <div class="row"><span>{item.name}</span><b>{item.count}</b></div>
                  {/each}
                  {#if !playerStatistics.statistics.lifeStates.length}<p class="muted">No player state rows yet.</p>{/if}
                </div>
              </div>
              <div>
                <h3>Recent logins</h3>
                <div class="rows compact">
                  {#each playerStatistics.statistics.recentPlayers as player}
                    <div class="row"><span>{player.characterName || `Account ${player.accountId}`}</span><b>{formatEventTime(player.lastLoginTime)}</b></div>
                  {/each}
                  {#if !playerStatistics.statistics.recentPlayers.length}<p class="muted">No recent login records yet.</p>{/if}
                </div>
              </div>
            </div>
          {:else}
            <p class="muted">Loading player statistics from controlled database queries.</p>
          {/if}
        </section>
        <section class="panel player-directory-panel">
          <div class="split-heading">
            <div>
              <h2>Player Directory</h2>
              <p class="muted">Operational player state from the game database: character, guild, partition, status, and recent activity.</p>
            </div>
            <div class="actions">
              <input bind:value={playerFilter} placeholder="Filter players, guilds, status, or partition" />
              <button disabled={databasePlayersBusy} on:click={() => loadDatabasePlayers()}>
                {databasePlayersBusy ? "Loading..." : databasePlayers ? "Refresh directory" : "Load directory"}
              </button>
            </div>
          </div>
          {#if visibleDatabasePlayers.length}
            <div class="player-directory">
              {#each visibleDatabasePlayers as player}
                <article>
                  <div>
                    <strong>{player.characterName || `Account ${player.accountId}`}</strong>
                    <span>{player.guildName || "No guild"} · account {player.accountId}</span>
                  </div>
                  <div class="player-directory-state">
                    <b class:good={player.onlineStatus === "Online"}>{player.onlineStatus || "Unknown"}</b>
                    <span>{player.lifeState || "No life state"}</span>
                  </div>
                  <div class="player-directory-meta">
                    <span>Server</span><b>{player.serverId || "None"}</b>
                    <span>Partition</span><b>{player.previousServerPartitionId ?? "Unknown"}</b>
                    <span>Home dimension</span><b>{player.homeDimensionIndex ?? "Unknown"}</b>
                    <span>Last login</span><b>{formatBackupTime(player.lastLoginTime)}</b>
                  </div>
                  <div class="player-tag-editor">
                    <div class="tag-row">
                      {#each player.tags as tag}
                        <span>
                          {tag}
                          <button
                            class="tag-remove"
                            disabled={playerTagBusy[player.accountId]}
                            on:click={() => removePlayerTag(player, tag)}
                          >
                            Remove
                          </button>
                        </span>
                      {/each}
                      {#if !player.tags.length}<em>No tags</em>{/if}
                    </div>
                    <div class="tag-add">
                      <input
                        value={playerTagDrafts[player.accountId] || ""}
                        maxlength="64"
                        placeholder="Add operator tag"
                        on:input={(event) => updatePlayerTagDraft(player.accountId, event.currentTarget.value)}
                      />
                      <button
                        class="inline"
                        disabled={playerTagBusy[player.accountId]}
                        on:click={() => savePlayerTag(player)}
                      >
                        {playerTagBusy[player.accountId] ? "Saving..." : "Add tag"}
                      </button>
                    </div>
                  </div>
                </article>
              {/each}
            </div>
          {:else if databasePlayers}
            <p class="muted">
              {databasePlayers.rows.length
                ? "No database players match the current filter."
                : "The database has no player_state rows yet."}
            </p>
          {:else}
            <p class="muted">Loading the player directory. This is a controlled database view, not raw SQL access.</p>
          {/if}
        </section>
        <section class="panel player-activity-panel">
          <div class="split-heading">
            <div>
              <h2>Player Activity</h2>
              <p class="muted">Director player IDs grouped into one operational view. Full mode adds transit, grace, completion, and queue buckets.</p>
            </div>
            <div class="actions">
              <button disabled={playersBusy} on:click={() => loadPlayerLists(false)}>
                {playersBusy && !playersFull ? "Loading..." : playerLists && !playersFull ? "Refresh active" : "Load active"}
              </button>
              <button disabled={playersBusy} on:click={() => loadPlayerLists(true)}>
                {playersBusy && playersFull ? "Loading..." : playersFull ? "Refresh full" : "Load full"}
              </button>
            </div>
          </div>
          {#if playerLists}
            <div class="player-table">
              {#if visiblePlayerRows.length}
                {#each visiblePlayerRows as player}
                  <article>
                    <div>
                      <strong>{player.id}</strong>
                      <span>{player.buckets.join(" · ") || "Observed"}</span>
                    </div>
                    <b class:warning={player.status !== "Online" && player.status !== "Active"}>{player.status}</b>
                  </article>
                {/each}
              {:else}
                <p class="muted">
                  {playerRows.length
                    ? "No players match the current filter."
                    : "No active player IDs are reported by Director right now."}
                </p>
              {/if}
            </div>
          {:else}
            <p class="muted">Loading active player buckets. Full player state is available on demand because some Director player queries can be slow.</p>
          {/if}
        </section>
        {#if playerLists}
          <section class="panel">
            <div class="editor-title">
              <div>
                <h2>Director Buckets</h2>
                <p class="muted">Detailed bucket lists for diagnosis and support cases.</p>
              </div>
            </div>
            <div class="player-columns">
              <PlayerBucket title="All" ids={playerLists.all} />
              <PlayerBucket title="Online" ids={playerLists.online} />
              <PlayerBucket title="In transit" ids={playerLists.inTransit} />
              <PlayerBucket title="Grace" ids={playerLists.gracePeriod} />
              <PlayerBucket title="Completion" ids={playerLists.completion} />
              <PlayerBucket title="Queued" ids={playerLists.queued} />
            </div>
          </section>
        {/if}
        <section class="panel">
          <h2>Maps</h2>
          <div class="rows">
            {#each overview?.maps ?? [] as map}
              <div class="row"><span>{map.name}</span><b>{map.players} players</b></div>
            {/each}
          </div>
        </section>
      {:else if page === "logs"}
        <section class="panel form">
          <div class="split-heading">
            <div>
              <h2>Logs</h2>
              <p class="muted">Read a tail snapshot or follow live Kubernetes pod logs.</p>
            </div>
            <b class:good={logStreaming}>{logStreaming ? "Streaming" : "Idle"}</b>
          </div>
          <select bind:value={selectedPod}>
            {#each pods as pod}<option>{pod.name}</option>{/each}
          </select>
          <select bind:value={selectedContainer}>
            <option value="">Default container</option>
            {#each selectedPodSummary?.containers ?? [] as container}<option>{container}</option>{/each}
          </select>
          <div class="actions">
            <button disabled={!selectedPod || logStreaming} on:click={loadLogs}>Load tail</button>
            <button disabled={!selectedPod || logStreaming} on:click={startLogStream}>Follow live</button>
            <button disabled={logStreaming || logExporting} on:click={exportAllLogs}>
              {logExporting ? "Exporting..." : "Export all pods"}
            </button>
            <button disabled={!logStreaming} class="danger" on:click={stopLogStream}>Stop stream</button>
          </div>
          {#if logStreamError}<p class="warn">{logStreamError}</p>{/if}
          <div class="logs" bind:this={logViewer}>{#each logLines as line}<div>{line}</div>{/each}</div>
        </section>
      {:else if page === "settings"}
        <section class="panel form">
          <h2>Settings</h2>
          <label>Display name <input bind:value={titleDraft} /></label>
          <button on:click={saveTitle}>Save name</button>
        </section>
        <section class="panel form">
          <div class="split-heading">
            <div>
              <h2>Manager API</h2>
              <p class="muted">Inspect the local control service that powers this web manager.</p>
            </div>
            <div class="actions">
              <button disabled={!!managerBusy} on:click={loadManagerSelf}>
                {managerBusy === "self" ? "Loading..." : "Load status"}
              </button>
              <button disabled={!!managerBusy} on:click={loadManagerLogs}>
                {managerBusy === "logs" ? "Loading..." : "Load logs"}
              </button>
            </div>
          </div>
          {#if managerSelf}
            <div class="rows compact">
              <div class="row"><span>Service</span><b>{managerSelf.serviceName}</b></div>
              <div class="row"><span>Version</span><b>{managerSelf.apiVersion}</b></div>
              <div class="row"><span>Uptime</span><b>{formatDuration(managerSelf.uptimeSeconds)}</b></div>
              <div class="row"><span>PID</span><b>{managerSelf.pid}</b></div>
              <div class="row"><span>Port</span><b>{managerSelf.port}</b></div>
              <div class="row"><span>Director</span><b>{managerSelf.directorConfigured ? "Reachable" : "Unavailable"}</b></div>
              <div class="row"><span>Binary</span><b>{managerSelf.binaryPath}</b></div>
              <div class="row"><span>Environment</span><b>{managerSelf.envPath}</b></div>
              <div class="row"><span>Log</span><b>{managerSelf.logPath}</b></div>
            </div>
          {:else}
            <p class="muted">Load Manager API status to inspect process paths, uptime, and service health.</p>
          {/if}

          {#if managerLogs}
            <div class="split-heading">
              <p class="muted">
                {managerLogs.available ? `${managerLogs.lines.length} redacted log lines` : "Manager log file is not available yet."}
                {managerLogs.truncated ? " Large log truncated before tailing." : ""}
              </p>
            </div>
            <div class="logs compact-log">{#each managerLogs.lines as line}<div>{line}</div>{/each}</div>
          {/if}
        </section>
      {/if}
    </section>
  </main>
{/if}
