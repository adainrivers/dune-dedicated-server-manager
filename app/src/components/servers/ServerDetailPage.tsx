import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Box, Flex, Tabs, Text } from "@radix-ui/themes";

import type {
  RemoteServerComponent,
  RemoteServerRecord,
  RemoteServerStatus,
} from "../../types/server";
import type { LogRow } from "../../types/log";
import type { CustomTunnelStartRequest, ServerTunnelStartRequest, ServerTunnelStatus } from "../../types/tunnel";
import type { ServerSubPage } from "../../types/ui";
import { isManagementSubPage } from "../../types/ui";
import {
  isBattlegroupStarted,
  remoteServerDefaultUser,
  resolveServerStatus,
} from "../../utils/remote-server";
import ActionButton from "../ui/ActionButton";
import StatusPill from "../ui/StatusPill";
import ServerDashboard from "./ServerDashboard";
import ServerPods from "./ServerPods";
import ServerUpdatePanel from "./ServerUpdatePanel";
import AdminTab, { type AdminTabPrefill } from "../management/AdminTab";
import AutomatedTasksTab from "../management/AutomatedTasksTab";
import UsersTab from "../management/UsersTab";
import WelcomePackageTab from "../management/WelcomePackageTab";
import { isManagementReady, useManagementStatus } from "../management/useManagementStatus";
import { useManagementTunnel } from "../management/useManagementTunnel";

export type ServerDetailPageProps = {
  server: RemoteServerRecord;
  sub: ServerSubPage;
  onSubChange: (sub: ServerSubPage) => void;
  status?: RemoteServerStatus;
  statusError?: string;
  busyLabel?: string;
  components: RemoteServerComponent[];
  componentLogs: Record<string, string>;
  componentLogBusy: Record<string, boolean>;
  componentRestartBusy: Record<string, boolean>;
  tunnels: Record<string, ServerTunnelStatus>;
  tunnelBusy: Record<string, boolean>;
  onRefresh: () => void;
  onRemove: () => void;
  onStartBattlegroup: () => void;
  onStopBattlegroup: () => void;
  onRestartBattlegroup: () => void;
  onUpdateBattlegroup: () => void;
  onStartTunnel: (request: ServerTunnelStartRequest) => void;
  onStartCustomTunnel: (request: CustomTunnelStartRequest, name: string) => void;
  onStopTunnel: (tunnelId: string) => void;
  onOpenTunnel: (tunnel: ServerTunnelStatus) => void;
  onRefreshComponentLog: (component: RemoteServerComponent) => void;
  onRestartComponent: (component: RemoteServerComponent) => void;
  appendLogRow: (row: LogRow) => void;
};

export default function ServerDetailPage(props: ServerDetailPageProps) {
  const {
    server,
    sub,
    onSubChange,
    status,
    statusError,
    busyLabel,
    components,
    componentLogs,
    componentLogBusy,
    componentRestartBusy,
    tunnels,
    tunnelBusy,
    onRefresh,
    onRemove,
    onStartBattlegroup,
    onStopBattlegroup,
    onRestartBattlegroup,
    onUpdateBattlegroup,
    onStartTunnel,
    onStartCustomTunnel,
    onStopTunnel,
    onOpenTunnel,
    onRefreshComponentLog,
    onRestartComponent,
    appendLogRow,
  } = props;
  const busy = !!busyLabel;
  const liveStatus = statusError ? undefined : status;
  const resolved = resolveServerStatus(statusError, liveStatus, busy, server);

  const management = useManagementStatus(server, appendLogRow);
  const managementReady = isManagementReady(management.state);
  const tunnelState = useManagementTunnel(server, managementReady);
  const tunnelId = tunnelState.kind === "ready" ? tunnelState.tunnelId : null;
  const [adminPrefill, setAdminPrefill] = useState<AdminTabPrefill>(null);

  const goToAdmin = useCallback(
    (prefill: AdminTabPrefill) => {
      setAdminPrefill(prefill);
      onSubChange("admin");
    },
    [onSubChange],
  );

  // If management goes away (uninstalled / unreachable) while a management
  // sub-page is active, bounce the user back to the dashboard.
  useEffect(() => {
    if (!managementReady && isManagementSubPage(sub)) {
      onSubChange("dashboard");
    }
  }, [managementReady, sub, onSubChange]);

  return (
    <Box className="pane page-pane">
      <Flex direction="column" gap="4" height="100%" minHeight="0" p="4">
        <div className="server-detail-hero" data-tone={resolved.tone}>
          <div className="server-detail-hero-rail" />
          <Flex direction="column" gap="1" minWidth="0">
            <Flex align="center" gap="3" wrap="wrap">
              <span className="server-name">{server.name}</span>
              <StatusPill label={resolved.label} tone={resolved.tone} pulse={resolved.pulse} />
              {busyLabel ? <span className="app-title-sub">{busyLabel}</span> : null}
            </Flex>
            <span className="server-host">
              {server.user || remoteServerDefaultUser(server.type)}@{server.host}
              {server.battlegroupName ? ` · ${server.battlegroupName}` : ""}
              {liveStatus?.battlegroup.uptime ? ` · up ${liveStatus.battlegroup.uptime}` : ""}
            </span>
          </Flex>
          <Flex align="center" gap="2">
            <ActionButton onClick={onRefresh} busy={busy} pendingLabel="Refreshing">
              Refresh
            </ActionButton>
            <ActionButton onClick={onRemove} tone="danger" disabled={busy}>
              Forget
            </ActionButton>
          </Flex>
        </div>

        <Tabs.Root
          className="server-detail-tabs"
          value={sub}
          onValueChange={(value) => onSubChange(value as ServerSubPage)}
        >
          <Tabs.List size="2" color="bronze">
            <Tabs.Trigger value="dashboard">Dashboard</Tabs.Trigger>
            <Tabs.Trigger value="update">Update</Tabs.Trigger>
            <Tabs.Trigger value="pods">Pods</Tabs.Trigger>
            {managementReady ? (
              <>
                <Tabs.Trigger value="users">Users</Tabs.Trigger>
                <Tabs.Trigger value="admin">Admin</Tabs.Trigger>
                <Tabs.Trigger value="welcome">Welcome Package</Tabs.Trigger>
                <Tabs.Trigger value="tasks">Automated tasks</Tabs.Trigger>
              </>
            ) : null}
          </Tabs.List>

          <Tabs.Content value="dashboard" className="server-detail-tab-content">
            <ServerDashboard
              server={server}
              status={liveStatus}
              statusError={statusError}
              busyLabel={busyLabel}
              tunnels={tunnels}
              tunnelBusy={tunnelBusy}
              managementStatus={management.state}
              onRefreshManagement={management.refresh}
              appendLogRow={appendLogRow}
              onStartBattlegroup={onStartBattlegroup}
              onStopBattlegroup={onStopBattlegroup}
              onRestartBattlegroup={onRestartBattlegroup}
              onStartTunnel={onStartTunnel}
              onStartCustomTunnel={onStartCustomTunnel}
              onStopTunnel={onStopTunnel}
              onOpenTunnel={onOpenTunnel}
            />
          </Tabs.Content>
          <Tabs.Content value="update" className="server-detail-tab-content">
            <ServerUpdatePanel
              server={server}
              status={liveStatus}
              busyLabel={busyLabel}
              onUpdateBattlegroup={onUpdateBattlegroup}
            />
          </Tabs.Content>
          <Tabs.Content value="pods" className="server-detail-tab-content">
            <ServerPods
              serverKey={server.id}
              components={liveStatus ? components : []}
              logs={componentLogs}
              logBusy={componentLogBusy}
              restartBusy={componentRestartBusy}
              onRefreshLog={onRefreshComponentLog}
              onRestart={onRestartComponent}
            />
          </Tabs.Content>
          {managementReady ? (
            <>
              <Tabs.Content value="users" className="server-detail-tab-content">
                <ManagementContent tunnelState={tunnelState} tunnelId={tunnelId}>
                  {(id) => (
                    <UsersTab
                      tunnelId={id}
                      serverReachable={
                        !!liveStatus && isBattlegroupStarted(liveStatus.battlegroup)
                      }
                      onSwitchToAdmin={goToAdmin}
                    />
                  )}
                </ManagementContent>
              </Tabs.Content>
              <Tabs.Content value="admin" className="server-detail-tab-content">
                <ManagementContent tunnelState={tunnelState} tunnelId={tunnelId}>
                  {(id) => (
                    <AdminTab
                      tunnelId={id}
                      prefill={adminPrefill}
                      onPrefillConsumed={() => setAdminPrefill(null)}
                    />
                  )}
                </ManagementContent>
              </Tabs.Content>
              <Tabs.Content value="tasks" className="server-detail-tab-content">
                <ManagementContent tunnelState={tunnelState} tunnelId={tunnelId}>
                  {(id) => (
                    <AutomatedTasksTab
                      tunnelId={id}
                      server={server}
                      onAfterRestart={management.refresh}
                    />
                  )}
                </ManagementContent>
              </Tabs.Content>
              <Tabs.Content value="welcome" className="server-detail-tab-content">
                <ManagementContent tunnelState={tunnelState} tunnelId={tunnelId}>
                  {(id) => (
                    <WelcomePackageTab
                      tunnelId={id}
                      server={server}
                      onAfterRestart={management.refresh}
                    />
                  )}
                </ManagementContent>
              </Tabs.Content>
            </>
          ) : null}
        </Tabs.Root>
      </Flex>
    </Box>
  );
}

function ManagementContent({
  tunnelState,
  tunnelId,
  children,
}: {
  tunnelState: ReturnType<typeof useManagementTunnel>;
  tunnelId: string | null;
  children: (tunnelId: string) => ReactNode;
}) {
  if (tunnelState.kind === "error") {
    return (
      <Box p="4">
        <Text color="red">Could not open tunnel: {tunnelState.message}</Text>
      </Box>
    );
  }
  if (!tunnelId || tunnelState.kind !== "ready") {
    return (
      <Box p="4">
        <Text color="gray">Opening tunnel to management service…</Text>
      </Box>
    );
  }
  return <>{children(tunnelId)}</>;
}
