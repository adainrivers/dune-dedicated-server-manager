import { useEffect, useRef } from "react";

/**
 * Calls `poll` every `intervalMs` while `enabled`, skipping ticks while the
 * window is hidden or a previous poll is still running (#22). The latest
 * `poll` is always used without restarting the timer.
 */
export function useStatusAutoRefresh(
  enabled: boolean,
  poll: () => Promise<void>,
  intervalMs: number,
): void {
  const pollRef = useRef(poll);
  const inFlight = useRef(false);
  pollRef.current = poll;

  useEffect(() => {
    if (!enabled) return;
    const handle = window.setInterval(() => {
      if (inFlight.current || document.visibilityState === "hidden") return;
      inFlight.current = true;
      void pollRef.current().finally(() => {
        inFlight.current = false;
      });
    }, intervalMs);
    return () => window.clearInterval(handle);
  }, [enabled, intervalMs]);
}
