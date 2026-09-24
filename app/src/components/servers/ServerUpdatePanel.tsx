import { Flex, Text } from "@radix-ui/themes";

import type { RemoteServerRecord, RemoteServerStatus } from "../../types/server";
import { hasBattlegroupUpdateAvailable } from "../../utils/remote-server";
import ActionButton from "../ui/ActionButton";
import ServerPackageCardStatus from "./ServerPackageCardStatus";

export type ServerUpdatePanelProps = {
  server: RemoteServerRecord;
  status?: RemoteServerStatus;
  busyLabel?: string;
  onUpdateBattlegroup: () => void;
};

/**
 * Per-server Update sub-tab: package versions strip + Update Server action.
 *
 * The staged-vs-live comparison only sees builds that are already downloaded
 * on the host, so a freshly published Steam build still reads as "no update"
 * (#18). The vendor update downloads from Steam itself, so the action stays
 * available either way; it is only emphasised when a newer build is staged.
 */
export default function ServerUpdatePanel({
  server: _server,
  status,
  busyLabel,
  onUpdateBattlegroup,
}: ServerUpdatePanelProps) {
  const updateAvailable = hasBattlegroupUpdateAvailable(status?.package);
  const busy = !!busyLabel;
  return (
    <Flex direction="column" gap="4">
      <div>
        <div className="section-title">Package versions</div>
        {status?.package ? (
          <ServerPackageCardStatus guestPackage={status.package} />
        ) : (
          <Text size="2" style={{ color: "var(--color-text-muted)" }}>
            No package information yet. Refresh the server to fetch versions.
          </Text>
        )}
      </div>

      <div>
        <div className="section-title">Apply update</div>
        <Flex direction="column" gap="2">
          {updateAvailable ? (
            <Text size="2" style={{ color: "var(--color-text-secondary)" }}>
              A newer battlegroup version is downloaded on the host. Apply it to roll the
              running images.
            </Text>
          ) : (
            <Text size="2" style={{ color: "var(--color-text-muted)" }}>
              The downloaded battlegroup version matches what is currently running. If Funcom
              has published a newer build that is not downloaded yet, Update Server downloads
              it from Steam and applies it, which restarts the servers.
            </Text>
          )}
          <div>
            <ActionButton
              onClick={onUpdateBattlegroup}
              busy={busy}
              disabled={busy || !status}
              tone={updateAvailable ? "accent" : "default"}
              pendingLabel="Updating"
              title="Run vendor `battlegroup update` (steamcmd + operators + maps + images)"
            >
              Update Server
            </ActionButton>
          </div>
        </Flex>
      </div>
    </Flex>
  );
}
