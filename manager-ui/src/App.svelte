<script lang="ts">
  import { onMount } from "svelte";
  import Card from "./Card.svelte";
  import { ApiError, api, type BattlegroupSummary, type LogsResponse, type Overview, type Session, type WorldLayout } from "./api";

  type Page = "dashboard" | "battlegroup" | "layout" | "players" | "logs" | "settings";

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

  $: battlegroup = overview?.battlegroups[0] ?? null;
  $: pods = overview?.workloads.pods ?? [];
  $: selectedPodSummary = pods.find((pod) => pod.name === selectedPod);

  onMount(async () => {
    await loadSession();
    if (session) await refresh();
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
    } catch (err) {
      error = message(err);
    } finally {
      signingIn = false;
    }
  }

  async function logout() {
    await api("/api/auth/logout", { method: "POST" });
    session = null;
    overview = null;
    layout = null;
  }

  async function refresh(showError = true) {
    try {
      overview = await api<Overview>("/api/overview");
      if (!selectedPod && overview.workloads.pods[0]) selectedPod = overview.workloads.pods[0].name;
      if (battlegroup) {
        titleDraft = titleDraft || battlegroup.title;
        layout = await api<WorldLayout>(`/api/battlegroups/${battlegroup.namespace}/${battlegroup.name}/layout`);
      }
    } catch (err) {
      if (err instanceof ApiError && err.status === 401) session = null;
      if (showError) error = message(err);
    }
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

  function message(err: unknown) {
    return err instanceof Error ? err.message : "Operation failed.";
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
      {#each ["dashboard", "battlegroup", "layout", "players", "logs", "settings"] as item}
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
        </div>
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
          <label>Deep Desert PvE <input type="number" min="0" max="64" bind:value={layout.deepDesertPveInstances} /></label>
          <label>Deep Desert PvP <input type="number" min="0" max="64" bind:value={layout.deepDesertPvpInstances} /></label>
          <button on:click={saveLayout}>Apply layout</button>
          {#if layout.restartRequired}<p class="warn">Restart required for all changes to converge.</p>{/if}
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
