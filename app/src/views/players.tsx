import { Clipboard, Download, RefreshCw, Users } from "lucide-react";
import { useMemo, useState } from "react";
import { EmptyState, Metric } from "../components/primitives";
import type { DirectorPlayerLists, DirectorPlayerSummary } from "../types";

type PlayerCategory = keyof DirectorPlayerLists;

type PlayersPanelProps = {
  players: DirectorPlayerSummary | null;
  playerLists: DirectorPlayerLists | null;
  busy: boolean;
  onReload: () => void;
};

const categories: { key: PlayerCategory; label: string }[] = [
  { key: "all", label: "All" },
  { key: "online", label: "Online" },
  { key: "queued", label: "Queued" },
  { key: "inTransit", label: "In Transit" },
  { key: "gracePeriod", label: "Grace" },
  { key: "completion", label: "Completion" }
];

export function PlayersPanel({ players, playerLists, busy, onReload }: PlayersPanelProps) {
  const [activeCategory, setActiveCategory] = useState<PlayerCategory>("online");
  const [filter, setFilter] = useState("");
  const activePlayers = playerLists?.[activeCategory] ?? [];
  const visiblePlayers = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    if (!needle) return activePlayers;
    return activePlayers.filter((player) => player.toLowerCase().includes(needle));
  }, [activePlayers, filter]);

  function exportPlayers() {
    const blob = new Blob([visiblePlayers.join("\n")], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `director-${activeCategory}-players.txt`;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  function copyPlayers() {
    void navigator.clipboard?.writeText(visiblePlayers.join("\n"));
  }

  return (
    <section className="panel">
      <div className="panel-title">
        <h2>Players</h2>
        <div className="button-row">
          <button onClick={onReload} disabled={busy}>
            <RefreshCw size={16} />
            Reload
          </button>
          <Users size={19} />
        </div>
      </div>
      {!players ? (
        <EmptyState text="No Director player telemetry loaded." />
      ) : (
        <>
          <div className="metric-grid">
            <Metric label="Active" value={players.active} />
            <Metric label="Online" value={players.online} />
            <Metric label="In Transit" value={players.inTransit} />
            <Metric label="Grace Period" value={players.gracePeriod} />
            <Metric label="Completion" value={players.completion} />
            <Metric label="Queued" value={players.queued} />
            <Metric label="Login Requests" value={players.loginRequestsTotal} />
            <Metric label="Travel Requests" value={players.travelRequestsTotal} />
          </div>

          <section className="native-detail-box">
            <div className="native-tabs" role="tablist" aria-label="Player categories">
              {categories.map((category) => (
                <button
                  className={category.key === activeCategory ? "active" : ""}
                  key={category.key}
                  onClick={() => setActiveCategory(category.key)}
                >
                  {category.label}
                  <span>{playerLists?.[category.key]?.length ?? 0}</span>
                </button>
              ))}
            </div>
            <div className="detail-toolbar">
              <input value={filter} onChange={(event) => setFilter(event.target.value)} placeholder="Filter player id" />
              <button onClick={copyPlayers} disabled={visiblePlayers.length === 0}>
                <Clipboard size={16} />
                Copy
              </button>
              <button onClick={exportPlayers} disabled={visiblePlayers.length === 0}>
                <Download size={16} />
                Export
              </button>
            </div>
            {visiblePlayers.length === 0 ? (
              <EmptyState text="No players in this category." />
            ) : (
              <div className="player-id-grid">
                {visiblePlayers.map((player) => (
                  <span className="mono" key={player}>
                    {player}
                  </span>
                ))}
              </div>
            )}
          </section>
        </>
      )}
    </section>
  );
}
