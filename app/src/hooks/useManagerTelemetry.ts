import { useEffect, useState } from "react";
import type { AppConfig, ManagerApiStatus, TelemetryEnvelope } from "../types";

export function useManagerTelemetry(configLoaded: boolean, config: AppConfig) {
  const [managerStatus, setManagerStatus] = useState<ManagerApiStatus | null>(null);
  const [managerTelemetry, setManagerTelemetry] = useState<TelemetryEnvelope | null>(null);
  const [managerSocketState, setManagerSocketState] = useState<"disabled" | "connecting" | "connected" | "error">(
    "disabled"
  );
  const [managerError, setManagerError] = useState("");

  useEffect(() => {
    const baseUrl = config.managerApiUrl.trim().replace(/\/$/, "");
    if (!configLoaded || !baseUrl) {
      setManagerStatus(null);
      setManagerTelemetry(null);
      setManagerSocketState("disabled");
      setManagerError("");
      return;
    }

    let closed = false;
    const headers: HeadersInit = config.managerApiToken
      ? { Authorization: `Bearer ${config.managerApiToken}` }
      : {};

    async function loadManagerStatus() {
      try {
        const response = await fetch(`${baseUrl}/api/status`, { headers });
        if (!response.ok) throw new Error(`Manager API returned ${response.status}`);
        const nextStatus = (await response.json()) as ManagerApiStatus;
        if (!closed) {
          setManagerStatus(nextStatus);
          setManagerError("");
        }
      } catch (error) {
        if (!closed) {
          setManagerStatus(null);
          setManagerError(String(error));
        }
      }
    }

    void loadManagerStatus();
    setManagerSocketState("connecting");
    const websocketUrl = `${baseUrl.replace(/^http/i, "ws")}/api/telemetry${
      config.managerApiToken ? `?token=${encodeURIComponent(config.managerApiToken)}` : ""
    }`;
    const socket = new WebSocket(websocketUrl);

    socket.onopen = () => {
      if (!closed) setManagerSocketState("connected");
    };
    socket.onmessage = (event) => {
      if (closed) return;
      try {
        const envelope = JSON.parse(event.data) as TelemetryEnvelope;
        setManagerTelemetry(envelope);
        setManagerError("");
      } catch {
        setManagerError("Manager API sent an unreadable telemetry event");
      }
    };
    socket.onerror = () => {
      if (!closed) setManagerSocketState("error");
    };
    socket.onclose = () => {
      if (!closed) setManagerSocketState("error");
    };

    return () => {
      closed = true;
      socket.close();
    };
  }, [configLoaded, config.managerApiUrl, config.managerApiToken]);

  return {
    managerStatus,
    setManagerStatus,
    managerTelemetry,
    setManagerTelemetry,
    managerSocketState,
    managerError,
    setManagerError
  };
}
