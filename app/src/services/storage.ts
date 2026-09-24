import type { RemoteServerRecord } from "../types/server";
import type { CustomTunnelDef } from "../types/tunnel";
import type { ActivePage, ServerSubPage } from "../types/ui";
import { SERVER_SUB_PAGES } from "../types/ui";

const remoteServersStorageKey = "dune-manager.remote-servers";
const activePageStorageKey = "dune-manager.active-page";
const logSidebarStorageKey = "dune-manager.log-sidebar";
const usersAutoRefreshStorageKey = "dune-manager.users-auto-refresh";
const statusAutoRefreshStorageKey = "dune-manager.status-auto-refresh";

export function isRemoteServerRecord(value: unknown): value is RemoteServerRecord {
  if (!value || typeof value !== "object") return false;
  const record = value as Partial<RemoteServerRecord>;
  return (
    record.type === "ubuntu" &&
    typeof record.id === "string" &&
    typeof record.name === "string" &&
    typeof record.host === "string" &&
    typeof record.keyPath === "string"
  );
}

export function readRemoteServers(): RemoteServerRecord[] {
  const text = window.localStorage.getItem(remoteServersStorageKey);
  if (!text) return [];
  try {
    const parsed = JSON.parse(text);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isRemoteServerRecord);
  } catch {
    window.localStorage.removeItem(remoteServersStorageKey);
    return [];
  }
}

export function mergeRemoteServers(
  current: RemoteServerRecord[],
  incoming: RemoteServerRecord[],
): RemoteServerRecord[] {
  const byId = new Map(current.map((server) => [server.id, server]));
  for (const server of incoming) {
    byId.set(server.id, { ...byId.get(server.id), ...server });
  }
  return Array.from(byId.values()).sort((a, b) => a.name.localeCompare(b.name));
}

export function persistRemoteServers(servers: RemoteServerRecord[]): RemoteServerRecord[] {
  const unique = mergeRemoteServers([], servers);
  window.localStorage.setItem(remoteServersStorageKey, JSON.stringify(unique));
  return unique;
}

export function upsertRemoteServer(
  servers: RemoteServerRecord[],
  server: RemoteServerRecord,
): RemoteServerRecord[] {
  return mergeRemoteServers(servers, [server]);
}

type PersistedActivePage = { activeServerId?: string; activeSub?: ServerSubPage };

function isServerSubPage(value: unknown): value is ServerSubPage {
  return typeof value === "string" && (SERVER_SUB_PAGES as readonly string[]).includes(value);
}

export function readActivePage(attachedServerIds: string[]): ActivePage {
  const text = window.localStorage.getItem(activePageStorageKey);
  if (!text) return { kind: "servers" };
  try {
    const parsed = JSON.parse(text) as PersistedActivePage;
    const id = parsed?.activeServerId;
    if (!id || !attachedServerIds.includes(id)) return { kind: "servers" };
    const sub = isServerSubPage(parsed?.activeSub) ? parsed.activeSub : "dashboard";
    return { kind: "server", serverId: id, sub };
  } catch {
    window.localStorage.removeItem(activePageStorageKey);
    return { kind: "servers" };
  }
}

export function writeActivePage(page: ActivePage): void {
  if (page.kind === "servers") {
    window.localStorage.removeItem(activePageStorageKey);
    return;
  }
  const payload: PersistedActivePage = { activeServerId: page.serverId, activeSub: page.sub };
  window.localStorage.setItem(activePageStorageKey, JSON.stringify(payload));
}

type PersistedLogSidebar = { collapsed?: boolean; scopeToActiveServer?: boolean };

export function readLogSidebar(): PersistedLogSidebar {
  const text = window.localStorage.getItem(logSidebarStorageKey);
  if (!text) return {};
  try {
    const parsed = JSON.parse(text) as PersistedLogSidebar;
    return {
      collapsed: typeof parsed.collapsed === "boolean" ? parsed.collapsed : undefined,
      scopeToActiveServer:
        typeof parsed.scopeToActiveServer === "boolean" ? parsed.scopeToActiveServer : undefined,
    };
  } catch {
    window.localStorage.removeItem(logSidebarStorageKey);
    return {};
  }
}

export function writeLogSidebar(state: PersistedLogSidebar): void {
  window.localStorage.setItem(logSidebarStorageKey, JSON.stringify(state));
}

// Users tab auto-refresh switch. Defaults to on; an explicit "off" survives
// tab switches and app restarts (#25).
export function readUsersAutoRefresh(): boolean {
  return window.localStorage.getItem(usersAutoRefreshStorageKey) !== "off";
}

export function writeUsersAutoRefresh(enabled: boolean): void {
  window.localStorage.setItem(usersAutoRefreshStorageKey, enabled ? "on" : "off");
}

// Server status auto-refresh switch (#22). Defaults to on.
export function readStatusAutoRefresh(): boolean {
  return window.localStorage.getItem(statusAutoRefreshStorageKey) !== "off";
}

export function writeStatusAutoRefresh(enabled: boolean): void {
  window.localStorage.setItem(statusAutoRefreshStorageKey, enabled ? "on" : "off");
}

function customTunnelsKey(serverId: string): string {
  return `dune-manager.custom-tunnels.${serverId}`;
}

function isCustomTunnelDef(value: unknown): value is CustomTunnelDef {
  if (!value || typeof value !== "object") return false;
  const d = value as Partial<CustomTunnelDef>;
  return (
    typeof d.id === "string" &&
    typeof d.name === "string" &&
    (d.protocol === "http" || d.protocol === "https" || d.protocol === "postgresql") &&
    typeof d.remotePort === "number" &&
    typeof d.localPort === "number"
  );
}

export function readCustomTunnels(serverId: string): CustomTunnelDef[] {
  const text = window.localStorage.getItem(customTunnelsKey(serverId));
  if (!text) return [];
  try {
    const parsed = JSON.parse(text);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isCustomTunnelDef);
  } catch {
    window.localStorage.removeItem(customTunnelsKey(serverId));
    return [];
  }
}

export function writeCustomTunnels(serverId: string, defs: CustomTunnelDef[]): void {
  window.localStorage.setItem(customTunnelsKey(serverId), JSON.stringify(defs));
}
