import { SlidersHorizontal } from "lucide-react";
import type { DirectorMapSummary, MapOverrideDraft } from "../../types";

export type MapOverridePanelProps = {
  directorMaps: DirectorMapSummary[];
  selectedDirectorMapSummary: DirectorMapSummary | null;
  mapOverrideDraft: MapOverrideDraft;
  busy: boolean;
  onSelectMap: (mapName: string) => void;
  onMapOverrideDraftChange: (draft: MapOverrideDraft) => void;
  onSaveMapOverride: () => void;
  onClearMapOverride: (mapName: string) => void;
  preview: Record<string, unknown> | null;
  onBackupConfig: (name: string, value: unknown) => void;
};

export function MapOverridePanel({
  directorMaps,
  selectedDirectorMapSummary,
  mapOverrideDraft,
  busy,
  onSelectMap,
  onMapOverrideDraftChange,
  onSaveMapOverride,
  onClearMapOverride,
  preview,
  onBackupConfig
}: MapOverridePanelProps) {
  if (!selectedDirectorMapSummary) return null;

  return (
    <section className="panel">
      <div className="panel-title">
        <h2>Map Override</h2>
        <SlidersHorizontal size={19} />
      </div>
      <section className="native-config-box">
        <div className="form-grid">
          <label>
            Map
            <select value={selectedDirectorMapSummary.name} onChange={(event) => onSelectMap(event.target.value)}>
              {directorMaps.map((map) => (
                <option value={map.name} key={`${map.kind}-option-${map.name}`}>
                  {map.name} ({map.kind})
                </option>
              ))}
            </select>
          </label>
          <label>
            Player hard cap
            <input
              type="number"
              min="1"
              value={mapOverrideDraft.playerHardCap}
              onChange={(event) => onMapOverrideDraftChange({ ...mapOverrideDraft, playerHardCap: event.target.value })}
              placeholder="leave empty for null"
            />
          </label>
          {selectedDirectorMapSummary.kind === "Instanced" && (
            <>
              <label>
                Scaling throttle
                <input
                  type="number"
                  min="0"
                  value={mapOverrideDraft.throttlingSeconds}
                  onChange={(event) =>
                    onMapOverrideDraftChange({ ...mapOverrideDraft, throttlingSeconds: event.target.value })
                  }
                  placeholder="seconds"
                />
              </label>
              <label>
                Min servers
                <input
                  type="number"
                  min="0"
                  value={mapOverrideDraft.minServers}
                  onChange={(event) => onMapOverrideDraftChange({ ...mapOverrideDraft, minServers: event.target.value })}
                />
              </label>
              <label>
                Extra servers
                <input
                  type="number"
                  min="0"
                  value={mapOverrideDraft.extraServers}
                  onChange={(event) =>
                    onMapOverrideDraftChange({ ...mapOverrideDraft, extraServers: event.target.value })
                  }
                />
              </label>
            </>
          )}
        </div>
        <div className="toggle-grid">
          <label>
            <input
              type="checkbox"
              checked={mapOverrideDraft.updatePlayerCountOnFls}
              onChange={(event) =>
                onMapOverrideDraftChange({ ...mapOverrideDraft, updatePlayerCountOnFls: event.target.checked })
              }
            />
            Update player count on FLS
          </label>
          {selectedDirectorMapSummary.kind === "Dimension" && (
            <label>
              <input
                type="checkbox"
                checked={mapOverrideDraft.enforceSameHomeDimension}
                onChange={(event) =>
                  onMapOverrideDraftChange({ ...mapOverrideDraft, enforceSameHomeDimension: event.target.checked })
                }
              />
              Enforce same home dimension
            </label>
          )}
          {selectedDirectorMapSummary.kind === "Instanced" && (
            <label>
              <input
                type="checkbox"
                checked={mapOverrideDraft.automaticScaling}
                onChange={(event) =>
                  onMapOverrideDraftChange({ ...mapOverrideDraft, automaticScaling: event.target.checked })
                }
              />
              Automatic scaling
            </label>
          )}
        </div>
        <div className="button-row">
          <button
            onClick={() => onBackupConfig(`${selectedDirectorMapSummary.name}-map-summary`, selectedDirectorMapSummary)}
            disabled={!selectedDirectorMapSummary}
          >
            Backup
          </button>
          <button onClick={onSaveMapOverride} disabled={busy}>
            Update Override
          </button>
          <button
            onClick={() => onClearMapOverride(selectedDirectorMapSummary.name)}
            disabled={busy || !selectedDirectorMapSummary.hasOverride}
          >
            Clear Override
          </button>
        </div>
        <pre className="config-preview">{JSON.stringify(preview, null, 2)}</pre>
      </section>
    </section>
  );
}
