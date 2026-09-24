import { useCallback, useEffect, useState } from "react";
import {
  AlertDialog,
  Badge,
  Box,
  Button,
  Card,
  Dialog,
  Flex,
  Text,
} from "@radix-ui/themes";

import type { RemoteServerRecord } from "../../types/server";
import { managementService } from "../../services/management";
import {
  INSTALL_STEPS,
  type InstallProgressEvent,
} from "../../types/management";
import type { LogRow } from "../../types/log";
import { listenToEvent } from "../../services/tauri";
import { log } from "../../utils/logging";

import type { ManagementStatusState } from "./useManagementStatus";

export type ManagementServiceCardProps = {
  server: RemoteServerRecord;
  status: ManagementStatusState;
  onRefresh: () => Promise<void>;
  appendLogRow: (row: LogRow) => void;
};

type StepStatus = "pending" | "running" | "ok" | "error" | "skipped";

type InstallPhase =
  | { kind: "idle" }
  | { kind: "installing"; steps: Record<string, { status: StepStatus; message?: string }> }
  | { kind: "done"; ok: boolean; message?: string };

export default function ManagementServiceCard({
  server,
  status,
  onRefresh,
  appendLogRow,
}: ManagementServiceCardProps) {
  const [bundledVersion, setBundledVersion] = useState<string | null>(null);
  const [installOpen, setInstallOpen] = useState(false);
  const [phase, setPhase] = useState<InstallPhase>({ kind: "idle" });
  const [uninstallOpen, setUninstallOpen] = useState(false);
  const [uninstallBusy, setUninstallBusy] = useState(false);
  const [restartBusy, setRestartBusy] = useState(false);

  useEffect(() => {
    managementService.bundledVersion().then(setBundledVersion).catch(() => setBundledVersion(null));
  }, []);

  useEffect(() => {
    if (!installOpen) return;
    let unlisten: (() => void) | null = null;
    const promise = listenToEvent<InstallProgressEvent>("management-install-progress", (ev) => {
      setPhase((current) => {
        if (current.kind !== "installing") return current;
        const next = { ...current.steps };
        next[ev.step] = { status: ev.status as StepStatus, message: ev.message ?? undefined };
        return { kind: "installing", steps: next };
      });
      const stepLabel = INSTALL_STEPS.find((s) => s.id === ev.step)?.label ?? ev.step;
      const detail = ev.message ? ` (${ev.message})` : "";
      if (ev.status === "error") {
        appendLogRow(
          log.error("mgmt.install", `${stepLabel} failed${detail}`, server.id),
        );
      } else if (ev.status === "running") {
        appendLogRow(log.info("mgmt.install", `${stepLabel} started${detail}`, server.id));
      } else if (ev.status === "ok") {
        appendLogRow(log.info("mgmt.install", `${stepLabel} ok${detail}`, server.id));
      }
    });
    promise.then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, [installOpen, appendLogRow, server.id]);

  const installed = status.kind === "ok" ? status.value.installed : false;
  const active = status.kind === "ok" ? status.value.active : false;
  const installedVersion = status.kind === "ok" ? status.value.installedVersion : null;
  const remoteBundled = status.kind === "ok" ? status.value.bundledVersion : null;
  const effectiveBundled = remoteBundled ?? bundledVersion;
  const updateAvailable =
    installed && !!installedVersion && !!effectiveBundled && installedVersion !== effectiveBundled;

  const startInstall = useCallback(async () => {
    const initial: Record<string, { status: StepStatus; message?: string }> = {};
    for (const step of INSTALL_STEPS) {
      initial[step.id] = { status: "pending" };
    }
    setPhase({ kind: "installing", steps: initial });
    appendLogRow(
      log.info(
        "mgmt.install",
        `${installed ? "Updating" : "Installing"} dune-server-service on ${server.host}…`,
        server.id,
      ),
    );
    try {
      const result = await managementService.install({
        host: server.host,
        user: server.user,
        keyPath: server.keyPath,
        port: server.port,
      });
      setPhase({
        kind: "done",
        ok: result.started,
        message: result.message,
      });
      appendLogRow(
        log.info(
          "mgmt.install",
          `${installed ? "Update" : "Install"} ${result.started ? "succeeded" : "completed but service not active"}${result.installedVersion ? ` (v${result.installedVersion})` : ""}.`,
          server.id,
        ),
      );
      await onRefresh();
    } catch (err) {
      const message = String(err);
      setPhase({ kind: "done", ok: false, message });
      appendLogRow(
        log.error("mgmt.install", `Install failed: ${message}`, server.id),
      );
    }
  }, [onRefresh, server.host, server.id, server.keyPath, server.port, server.user, installed, appendLogRow]);

  const closeInstall = useCallback(() => {
    setInstallOpen(false);
    setPhase({ kind: "idle" });
  }, []);

  const handleRestart = useCallback(async () => {
    setRestartBusy(true);
    appendLogRow(
      log.info("mgmt.restart", `Restarting dune-server-service on ${server.host}…`, server.id),
    );
    try {
      await managementService.restart({
        host: server.host,
        user: server.user,
        keyPath: server.keyPath,
        port: server.port,
      });
      appendLogRow(log.info("mgmt.restart", `Restart issued on ${server.host}.`, server.id));
      // systemd needs a moment to come back up before /api/health responds again.
      setTimeout(() => void onRefresh(), 1500);
    } catch (err) {
      const message = String(err);
      appendLogRow(log.error("mgmt.restart", `Restart failed: ${message}`, server.id));
      alert(`Restart failed: ${message}`);
    } finally {
      setRestartBusy(false);
    }
  }, [server, appendLogRow, onRefresh]);

  const handleUninstall = useCallback(async () => {
    setUninstallBusy(true);
    appendLogRow(
      log.info("mgmt.uninstall", `Uninstalling dune-server-service from ${server.host}…`, server.id),
    );
    try {
      await managementService.uninstall({
        host: server.host,
        user: server.user,
        keyPath: server.keyPath,
        port: server.port,
      });
      setUninstallOpen(false);
      appendLogRow(log.info("mgmt.uninstall", `Uninstalled from ${server.host}.`, server.id));
      await onRefresh();
    } catch (err) {
      const message = String(err);
      appendLogRow(log.error("mgmt.uninstall", `Uninstall failed: ${message}`, server.id));
      alert(`Uninstall failed: ${message}`);
    } finally {
      setUninstallBusy(false);
    }
  }, [onRefresh, server, appendLogRow]);

  const showInstallButton = !installed;
  const showUpdateButton = installed && updateAvailable;
  const installInProgress = phase.kind === "installing";

  return (
    <Card mt="3">
      <Flex justify="between" align="start" gap="3" wrap="wrap">
        <Box>
          <Text size="3" weight="medium">
            Management service
          </Text>
          <Flex align="center" gap="2" mt="1" wrap="wrap">
            <Badge color={installed ? (active ? "green" : "amber") : "gray"}>
              {status.kind === "loading"
                ? "checking..."
                : installed
                  ? active
                    ? `active${installedVersion ? ` ${installedVersion}` : ""}`
                    : `installed, not running${installedVersion ? ` (${installedVersion})` : ""}`
                  : "not installed"}
            </Badge>
            {status.kind === "ok" && status.value.initSystem ? (
              <Badge color="gray" variant="surface">
                {status.value.initSystem}
              </Badge>
            ) : null}
            {updateAvailable ? (
              <Badge color="amber" variant="soft">
                update available: {installedVersion} → {effectiveBundled}
              </Badge>
            ) : installed && active && installedVersion && effectiveBundled && !updateAvailable ? (
              <Text size="1" color="gray">
                Up to date
              </Text>
            ) : null}
            {status.kind === "error" ? (
              <Text size="1" color="red">
                {status.message}
              </Text>
            ) : null}
          </Flex>
        </Box>
        <Flex gap="2" wrap="wrap">
          <Button size="1" variant="surface" onClick={() => void onRefresh()}>
            Refresh
          </Button>
          {showInstallButton ? (
            <Button size="1" variant="solid" onClick={() => setInstallOpen(true)}>
              Install
            </Button>
          ) : null}
          {showUpdateButton ? (
            <Button size="1" variant="solid" color="amber" onClick={() => setInstallOpen(true)}>
              Update
            </Button>
          ) : null}
          {installed ? (
            <Button
              size="1"
              variant="surface"
              onClick={handleRestart}
              disabled={restartBusy}
            >
              {restartBusy ? "Restarting…" : "Restart"}
            </Button>
          ) : null}
          {installed ? (
            <Button size="1" variant="surface" color="red" onClick={() => setUninstallOpen(true)}>
              Uninstall
            </Button>
          ) : null}
        </Flex>
      </Flex>

      <Dialog.Root
        open={installOpen}
        onOpenChange={(open) => {
          if (!open && !installInProgress) closeInstall();
        }}
      >
        <Dialog.Content maxWidth="540px">
          <Dialog.Title>{installed ? "Update management service" : "Install management service"}</Dialog.Title>
          <Dialog.Description size="2" mb="3" color="gray">
            Uploads the bundled dune-server-service binary to{" "}
            <Text className="mono">/opt/dune-server-service/</Text>, installs the unit, and starts the service.
          </Dialog.Description>

          {phase.kind === "idle" ? (
            <Text size="2" color="gray">
              Ready to {installed ? "update" : "install"}. Click {installed ? '"Update"' : '"Install"'} to begin.
            </Text>
          ) : (
            <Box className="install-step-list">
              {INSTALL_STEPS.map((step) => {
                const s =
                  phase.kind === "installing"
                    ? phase.steps[step.id]?.status ?? "pending"
                    : "ok";
                const msg =
                  phase.kind === "installing" ? phase.steps[step.id]?.message : undefined;
                return (
                  <Flex key={step.id} align="center" gap="2" className="install-step-row">
                    <span className={`install-step-icon install-step-${s}`} aria-hidden>
                      {iconFor(s)}
                    </span>
                    <Text size="2">{step.label}</Text>
                    {msg ? (
                      <Text size="1" color="gray">
                        — {msg}
                      </Text>
                    ) : null}
                  </Flex>
                );
              })}
              {phase.kind === "done" && !phase.ok ? (
                <Text size="1" color="red" mt="2">
                  {phase.message ?? "Install failed."}
                </Text>
              ) : null}
              {phase.kind === "done" && phase.ok ? (
                <Text size="1" color="green" mt="2">
                  {phase.message ?? "Install complete."}
                </Text>
              ) : null}
            </Box>
          )}

          <Flex gap="2" mt="4" justify="end">
            {phase.kind === "idle" ? (
              <>
                <Dialog.Close>
                  <Button variant="soft" color="gray">
                    Cancel
                  </Button>
                </Dialog.Close>
                <Button onClick={startInstall}>{installed ? "Update" : "Install"}</Button>
              </>
            ) : phase.kind === "done" ? (
              <Button onClick={closeInstall}>Close</Button>
            ) : (
              <Button disabled>Installing…</Button>
            )}
          </Flex>
        </Dialog.Content>
      </Dialog.Root>

      <AlertDialog.Root open={uninstallOpen} onOpenChange={setUninstallOpen}>
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>Uninstall management service?</AlertDialog.Title>
          <AlertDialog.Description size="2">
            Stops and removes <Text className="mono">dune-server-service</Text> and its unit file from the host.
            Its settings, run history, and welcome-package ledger stay in the service account&apos;s{" "}
            <Text className="mono">~/.dune/state</Text>, so reinstalling picks them up again.
          </AlertDialog.Description>
          <Flex gap="2" mt="4" justify="end">
            <AlertDialog.Cancel>
              <Button variant="soft" color="gray" disabled={uninstallBusy}>
                Cancel
              </Button>
            </AlertDialog.Cancel>
            <Button color="red" onClick={handleUninstall} disabled={uninstallBusy}>
              {uninstallBusy ? "Uninstalling…" : "Uninstall"}
            </Button>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </Card>
  );
}

function iconFor(status: StepStatus): string {
  switch (status) {
    case "ok":
      return "OK";
    case "error":
      return "X";
    case "running":
      return "…";
    case "skipped":
      return "-";
    default:
      return " ";
  }
}
