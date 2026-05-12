<script lang="ts">
  import { onMount } from "svelte";
  import Card from "./Card.svelte";
  import {
    ApiError,
    api,
    type DirectorMapConfigDetail,
    type LogsResponse,
    type Overview,
    type Session,
    type TelemetryEnvelope,
    type TelemetrySnapshot,
    type UserSettingsCatalog,
    type UserSettingsFile,
    type UserSettingsUpdateResponse,
    type WorldLayout,
  } from "./api";

  type Page = "dashboard" | "battlegroup" | "layout" | "config" | "director" | "players" | "logs" | "settings";

  let session: Session | null = null;
  let token = "";
  let loading = true;
  let signingIn = false;
  let error = "";
  let page: Page = "dashboard";
  let overview: Overview | null = null;
  let layout: WorldLayout | null = null;
  let selectedPod = "";
  let selectedContainer = "";
  let logLines: string[] = [];
  let titleDraft = "";
  let settingsCatalog: UserSettingsCatalog | null = null;
  let selectedSettingsFile = "game";
  let settingsFile: UserSettingsFile | null = null;
  let settingsDraft = "";
  let settingsSaving = false;
  let settingsNotice = "";
  let directorFlsDraft = "";
  let directorTransferDraft = "";
  let selectedDirectorMap = "";
  let directorMapDetail: DirectorMapConfigDetail | null = null;
  let directorMapDraft = "";
  let directorNotice = "";
  let directorBusy = false;
  let telemetrySocket: WebSocket | null = null;
  let telemetryConnected = false;
  let telemetrySnapshots = 0;
  let telemetryLastAt = "";
  let telemetryError = "";

  $: battlegroup = overview?.battlegroups[0] ?? null;
  $: pods = overview?.workloads.pods ?? [];
  $: selectedPodSummary = pods.find((pod) => pod.name === selectedPod);

  onMount(async () => {
    await loadSession();
    if (session) {
      await refresh();
      startTelemetry();
    }
    loading = false;
    const timer = window.setInterval(() => {
      if (session) void refresh(false);
    }, 10000);
    return () => window.clearInterval(timer);
  });

  async function loadSession() {
    try {
      const current = await api<Session>("/api/auth/session");
      session = current.authenticated ? current : null;
    } catch {
      session = null;
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
    session = null;
    overview = null;
    layout = null;
    settingsCatalog = null;
    settingsFile = null;
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
    error = "";
    try {
      await api(`/api/battlegroups/${battlegroup.namespace}/${battlegroup.name}/${action}`, { method: "POST" });
      await refresh();
    } catch (err) {
      error = message(err);
    }
  }

  async function saveLayout() {
    if (!battlegroup || !layout) return;
    error = "";
    try {
      const result = await api<{ layout: WorldLayout; warnings: string[] }>(
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
      if (result.warnings.length) error = result.warnings.join(" ");
      await refresh(false);
    } catch (err) {
      error = message(err);
    }
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
    const query = new URLSearchParams({ pod: selectedPod, tail: "400" });
    if (selectedContainer) query.set("container", selectedContainer);
    try {
      const logs = await api<LogsResponse>(`/api/logs?${query}`);
      logLines = logs.lines;
    } catch (err) {
      error = message(err);
    }
  }

  async function loadSettingsFile(file = selectedSettingsFile) {
    error = "";
    settingsNotice = "";
    selectedSettingsFile = file;
    try {
      settingsFile = await api<UserSettingsFile>(`/api/config/user-settings/${file}`);
      settingsDraft = settingsFile.content;
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
      settingsNotice = result.restartRecommended
        ? "Saved. Restart the battlegroup for every runtime system to pick up the change."
        : "Saved.";
    } catch (err) {
      error = message(err);
    } finally {
      settingsSaving = false;
    }
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
      const mapName = selectedDirectorMap || overview?.maps[0]?.name;
      if (mapName) await loadDirectorMapOverride(mapName, false);
    } catch (err) {
      error = message(err);
    } finally {
      directorBusy = false;
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
      {#each ["dashboard", "battlegroup", "layout", "config", "director", "players", "logs", "settings"] as item}
        <button class:active={page === item} on:click={() => (page = item as Page)}>{item}</button>
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
      {#if error}<p class="error">{error}</p>{/if}

      {#if page === "dashboard"}
        <div class="grid">
          <Card label="Battlegroup" value={battlegroup?.phase || "Unknown"} />
          <Card label="Pods" value={`${overview?.status.pods ?? 0}`} />
          <Card label="Players" value={`${overview?.players?.active ?? 0}`} />
          <Card label="Director" value={overview?.directorAvailable ? "Reachable" : "Unavailable"} />
          <Card label="Telemetry" value={telemetryConnected ? "Live" : "Offline"} />
        </div>
        <section class="panel">
          <h2>Live Stream</h2>
          <div class="rows compact">
            <div class="row"><span>Status</span><b class:good={telemetryConnected}>{telemetryConnected ? "Connected" : "Disconnected"}</b></div>
            <div class="row"><span>Snapshots</span><b>{telemetrySnapshots}</b></div>
            <div class="row"><span>Last update</span><b>{telemetryLastAt || "Waiting"}</b></div>
          </div>
          {#if telemetryError}<p class="warn">{telemetryError}</p>{/if}
        </section>
        <section class="panel">
          <h2>Workloads</h2>
          <div class="rows">
            {#each pods as pod}
              <div class="row"><span>{pod.name}</span><b class:good={pod.ready}>{pod.ready ? "Ready" : pod.phase}</b></div>
            {/each}
          </div>
        </section>
      {:else if page === "battlegroup"}
        <section class="panel">
          <h2>Battlegroup</h2>
          <div class="actions">
            <button on:click={() => lifecycle("start")}>Start</button>
            <button on:click={() => lifecycle("restart")}>Restart</button>
            <button class="danger" on:click={() => lifecycle("stop")}>Stop</button>
          </div>
          <div class="rows">
            <div class="row"><span>Name</span><b>{battlegroup?.name}</b></div>
            <div class="row"><span>Namespace</span><b>{battlegroup?.namespace}</b></div>
            <div class="row"><span>Stopped</span><b>{battlegroup?.stop ? "Yes" : "No"}</b></div>
            <div class="row"><span>Image</span><b>{battlegroup?.serverImage}</b></div>
          </div>
        </section>
      {:else if page === "layout" && layout}
        <section class="panel form">
          <h2>World Layout</h2>
          <label>Hagga Basin instances <input type="number" min="1" max="64" bind:value={layout.haggaBasinInstances} /></label>
          <label>Social Hubs <input type="checkbox" bind:checked={layout.socialHubsEnabled} /></label>
          <label>Deep Desert PvE <input type="number" min="0" max="1" bind:value={layout.deepDesertPveInstances} /></label>
          <label>Deep Desert PvP <input type="number" min="0" max="1" bind:value={layout.deepDesertPvpInstances} /></label>
          <p class="muted">Current builds support one Deep Desert instance total.</p>
          <button on:click={saveLayout}>Apply layout</button>
          {#if layout.restartRequired}<p class="warn">Restart required for all changes to converge.</p>{/if}
        </section>
      {:else if page === "config"}
        <section class="panel form">
          <div class="split-heading">
            <div>
              <h2>User Settings</h2>
              <p class="muted">Edit the runtime ini files mounted through the filebrowser volume.</p>
            </div>
            {#if settingsFile}
              <button disabled={settingsSaving || settingsDraft === settingsFile.content} on:click={saveSettingsFile}>
                {settingsSaving ? "Saving..." : "Save file"}
              </button>
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
            <button on:click={() => loadSettingsFile()}>Load settings file</button>
          {:else}
            <div class="rows compact">
              <div class="row"><span>Path</span><b>{settingsFile.path}</b></div>
              <div class="row"><span>Size</span><b>{settingsFile.sizeBytes} bytes</b></div>
              <div class="row"><span>Sections</span><b>{settingsFile.sections.length}</b></div>
            </div>
            <textarea bind:value={settingsDraft} spellcheck="false"></textarea>
            {#if settingsNotice}<p class="warn">{settingsNotice}</p>{/if}
            <div class="section-list">
              {#each settingsFile.sections.slice(0, 12) as section}
                <details>
                  <summary>{section.name} <span>{section.entries.length}</span></summary>
                  <div class="rows compact">
                    {#each section.entries.slice(0, 12) as entry}
                      <div class="row"><span>{entry.key}</span><b>{entry.value}</b></div>
                    {/each}
                  </div>
                </details>
              {/each}
            </div>
          {/if}
        </section>
      {:else if page === "director"}
        <section class="panel form">
          <div class="split-heading">
            <div>
              <h2>Director Config</h2>
              <p class="muted">Edit authenticated Director overrides without exposing the internal Director service.</p>
            </div>
            <button disabled={directorBusy} on:click={loadDirectorConfig}>
              {directorBusy ? "Working..." : "Load config"}
            </button>
          </div>
          {#if directorNotice}<p class="warn">{directorNotice}</p>{/if}
          <div class="editor-grid">
            <section>
              <div class="editor-title">
                <h3>FLS report settings</h3>
                <div class="actions">
                  <button disabled={directorBusy || !directorFlsDraft} on:click={() => saveDirectorConfig("fls")}>Save</button>
                  <button disabled={directorBusy} class="danger" on:click={() => clearDirectorConfig("fls")}>Clear</button>
                </div>
              </div>
              <textarea bind:value={directorFlsDraft} spellcheck="false" placeholder="Load config to edit JSON"></textarea>
            </section>
            <section>
              <div class="editor-title">
                <h3>Character transfer</h3>
                <div class="actions">
                  <button disabled={directorBusy || !directorTransferDraft} on:click={() => saveDirectorConfig("transfer")}>Save</button>
                  <button disabled={directorBusy} class="danger" on:click={() => clearDirectorConfig("transfer")}>Clear</button>
                </div>
              </div>
              <textarea bind:value={directorTransferDraft} spellcheck="false" placeholder="Load config to edit JSON"></textarea>
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
              <textarea bind:value={directorMapDraft} spellcheck="false" placeholder="Load a map to edit override JSON"></textarea>
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
        </section>
      {:else if page === "players"}
        <div class="grid">
          <Card label="Active" value={`${overview?.players?.active ?? 0}`} />
          <Card label="Online" value={`${overview?.players?.online ?? 0}`} />
          <Card label="Queued" value={`${overview?.players?.queued ?? 0}`} />
        </div>
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
          <h2>Logs</h2>
          <select bind:value={selectedPod}>
            {#each pods as pod}<option>{pod.name}</option>{/each}
          </select>
          <select bind:value={selectedContainer}>
            <option value="">Default container</option>
            {#each selectedPodSummary?.containers ?? [] as container}<option>{container}</option>{/each}
          </select>
          <button on:click={loadLogs}>Load logs</button>
          <div class="logs">{#each logLines as line}<div>{line}</div>{/each}</div>
        </section>
      {:else if page === "settings"}
        <section class="panel form">
          <h2>Settings</h2>
          <label>Display name <input bind:value={titleDraft} /></label>
          <button on:click={saveTitle}>Save name</button>
        </section>
      {/if}
    </section>
  </main>
{/if}
