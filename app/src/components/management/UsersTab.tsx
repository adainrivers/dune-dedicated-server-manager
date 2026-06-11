import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Badge,
  Box,
  Button,
  DropdownMenu,
  Flex,
  Switch,
  Table,
  Text,
  TextField,
} from "@radix-ui/themes";

import { managementApi } from "../../services/management";
import type { PlayerDto } from "../../types/management";
import { copyTextToClipboard } from "../../utils/clipboard";
import { formatDateTime } from "../../utils/formatting";

import type { AdminTabPrefill } from "./AdminTab";

// The service sends last-seen as a UTC wall-clock string with no offset
// ("YYYY-MM-DD HH:MM:SS"). Tag it as UTC so it localizes instead of being
// parsed as the viewer's local time, then render in their timezone.
function formatLastSeen(raw: string): string {
  const s = raw.trim();
  if (!s) return "—";
  return formatDateTime(`${s.replace(" ", "T")}Z`);
}

export type UsersTabProps = {
  tunnelId: string;
  /**
   * Whether the BattleGroup is up and the player query can succeed. When false
   * (BG stopped/offline) we stop the initial load, debounced search, and the
   * auto-refresh poll — otherwise each poll hangs on an unavailable endpoint
   * (up to the tunnel timeout) and stacks up, freezing the screen (#25).
   */
  serverReachable: boolean;
  onSwitchToAdmin: (prefill: AdminTabPrefill) => void;
};

export default function UsersTab({
  tunnelId,
  serverReachable,
  onSwitchToAdmin,
}: UsersTabProps) {
  const [users, setUsers] = useState<PlayerDto[]>([]);
  const [query, setQuery] = useState("");
  const [onlineOnly, setOnlineOnly] = useState(false);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(
    async (q: string) => {
      setBusy(true);
      setError(null);
      try {
        const rows = await managementApi.searchPlayers(tunnelId, q, 200);
        setUsers(rows);
      } catch (err) {
        setError(String(err));
      } finally {
        setBusy(false);
      }
    },
    [tunnelId],
  );

  useEffect(() => {
    if (!serverReachable) return;
    void reload("");
  }, [reload, serverReachable]);

  useEffect(() => {
    if (!serverReachable) return;
    const handle = setTimeout(() => {
      void reload(query.trim());
    }, 300);
    return () => clearTimeout(handle);
  }, [query, reload, serverReachable]);

  // Poll for live player-status changes. Without this the list only refreshed
  // on mount / manual click, so logins and logouts went unseen until the app
  // was reopened (#13). Toggleable per #14; on by default. Gated on
  // serverReachable so a stopped BattleGroup doesn't get polled (#25).
  useEffect(() => {
    if (!autoRefresh || !serverReachable) return;
    const handle = setInterval(() => {
      void reload(query.trim());
    }, 5000);
    return () => clearInterval(handle);
  }, [autoRefresh, query, reload, serverReachable]);

  const visible = useMemo(
    () => (onlineOnly ? users.filter((u) => u.online.toLowerCase() === "online") : users),
    [users, onlineOnly],
  );

  return (
    <Box mt="3">
      <Flex gap="3" align="center" wrap="wrap" mb="3">
        <TextField.Root
          placeholder="Search name or FLS id…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          disabled={!serverReachable}
          style={{ flex: "1 1 280px", minWidth: 0 }}
        />
        <Flex align="center" gap="2">
          <Switch checked={onlineOnly} onCheckedChange={setOnlineOnly} />
          <Text size="2">Online only</Text>
        </Flex>
        <Flex align="center" gap="2">
          <Switch checked={autoRefresh} onCheckedChange={setAutoRefresh} />
          <Text size="2">Auto-refresh</Text>
        </Flex>
        <Button
          size="1"
          variant="ghost"
          onClick={() => void reload(query.trim())}
          disabled={busy || !serverReachable}
          style={{ minWidth: 64, justifyContent: "center" }}
        >
          {busy ? "Loading…" : "Refresh"}
        </Button>
        <Text
          size="1"
          color="gray"
          style={{
            marginLeft: "auto",
            flexShrink: 0,
            minWidth: 96,
            textAlign: "right",
            fontVariantNumeric: "tabular-nums",
          }}
        >
          {visible.length} of {users.length}
        </Text>
      </Flex>

      {error ? (
        <Text size="1" color="red">
          {error}
        </Text>
      ) : null}

      {!serverReachable ? (
        <Box className="server-error">
          <Text size="2" color="gray">
            The BattleGroup is offline — player data isn&apos;t available. Auto-refresh is
            paused and resumes automatically when the server is back up.
          </Text>
        </Box>
      ) : (
      <Table.Root variant="surface" size="1">
        <Table.Header>
          <Table.Row>
            <Table.ColumnHeaderCell>Name</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell>FLS ID</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell>Level</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell>Partition</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell>Status</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell>Last seen</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell></Table.ColumnHeaderCell>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {visible.map((user) => (
            <Table.Row key={user.flsId}>
              <Table.Cell>{user.name || <Text color="gray">—</Text>}</Table.Cell>
              <Table.Cell className="mono" style={{ fontSize: 11 }}>
                {user.flsId}
              </Table.Cell>
              <Table.Cell className="mono" style={{ fontSize: 11 }}>
                {user.level ?? <Text color="gray">—</Text>}
              </Table.Cell>
              <Table.Cell className="mono" style={{ fontSize: 11 }}>
                {user.partitionId ?? <Text color="gray">—</Text>}
              </Table.Cell>
              <Table.Cell>
                <Badge color={user.online.toLowerCase() === "online" ? "green" : "gray"}>
                  {user.online || "offline"}
                </Badge>
              </Table.Cell>
              <Table.Cell className="mono" style={{ fontSize: 11, color: "var(--gray-10)" }}>
                {user.online.toLowerCase() === "online" ? "—" : formatLastSeen(user.lastSeen)}
              </Table.Cell>
              <Table.Cell>
                <DropdownMenu.Root>
                  <DropdownMenu.Trigger>
                    <Button size="1" variant="ghost">
                      Actions
                    </Button>
                  </DropdownMenu.Trigger>
                  <DropdownMenu.Content>
                    <DropdownMenu.Item onSelect={() => void copyTextToClipboard(user.flsId)}>
                      Copy FLS ID
                    </DropdownMenu.Item>
                    <DropdownMenu.Item
                      onSelect={() =>
                        onSwitchToAdmin({
                          commandId: "AddItemToInventory",
                          values: { PlayerId: user.flsId },
                        })
                      }
                    >
                      Grant item…
                    </DropdownMenu.Item>
                    <DropdownMenu.Item
                      onSelect={() =>
                        onSwitchToAdmin({
                          commandId: "AwardXP",
                          values: { PlayerId: user.flsId },
                        })
                      }
                    >
                      Award XP…
                    </DropdownMenu.Item>
                    <DropdownMenu.Item
                      onSelect={() =>
                        onSwitchToAdmin({
                          commandId: "TeleportTo",
                          values: { PlayerId: user.flsId },
                        })
                      }
                    >
                      Teleport…
                    </DropdownMenu.Item>
                    <DropdownMenu.Separator />
                    <DropdownMenu.Item
                      color="red"
                      onSelect={() =>
                        onSwitchToAdmin({
                          commandId: "KickPlayer",
                          values: { PlayerId: user.flsId },
                        })
                      }
                    >
                      Kick player…
                    </DropdownMenu.Item>
                  </DropdownMenu.Content>
                </DropdownMenu.Root>
              </Table.Cell>
            </Table.Row>
          ))}
          {visible.length === 0 && !busy ? (
            <Table.Row>
              <Table.Cell colSpan={7}>
                <Text color="gray">
                  No users{onlineOnly ? " online" : ""}.
                </Text>
              </Table.Cell>
            </Table.Row>
          ) : null}
        </Table.Body>
      </Table.Root>
      )}
    </Box>
  );
}
