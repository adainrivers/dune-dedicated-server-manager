import { Box, Flex, Theme } from "@radix-ui/themes";

import AppErrorBoundary from "./components/AppErrorBoundary";
import Header from "./components/layout/Header";
import LogWindow from "./components/logs/LogWindow";
import RemoteAttachDialog from "./components/dialogs/RemoteAttachDialog";
import RemoveRemoteServerDialog from "./components/dialogs/RemoveRemoteServerDialog";
import UpdateDialog from "./components/dialogs/UpdateDialog";
import ServerDetailPage from "./components/servers/ServerDetailPage";
import ServersListPage from "./components/servers/ServersListPage";
import { useAppUpdates } from "./hooks/useAppUpdates";
import { useComponentActions } from "./hooks/useComponentActions";
import { useOperationLogs } from "./hooks/useOperationLogs";
import { useRemoteServerStatus } from "./hooks/useRemoteServerStatus";
import { useRemoteServers } from "./hooks/useRemoteServers";
import { useServerTunnels } from "./hooks/useServerTunnels";
import { useActivePage } from "./hooks/useActivePage";
import { log } from "./utils/logging";

export function App() {
  const {
    logLevelFilter,
    setLogLevelFilter,
    logPanelCollapsed,
    setLogPanelCollapsed,
    scopeToActiveServer,
    setScopeToActiveServer,
    appendLogRow,
    clearLogRows,
    renderedLogRows,
  } = useOperationLogs();

  const remoteServersHook = useRemoteServers({ appendLogRow });

  const tunnels = useServerTunnels({ appendLogRow });

  const status = useRemoteServerStatus({
    appendLogRow,
    setRemoteServers: remoteServersHook.setRemoteServers,
  });

  const componentActions = useComponentActions({
    appendLogRow,
    detectRemoteServerDetails: status.detectRemoteServerDetails,
    setRemoteServerComponents: status.setRemoteServerComponents,
    setRemoteComponentLogs: status.setRemoteComponentLogs,
    setRemoteComponentLogBusy: status.setRemoteComponentLogBusy,
    setRemoteComponentRestartBusy: status.setRemoteComponentRestartBusy,
  });

  const updates = useAppUpdates({ appendLogRow });

  remoteServersHook.bindRefreshRemoteServerStatus(status.refreshRemoteServerStatus);
  remoteServersHook.bindRemoteServerBusy(status.remoteServerBusy);
  remoteServersHook.bindClearStatusForServer(status.clearStatusForServer);
  remoteServersHook.bindStopTunnelsForServer(tunnels.stopTunnelsForServer);

  const { activePage, openServer, openServersList, setSub } = useActivePage({
    remoteServers: remoteServersHook.remoteServers,
  });

  const scopeServerId = activePage.kind === "server" ? activePage.serverId : undefined;
  const visibleLogRows =
    scopeServerId && scopeToActiveServer
      ? renderedLogRows.filter((row) => !row.serverId || row.serverId === scopeServerId)
      : renderedLogRows;

  const activeServer =
    activePage.kind === "server"
      ? remoteServersHook.remoteServers.find((server) => server.id === activePage.serverId)
      : undefined;

  return (
    <Theme
      appearance="dark"
      accentColor="bronze"
      grayColor="sand"
      radius="medium"
      scaling="100%"
      panelBackground="solid"
    >
      <Flex direction="column" height="100vh" className="app-shell">
        <Header
          activePage={activePage}
          servers={remoteServersHook.remoteServers}
          statuses={status.remoteServerStatuses}
          statusErrors={status.remoteServerStatusErrors}
          busyMap={status.remoteServerBusy}
          onOpenServersList={openServersList}
          onOpenServer={openServer}
          onAddServer={() => remoteServersHook.setRemoteAttachOpen(true)}
          updateStatus={updates.updateStatus}
          update={updates.availableUpdate}
          updateProgress={updates.updateProgress}
          onCheckUpdate={updates.checkForAppUpdate}
          onOpenUpdate={() => updates.setUpdateDialogOpen(true)}
        />
        <Flex className="content-shell" gap="3" p="4" pt="0" minHeight="0">
          <Box className="main-pane">
            <AppErrorBoundary onError={(message) => appendLogRow(log.error("ui", message))}>
              {activePage.kind === "servers" || !activeServer ? (
                <ServersListPage
                  servers={remoteServersHook.remoteServers}
                  statuses={status.remoteServerStatuses}
                  statusErrors={status.remoteServerStatusErrors}
                  busyMap={status.remoteServerBusy}
                  onOpenServer={openServer}
                  onAddServer={() => remoteServersHook.setRemoteAttachOpen(true)}
                />
              ) : (
                <ServerDetailPage
                  server={activeServer}
                  sub={activePage.sub}
                  onSubChange={setSub}
                  status={status.remoteServerStatuses[activeServer.id]}
                  statusError={status.remoteServerStatusErrors[activeServer.id]}
                  busyLabel={status.remoteServerBusy[activeServer.id]}
                  components={status.remoteServerComponents[activeServer.id] ?? []}
                  componentLogs={status.remoteComponentLogs}
                  componentLogBusy={status.remoteComponentLogBusy}
                  componentRestartBusy={status.remoteComponentRestartBusy}
                  tunnels={tunnels.serverTunnels}
                  tunnelBusy={tunnels.serverTunnelBusy}
                  onRefresh={() => status.refreshRemoteServerStatus(activeServer)}
                  onPollStatus={() => status.pollRemoteServerStatus(activeServer)}
                  onRemove={() => remoteServersHook.setRemoteServerToRemove(activeServer)}
                  onStartBattlegroup={() => status.runRemoteBattlegroupAction(activeServer, "start")}
                  onStopBattlegroup={() => status.runRemoteBattlegroupAction(activeServer, "stop")}
                  onRestartBattlegroup={() =>
                    status.runRemoteBattlegroupAction(activeServer, "restart")
                  }
                  onUpdateBattlegroup={() => status.runRemoteBattlegroupAction(activeServer, "update")}
                  onStartTunnel={tunnels.startServerTunnel}
                  onStartCustomTunnel={tunnels.startCustomTunnel}
                  onStopTunnel={tunnels.stopServerTunnel}
                  onOpenTunnel={tunnels.openServerTunnel}
                  onRefreshComponentLog={(component) =>
                    componentActions.refreshRemoteComponentLog(activeServer, component)
                  }
                  onRestartComponent={(component) =>
                    componentActions.restartRemoteComponent(activeServer, component)
                  }
                  appendLogRow={appendLogRow}
                />
              )}
            </AppErrorBoundary>
          </Box>
          <LogWindow
            rows={visibleLogRows}
            level={logLevelFilter}
            collapsed={logPanelCollapsed}
            scopedToServer={scopeToActiveServer}
            canScopeToServer={!!scopeServerId}
            onLevelChange={setLogLevelFilter}
            onClear={clearLogRows}
            onToggleCollapsed={() => setLogPanelCollapsed((collapsed) => !collapsed)}
            onToggleScope={setScopeToActiveServer}
          />
        </Flex>
        <RemoteAttachDialog
          open={remoteServersHook.remoteAttachOpen}
          form={remoteServersHook.remoteAttachForm}
          running={remoteServersHook.remoteAttachRunning}
          errorMessage={remoteServersHook.remoteAttachError}
          preflight={remoteServersHook.remoteAttachPreflight}
          onOpenChange={(open) => {
            remoteServersHook.setRemoteAttachOpen(open);
            if (!open) remoteServersHook.setRemoteAttachError(null);
          }}
          onChange={remoteServersHook.setRemoteAttachForm}
          onAttach={remoteServersHook.addRemoteServer}
        />
        <RemoveRemoteServerDialog
          server={remoteServersHook.remoteServerToRemove}
          onOpenChange={(open) => {
            if (!open) remoteServersHook.setRemoteServerToRemove(null);
          }}
          onRemove={(server) => {
            remoteServersHook.removeRemoteServer(server);
            remoteServersHook.setRemoteServerToRemove(null);
          }}
        />
        <UpdateDialog
          open={updates.updateDialogOpen}
          update={updates.availableUpdate}
          status={updates.updateStatus}
          progress={updates.updateProgress}
          onOpenChange={updates.setUpdateDialogOpen}
          onInstall={updates.installAppUpdate}
        />
      </Flex>
    </Theme>
  );
}
