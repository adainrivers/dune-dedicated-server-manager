import type { AppConfig, CommandFailure, ManagerWorkloads, Workloads } from "./types";

export const defaultConfig: AppConfig = {
  installPath: "",
  vmName: "",
  vmIp: "",
  sshUser: "",
  sshPath: "",
  managerApiUrl: "",
  managerApiToken: "",
  managerApiNamespace: "",
  managerApiImage: "",
  managerApiBinaryPath: "",
  managerApiDirectorUrl: ""
};

export function formatBytes(bytes: number) {
  if (!bytes) return "0 GB";
  return `${Math.round((bytes / 1024 ** 3) * 10) / 10} GB`;
}

export function asError(error: unknown): CommandFailure {
  if (typeof error === "object" && error !== null && "message" in error) {
    return error as CommandFailure;
  }
  return { message: String(error) };
}

export function vmHealthLabel(state?: string | null, status?: string | null) {
  if (!status) return "Unknown";
  if ((state ?? "").toLowerCase() === "off" && status.toLowerCase() === "operating normally") {
    return "Configuration OK";
  }
  return status;
}

export function valueAt(value: unknown, path: string[]) {
  let current = value;
  for (const key of path) {
    if (!current || typeof current !== "object" || !(key in current)) return null;
    current = (current as Record<string, unknown>)[key];
  }
  if (current === null || current === undefined) return null;
  if (typeof current === "boolean") return current ? "true" : "false";
  if (typeof current === "number" || typeof current === "string") return current;
  return JSON.stringify(current);
}

export function numberAt(value: unknown, path: string[], fallback = "") {
  const found = valueAt(value, path);
  return found === null ? fallback : String(found);
}

export function boolAt(value: unknown, path: string[], fallback = false) {
  let current = value;
  for (const key of path) {
    if (!current || typeof current !== "object" || !(key in current)) return fallback;
    current = (current as Record<string, unknown>)[key];
  }
  return typeof current === "boolean" ? current : fallback;
}

export function nullableNumber(value: string) {
  const trimmed = value.trim();
  return trimmed ? Number(trimmed) : null;
}

export function delay(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

export function generateToken() {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function managerWorkloadsToUi(value: ManagerWorkloads): Workloads {
  return {
    pods: {
      items: value.pods.map((pod) => ({
        metadata: { name: pod.name, creationTimestamp: pod.createdAt ?? undefined },
        status: { phase: pod.phase, ready: pod.ready, restarts: pod.restarts }
      }))
    },
    services: {
      items: value.services.map((service) => ({
        metadata: { name: service.name },
        spec: {
          type: service.serviceType,
          clusterIP: service.clusterIp,
          externalIPs: service.externalIps,
          ports: service.ports
        }
      }))
    }
  };
}
