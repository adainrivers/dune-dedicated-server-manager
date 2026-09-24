import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import { readUsersAutoRefresh, writeUsersAutoRefresh } from "../../services/storage";
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
  /** True when the BattleGroup is known to be stopped; pauses polling. */
  battlegroupStopped: boolean;
  onSwitchToAdmin: (prefill: AdminTabPrefill) => void;
};

export default function UsersTab({ tunnelId, battlegroupStopped, onSwitchToAdmin }: UsersTabProps) {
  const [users, setUsers] = useState<PlayerDto[]>([]);
  const [query, setQuery] = useState("");
  const [onlineOnly, setOnlineOnly] = useState(false);
  const [autoRefresh, setAutoRefreshState] = useState(readUsersAutoRefresh);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false);

  const setAutoRefresh = useCallback((enabled: boolean) => {
    setAutoRefreshState(enabled);
    writeUsersAutoRefresh(enabled);
  }, []);

  const reload = useCallback(
    async (q: string) => {
      inFlight.current = true;
      setBusy(true);
      setError(null);
      try {
        const rows = await managementApi.searchPlayers(tunnelId, q, 200);
        setUsers(rows);
      } catch (err) {
        setError(String(err));
      } finally {
        inFlight.current = false;
        setBusy(false);
      }
    },
    [tunnelId],
  );

  useEffect(() => {
    void reload("");
  }, [reload]);

  useEffect(() => {
    const handle = setTimeout(() => {
      void reload(query.trim());
    }, 300);
    return () => clearTimeout(handle);
  }, [query, reload]);

  // Poll for live player-status changes. Without this the list only refreshed
  // on mount / manual click, so logins and logouts went unseen until the app
  // was reopened (#13). Toggleable per #14; on by default. Paused while the
  // BattleGroup is stopped (the game DB is down, so every poll would just
  // wait out a timeout) and skipped while a previous poll is in flight (#25).
  const polling = autoRefresh && !battlegroupStopped;
  useEffect(() => {
    if (!polling) return;
    const handle = setInterval(() => {
      if (inFlight.current) return;
      void reload(query.trim());
    }, 5000);
    return () => clearInterval(handle);
  }, [polling, query, reload]);

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
          style={{ flex: "1 1 280px", minWidth: 0 }}
        />
        <Flex align="center" gap="2">
          <Switch checked={onlineOnly} onCheckedChange={setOnlineOnly} />
          <Text size="2">Online only</Text>
        </Flex>
        <Flex align="center" gap="2">
          <Switch checked={autoRefresh} onCheckedChange={setAutoRefresh} />
          <Text size="2">
            Auto-refresh
            {autoRefresh && battlegroupStopped ? (
              <Text color="gray"> (paused, BattleGroup stopped)</Text>
            ) : null}
          </Text>
        </Flex>
        <Button
          size="1"
          variant="ghost"
          onClick={() => void reload(query.trim())}
          disabled={busy}
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
    </Box>
  );
}
