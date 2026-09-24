import { Flex } from "@radix-ui/themes";

import type { RemoteDiskUsage, RemoteServerRecord, RemoteServerStatus } from "../../types/server";
import type { CustomTunnelStartRequest, ServerTunnelStartRequest, ServerTunnelStatus } from "../../types/tunnel";
import {
  isBattlegroupStarted,
  isDirectorReadyPhase,
  phaseTone,
  remoteServerDefaultUser,
} from "../../utils/remote-server";
import { formatBytes } from "../../utils/formatting";
import ActionButton from "../ui/ActionButton";
import Metric, { type MetricTone } from "../ui/Metric";
import ServerStatsTable from "./ServerStatsTable";
import ServerTunnelControls from "./ServerTunnelControls";
import CustomTunnelControls from "./CustomTunnelControls";
import ManagementServiceCard from "../management/ManagementServiceCard";
import type { ManagementStatusState } from "../management/useManagementStatus";
import type { LogRow } from "../../types/log";

export type ServerDashboardProps = {
  server: RemoteServerRecord;
  status?: RemoteServerStatus;
  statusError?: string;
  busyLabel?: string;
  tunnels: Record<string, ServerTunnelStatus>;
  tunnelBusy: Record<string, boolean>;
  managementStatus: ManagementStatusState;
  onRefreshManagement: () => Promise<void>;
  appendLogRow: (row: LogRow) => void;
  onStartBattlegroup: () => void;
  onStopBattlegroup: () => void;
  onRestartBattlegroup: () => void;
  onStartTunnel: (request: ServerTunnelStartRequest) => void;
  onStartCustomTunnel: (request: CustomTunnelStartRequest, name: string) => void;
  onStopTunnel: (tunnelId: string) => void;
  onOpenTunnel: (tunnel: ServerTunnelStatus) => void;
};

/**
 * Per-server Dashboard sub-tab: status hero metrics, per-map server-stats
 * table, lifecycle action row (start/stop/restart), and tunnel controls.
 */
export default function ServerDashboard({
  server,
  status,
  statusError,
  busyLabel,
  tunnels,
  tunnelBusy,
  managementStatus,
  onRefreshManagement,
  appendLogRow,
  onStartBattlegroup,
  onStopBattlegroup,
  onRestartBattlegroup,
  onStartTunnel,
  onStartCustomTunnel,
  onStopTunnel,
  onOpenTunnel,
}: ServerDashboardProps) {
  const liveStatus = statusError ? undefined : status;
  const battlegroup = liveStatus?.battlegroup;
  const battlegroupStarted = liveStatus ? isBattlegroupStarted(liveStatus.battlegroup) : false;
  const battlegroupStartRequested = liveStatus ? !liveStatus.battlegroup.stop : false;
  const battlegroupStopped = liveStatus ? liveStatus.battlegroup.stop : false;
  const directorReady = !!liveStatus && isDirectorReadyPhase(liveStatus.battlegroup.directorPhase);
  const busy = !!busyLabel;

  return (
    <Flex direction="column" gap="4">
      <div className="metric-grid">
        <Metric label="Namespace" value={server.namespace || ""} />
        <Metric label="BattleGroup" value={server.battlegroupName || ""} />
        <Metric
          label="Database"
          value={battlegroup?.databasePhase ?? ""}
          tone={battlegroup ? phaseTone(battlegroup.databasePhase ?? "") : "muted"}
        />
        <Metric
          label="Gateway"
          value={battlegroup?.gatewayPhase ?? ""}
          tone={battlegroup ? phaseTone(battlegroup.gatewayPhase ?? "") : "muted"}
        />
        <Metric
          label="Servers"
          value={battlegroup?.serverGroupPhase ?? ""}
          tone={battlegroup ? phaseTone(battlegroup.serverGroupPhase) : "muted"}
        />
        <Metric
          label="Director"
          value={battlegroup?.directorPhase ?? ""}
          tone={battlegroup ? phaseTone(battlegroup.directorPhase) : "muted"}
        />
        <Metric label="Uptime" value={battlegroup?.uptime ?? ""} />
        {(liveStatus?.disks ?? []).map((disk) => (
          <Metric
            key={disk.mount}
            label={`Disk ${disk.mount}`}
            value={diskValue(disk)}
            tone={diskTone(disk)}
          />
        ))}
      </div>

      {battlegroup?.serverStats && battlegroup.serverStats.length > 0 ? (
        <ServerStatsTable rows={battlegroup.serverStats} />
      ) : null}

      {statusError ? <div className="server-error">{statusError}</div> : null}

      <div className="action-row">
        {battlegroupStopped || !liveStatus ? (
          <ActionButton
            onClick={onStartBattlegroup}
            busy={busy && !battlegroupStarted}
            disabled={busy || !liveStatus || !battlegroupStopped}
            tone="accent"
            pendingLabel="Starting"
          >
            Start BattleGroup
          </ActionButton>
        ) : null}
        {battlegroupStartRequested ? (
          <>
            <ActionButton
              onClick={onRestartBattlegroup}
              busy={busy}
              disabled={busy || !liveStatus}
              tone="default"
              pendingLabel="Restarting"
            >
              Restart
            </ActionButton>
            <ActionButton
              onClick={onStopBattlegroup}
              busy={busy && battlegroupStartRequested && !battlegroupStopped}
              disabled={busy || !liveStatus}
              tone="danger"
              pendingLabel="Stopping"
            >
              Stop BattleGroup
            </ActionButton>
          </>
        ) : null}
      </div>

      <ManagementServiceCard
        server={server}
        status={managementStatus}
        onRefresh={onRefreshManagement}
        appendLogRow={appendLogRow}
      />

      <ServerTunnelControls
        serverKey={server.id}
        namespace={server.namespace}
        host={server.host}
        serverKind={server.type}
        user={server.user || remoteServerDefaultUser(server.type)}
        keyPath={server.keyPath}
        port={server.port}
        canStartDirectorTunnel={!!liveStatus && !liveStatus.battlegroup.stop && directorReady}
        canStartFileBrowserTunnel={!!liveStatus && !liveStatus.battlegroup.stop}
        canStartDatabaseTunnel={!!liveStatus && !liveStatus.battlegroup.stop}
        canStartPgHeroTunnel={!!liveStatus && !liveStatus.battlegroup.stop}
        tunnels={tunnels}
        tunnelBusy={tunnelBusy}
        onStartTunnel={onStartTunnel}
        onStopTunnel={onStopTunnel}
        onOpenTunnel={onOpenTunnel}
      />
      <CustomTunnelControls
        key={server.id}
        serverKey={server.id}
        host={server.host}
        serverKind={server.type}
        user={server.user || remoteServerDefaultUser(server.type)}
        keyPath={server.keyPath}
        port={server.port}
        tunnels={tunnels}
        tunnelBusy={tunnelBusy}
        onStartCustomTunnel={onStartCustomTunnel}
        onStopTunnel={onStopTunnel}
        onOpenTunnel={onOpenTunnel}
      />
    </Flex>
  );
}

// Same capacity figure `df` reports: used / (used + available). Reserved
// root blocks are excluded, so this is what actually runs out (#38).
function diskPercent(disk: RemoteDiskUsage): number {
  const usable = disk.usedKb + disk.availableKb;
  return usable > 0 ? Math.round((disk.usedKb / usable) * 100) : 0;
}

function diskValue(disk: RemoteDiskUsage): string {
  return `${diskPercent(disk)}%, ${formatBytes(disk.availableKb * 1024)} free`;
}

function diskTone(disk: RemoteDiskUsage): MetricTone {
  const percent = diskPercent(disk);
  if (percent >= 90) return "err";
  if (percent >= 80) return "warn";
  return "ok";
}
