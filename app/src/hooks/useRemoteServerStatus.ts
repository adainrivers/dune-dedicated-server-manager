import { useRef, useState } from "react";

import {
  detectRemoteUbuntuServers,
  getRemoteServerComponents,
  getRemoteServerStatus,
  restartRemoteBattlegroup,
  startRemoteBattlegroup,
  stopRemoteBattlegroup,
  updateRemoteBattlegroup,
} from "../services/tauri";
import { persistRemoteServers, upsertRemoteServer } from "../services/storage";
import type { LogRow } from "../types/log";
import type {
  RemoteBattlegroupStatus,
  RemoteServerComponent,
  RemoteServerRecord,
  RemoteServerStatus,
} from "../types/server";
import { errorMessage } from "../utils/errors";
import { log } from "../utils/logging";
import {
  omitKey,
  omitPrefix,
  remoteServerActionRequest,
  remoteServerDefaultUser,
} from "../utils/remote-server";

type UseRemoteServerStatusArgs = {
  appendLogRow: (row: LogRow) => void;
  setRemoteServers: React.Dispatch<React.SetStateAction<RemoteServerRecord[]>>;
};

export function useRemoteServerStatus({ appendLogRow, setRemoteServers }: UseRemoteServerStatusArgs) {
  const [remoteServerStatuses, setRemoteServerStatuses] = useState<Record<string, RemoteServerStatus>>({});
  const [remoteServerComponents, setRemoteServerComponents] = useState<Record<string, RemoteServerComponent[]>>({});
  const [remoteServerStatusErrors, setRemoteServerStatusErrors] = useState<Record<string, string>>({});
  const [remoteServerBusy, setRemoteServerBusy] = useState<Record<string, string>>({});
  const [remoteComponentLogs, setRemoteComponentLogs] = useState<Record<string, string>>({});
  const [remoteComponentLogBusy, setRemoteComponentLogBusy] = useState<Record<string, boolean>>({});
  const [remoteComponentRestartBusy, setRemoteComponentRestartBusy] = useState<Record<string, boolean>>({});
  // Bumped when a full refresh or a BattleGroup action starts, so a quiet poll
  // that was already in flight cannot overwrite the newer result (#22).
  const statusEpoch = useRef<Record<string, number>>({});
  const bumpStatusEpoch = (serverId: string) => {
    statusEpoch.current[serverId] = (statusEpoch.current[serverId] ?? 0) + 1;
  };

  const detectRemoteServerDetails = async (server: RemoteServerRecord): Promise<RemoteServerRecord> => {
    const detected = await detectRemoteUbuntuServers({
      host: server.host,
      keyPath: server.keyPath,
      serverType: "ubuntu",
      user: server.user || remoteServerDefaultUser(server.type),
      port: server.port,
    });
    if (detected.length === 0) {
      throw new Error("No Dune battlegroups were detected on the remote server.");
    }
    return detected.find((candidate) => candidate.battlegroupName === server.battlegroupName) ?? detected[0];
  };

  const refreshRemoteServerStatus = async (server: RemoteServerRecord) => {
    if (!server.host || !server.keyPath) return;
    bumpStatusEpoch(server.id);
    setRemoteServerBusy((busy) => ({ ...busy, [server.id]: "Retrieving server information" }));
    setRemoteServerStatuses((statuses) => omitKey(statuses, server.id));
    setRemoteServerComponents((components) => omitKey(components, server.id));
    setRemoteComponentLogs((logs) => omitPrefix(logs, `${server.id}:`));
    setRemoteComponentLogBusy((busy) => omitPrefix(busy, `${server.id}:`));
    setRemoteComponentRestartBusy((busy) => omitPrefix(busy, `${server.id}:`));
    setRemoteServerStatusErrors((errors) => omitKey(errors, server.id));
    try {
      const liveServer = await detectRemoteServerDetails(server);
      setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, liveServer)));
      const status = await getRemoteServerStatus(remoteServerActionRequest(liveServer));
      const components = await getRemoteServerComponents(remoteServerActionRequest(liveServer));
      setRemoteServerStatuses((statuses) => ({ ...statuses, [liveServer.id]: status }));
      setRemoteServerComponents((current) => ({ ...current, [liveServer.id]: components }));
      setRemoteServerStatusErrors((errors) => omitKey(errors, liveServer.id));
      setRemoteServers((servers) =>
        persistRemoteServers(
          servers.map((candidate) =>
            candidate.id === liveServer.id
              ? { ...liveServer, phase: status.battlegroup.phase || liveServer.phase }
              : candidate,
          ),
        ),
      );
      appendLogRow(
        log.info(
          "remote.status",
          buildStatusLogLine(liveServer.name, status.battlegroup),
          liveServer.id,
        ),
      );
    } catch (err) {
      const message = errorMessage(err);
      setRemoteServerStatuses((statuses) => omitKey(statuses, server.id));
      setRemoteServerComponents((components) => omitKey(components, server.id));
      setRemoteComponentLogs((logs) => omitPrefix(logs, `${server.id}:`));
      setRemoteServerStatusErrors((errors) => ({ ...errors, [server.id]: message }));
      appendLogRow(log.warn("remote.status", message, server.id));
    } finally {
      setRemoteServerBusy((busy) => omitKey(busy, server.id));
    }
  };

  const runRemoteBattlegroupAction = async (
    server: RemoteServerRecord,
    action: "start" | "stop" | "restart" | "update",
  ) => {
    const verbs: Record<typeof action, [busy: string, log: string]> = {
      start: ["Starting battlegroup", "Starting"],
      stop: ["Stopping battlegroup", "Stopping"],
      restart: ["Restarting battlegroup", "Restarting"],
      update: ["Updating battlegroup", "Updating"],
    };
    const [busyText, verb] = verbs[action];
    bumpStatusEpoch(server.id);
    setRemoteServerBusy((busy) => ({ ...busy, [server.id]: busyText }));
    appendLogRow(log.info("bg", `${verb} remote battlegroup.`, server.id));
    try {
      const liveServer =
        server.namespace && server.battlegroupName ? server : await detectRemoteServerDetails(server);
      setRemoteServers((servers) => persistRemoteServers(upsertRemoteServer(servers, liveServer)));
      const request = remoteServerActionRequest(liveServer);
      const status =
        action === "start"
          ? await startRemoteBattlegroup(request)
          : action === "stop"
            ? await stopRemoteBattlegroup(request)
            : action === "restart"
              ? await restartRemoteBattlegroup(request)
              : await updateRemoteBattlegroup(request);
      const components = await getRemoteServerComponents(request);
      setRemoteServerStatuses((statuses) => ({ ...statuses, [liveServer.id]: status }));
      setRemoteServerComponents((current) => ({ ...current, [liveServer.id]: components }));
      setRemoteServerStatusErrors((errors) => omitKey(errors, liveServer.id));
      setRemoteServers((servers) =>
        persistRemoteServers(
          servers.map((candidate) =>
            candidate.id === liveServer.id
              ? { ...liveServer, phase: status.battlegroup.phase || liveServer.phase }
              : candidate,
          ),
        ),
      );
    } catch (err) {
      const message = errorMessage(err);
      setRemoteServerStatusErrors((errors) => ({ ...errors, [server.id]: message }));
      appendLogRow(log.error("bg", message, server.id));
    } finally {
      setRemoteServerBusy((busy) => omitKey(busy, server.id));
    }
  };

  // Quiet status-only refresh for auto-refresh (#22). Unlike the full refresh
  // it keeps the current view, sets no busy label, and writes no log line.
  // Failures keep the last known state; a manual Refresh surfaces the error.
  const pollRemoteServerStatus = async (server: RemoteServerRecord) => {
    if (!server.host || !server.keyPath || !server.namespace || !server.battlegroupName) return;
    const epoch = statusEpoch.current[server.id] ?? 0;
    try {
      const status = await getRemoteServerStatus(remoteServerActionRequest(server));
      if ((statusEpoch.current[server.id] ?? 0) !== epoch) return;
      setRemoteServerStatuses((statuses) => ({ ...statuses, [server.id]: status }));
      setRemoteServerStatusErrors((errors) => omitKey(errors, server.id));
    } catch {
      // Keep the last known state.
    }
  };

  const clearStatusForServer = (serverId: string) => {
    setRemoteServerStatuses((statuses) => omitKey(statuses, serverId));
    setRemoteServerComponents((components) => omitKey(components, serverId));
    setRemoteServerStatusErrors((errors) => omitKey(errors, serverId));
    setRemoteComponentLogs((logs) => omitPrefix(logs, `${serverId}:`));
    setRemoteComponentLogBusy((busy) => omitPrefix(busy, `${serverId}:`));
    setRemoteComponentRestartBusy((busy) => omitPrefix(busy, `${serverId}:`));
  };

  return {
    remoteServerStatuses,
    remoteServerComponents,
    setRemoteServerComponents,
    remoteServerStatusErrors,
    remoteServerBusy,
    remoteComponentLogs,
    setRemoteComponentLogs,
    remoteComponentLogBusy,
    setRemoteComponentLogBusy,
    remoteComponentRestartBusy,
    setRemoteComponentRestartBusy,
    detectRemoteServerDetails,
    refreshRemoteServerStatus,
    pollRemoteServerStatus,
    runRemoteBattlegroupAction,
    clearStatusForServer,
  };
}

function buildStatusLogLine(name: string, bg: RemoteBattlegroupStatus): string {
  const parts: string[] = [
    `${name}: ${bg.phase || "unknown"}`,
    `server group ${bg.serverGroupPhase || "unknown"}`,
  ];
  if (bg.databasePhase) parts.push(`DB ${bg.databasePhase}`);
  parts.push(`Director ${bg.directorPhase || "unknown"}`);
  if (bg.uptime) parts.push(`up ${bg.uptime}`);
  if (bg.stop) parts.push("STOP");
  return parts.join(", ") + ".";
}
