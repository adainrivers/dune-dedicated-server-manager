import { Download, RefreshCw, Terminal } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { CSSProperties } from "react";
import { EmptyState } from "../components/primitives";
import type { KubeItem, ManagerLogResponse } from "../types";

type LogSegment = {
  text: string;
  className?: string;
  style?: CSSProperties;
};

const ansi256Palette: Record<number, string> = {
  196: "#ff5f5f",
  202: "#ff5f00",
  208: "#ff8700",
  214: "#ffaf00",
  220: "#ffd700",
  226: "#ffff00",
  82: "#5fff00",
  46: "#00ff00",
  51: "#00ffff",
  45: "#00d7ff",
  39: "#00afff",
  117: "#87d7ff",
  141: "#af87ff",
  213: "#ff87ff",
  244: "#808080",
  250: "#bcbcbc"
};

type LogsPanelProps = {
  pods: KubeItem[];
  busy: boolean;
  onLoadLogs: (pod: string, container: string, tail: number) => Promise<ManagerLogResponse | null>;
};

export function LogsPanel({ pods, busy, onLoadLogs }: LogsPanelProps) {
  const [selectedPod, setSelectedPod] = useState("");
  const [selectedContainer, setSelectedContainer] = useState("");
  const [tail, setTail] = useState("300");
  const [filter, setFilter] = useState("");
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [logResponse, setLogResponse] = useState<ManagerLogResponse | null>(null);
  const selectedPodItem = pods.find((pod) => pod.metadata?.name === selectedPod) ?? pods[0] ?? null;
  const containers = useMemo(() => {
    const values = selectedPodItem?.status?.containers;
    return Array.isArray(values) ? values.map(String) : [];
  }, [selectedPodItem]);

  async function load() {
    const podName = selectedPodItem?.metadata?.name;
    if (!podName) return;
    const response = await onLoadLogs(podName, selectedContainer, Number(tail) || 300);
    if (response) setLogResponse(response);
  }

  function exportLogs() {
    if (!logResponse) return;
    const name = [logResponse.pod, logResponse.container, "logs.txt"].filter(Boolean).join("-");
    const blob = new Blob([visibleLines(logResponse.lines, filter).join("\n")], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = name;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  useEffect(() => {
    if (!selectedPod && pods[0]?.metadata?.name) {
      setSelectedPod(pods[0].metadata.name);
    }
  }, [pods, selectedPod]);

  useEffect(() => {
    if (containers.length > 0 && selectedContainer && !containers.includes(selectedContainer)) {
      setSelectedContainer("");
    }
  }, [containers, selectedContainer]);

  useEffect(() => {
    if (!autoRefresh) return;
    const interval = window.setInterval(() => void load(), 4000);
    return () => window.clearInterval(interval);
  }, [autoRefresh, selectedPodItem?.metadata?.name, selectedContainer, tail]);

  return (
    <section className="panel logs-panel">
      <div className="panel-title">
        <h2>Logs</h2>
        <Terminal size={19} />
      </div>

      {pods.length === 0 ? (
        <EmptyState text="No pods are available for log collection." />
      ) : (
        <>
          <section className="log-controls">
            <label>
              Pod
              <select value={selectedPodItem?.metadata?.name ?? ""} onChange={(event) => setSelectedPod(event.target.value)}>
                {pods.map((pod) => (
                  <option key={pod.metadata?.name} value={pod.metadata?.name}>
                    {pod.metadata?.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Container
              <select value={selectedContainer} onChange={(event) => setSelectedContainer(event.target.value)}>
                <option value="">Default</option>
                {containers.map((container) => (
                  <option key={container} value={container}>
                    {container}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Tail lines
              <input type="number" min="1" max="2000" value={tail} onChange={(event) => setTail(event.target.value)} />
            </label>
            <label>
              Filter
              <input value={filter} onChange={(event) => setFilter(event.target.value)} placeholder="warning, AMQP, pod id" />
            </label>
            <label className="inline-toggle">
              <input type="checkbox" checked={autoRefresh} onChange={(event) => setAutoRefresh(event.target.checked)} />
              Stream
            </label>
            <button onClick={load} disabled={busy || !selectedPodItem}>
              <RefreshCw size={16} />
              Refresh
            </button>
            <button onClick={exportLogs} disabled={!logResponse}>
              <Download size={16} />
              Export
            </button>
          </section>

          {logResponse ? (
            <LogOutput lines={visibleLines(logResponse.lines, filter)} />
          ) : (
            <EmptyState text="Select a pod and refresh to load recent logs." />
          )}
        </>
      )}
    </section>
  );
}

function visibleLines(lines: string[], filter: string) {
  const trimmed = filter.trim().toLowerCase();
  if (!trimmed) return lines;
  return lines.filter((line) => line.toLowerCase().includes(trimmed));
}

function LogOutput({ lines }: { lines: string[] }) {
  if (lines.length === 0) {
    return <div className="log-output empty-log">No log lines returned.</div>;
  }

  return (
    <div className="log-output" role="log" aria-live="polite">
      {lines.map((line, index) => (
        <div className="log-line" key={`${index}-${line.slice(0, 32)}`}>
          {parseAnsiLine(line).map((segment, segmentIndex) => (
            <span className={segment.className} style={segment.style} key={`${segmentIndex}-${segment.text.slice(0, 16)}`}>
              {segment.text || " "}
            </span>
          ))}
        </div>
      ))}
    </div>
  );
}

function parseAnsiLine(line: string): LogSegment[] {
  const normalized = line.replace(/\u001b\[/g, "[");
  const pattern = /\[(\d+(?:;\d+)*)m/g;
  const segments: LogSegment[] = [];
  let lastIndex = 0;
  let currentStyle: CSSProperties = {};
  let currentClass = logSeverityClass(normalized);
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(normalized))) {
    if (match.index > lastIndex) {
      segments.push({
        text: normalized.slice(lastIndex, match.index),
        className: currentClass,
        style: Object.keys(currentStyle).length ? { ...currentStyle } : undefined
      });
    }
    const parsed = applyAnsiCodes(match[1], currentStyle, currentClass);
    currentStyle = parsed.style;
    currentClass = parsed.className;
    lastIndex = pattern.lastIndex;
  }

  segments.push({
    text: normalized.slice(lastIndex),
    className: currentClass,
    style: Object.keys(currentStyle).length ? currentStyle : undefined
  });

  return segments.filter((segment) => segment.text.length > 0);
}

function applyAnsiCodes(
  rawCodes: string,
  style: CSSProperties,
  className?: string
): { style: CSSProperties; className?: string } {
  const codes = rawCodes.split(";").map((code) => Number(code));
  let nextStyle = { ...style };
  let nextClass = className;

  for (let index = 0; index < codes.length; index += 1) {
    const code = codes[index];
    if (code === 0) {
      nextStyle = {};
      nextClass = undefined;
    } else if (code === 1) {
      nextStyle.fontWeight = 800;
    } else if (code === 2) {
      nextStyle.opacity = 0.72;
    } else if (code === 3) {
      nextStyle.fontStyle = "italic";
    } else if (code === 22) {
      delete nextStyle.fontWeight;
      delete nextStyle.opacity;
    } else if (code === 23) {
      delete nextStyle.fontStyle;
    } else if (code === 39) {
      delete nextStyle.color;
    } else if (code >= 30 && code <= 37) {
      nextStyle.color = basicAnsiColor(code - 30);
    } else if (code >= 90 && code <= 97) {
      nextStyle.color = basicAnsiColor(code - 90, true);
    } else if (code === 38 && codes[index + 1] === 5 && Number.isFinite(codes[index + 2])) {
      nextStyle.color = ansi256Color(codes[index + 2]);
      index += 2;
    }
  }

  return { style: nextStyle, className: nextClass };
}

function basicAnsiColor(index: number, bright = false) {
  const normal = ["#2d3436", "#ff7675", "#55efc4", "#fdcb6e", "#74b9ff", "#a29bfe", "#81ecec", "#dfe6e9"];
  const intense = ["#636e72", "#ff8f87", "#78ffd6", "#ffe08a", "#9bd1ff", "#c0b7ff", "#a5fff4", "#ffffff"];
  return (bright ? intense : normal)[index] ?? undefined;
}

function ansi256Color(index: number) {
  if (ansi256Palette[index]) return ansi256Palette[index];
  if (index >= 16 && index <= 231) {
    const value = index - 16;
    const red = Math.floor(value / 36);
    const green = Math.floor((value % 36) / 6);
    const blue = value % 6;
    return `rgb(${ansiCube(red)}, ${ansiCube(green)}, ${ansiCube(blue)})`;
  }
  if (index >= 232 && index <= 255) {
    const shade = 8 + (index - 232) * 10;
    return `rgb(${shade}, ${shade}, ${shade})`;
  }
  return undefined;
}

function ansiCube(value: number) {
  return value === 0 ? 0 : 55 + value * 40;
}

function logSeverityClass(line: string) {
  const lower = line.toLowerCase();
  if (lower.includes("[error]") || lower.includes(" error ")) return "log-severity-error";
  if (lower.includes("[warning]") || lower.includes(" warn")) return "log-severity-warning";
  if (lower.includes("[info]") || lower.includes(" info")) return "log-severity-info";
  return undefined;
}
