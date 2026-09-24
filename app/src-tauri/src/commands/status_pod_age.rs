use std::collections::HashMap;

use dune_manager_core::orchestration::{RemoteCommandRunner, RusshRunner};

use crate::commands::shared::sh_single_quoted;

/// Reads `partition index -> pod start time` for the map server pods of a
/// BattleGroup namespace. The BattleGroup CR only carries a BG-level
/// `startTimestamp`, so without this every server row showed the BG uptime
/// instead of its own pod's (#21).
///
/// Best-effort: any failure yields an empty map and the caller falls back to
/// the BG-level age. Only names and start times are requested, so pod specs
/// (which carry credentials in their args) never leave the host.
pub(crate) fn read_server_pod_starts(
    runner: &RusshRunner,
    namespace: &str,
) -> HashMap<u64, String> {
    let command = format!(
        "sudo kubectl get pods -n {} -o jsonpath='{{range .items[*]}}{{.metadata.name}}{{\"\\t\"}}{{.status.startTime}}{{\"\\n\"}}{{end}}'",
        sh_single_quoted(namespace),
    );
    runner
        .run(&command)
        .map(|text| parse_server_pod_starts(&text))
        .unwrap_or_default()
}

/// Parses `name<TAB>startTime` lines. Map server pods are named
/// `<bg>-sg-<map>-pod-<partitionIndex>`; everything else is ignored.
pub(crate) fn parse_server_pod_starts(text: &str) -> HashMap<u64, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some((name, start)) = line.split_once('\t') else {
            continue;
        };
        let start = start.trim();
        if start.is_empty() || !name.contains("-sg-") {
            continue;
        }
        let Some((_, suffix)) = name.trim().rsplit_once("-pod-") else {
            continue;
        };
        if let Ok(partition) = suffix.parse::<u64>() {
            out.insert(partition, start.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_partition_suffix_to_start_time() {
        let text = "sh-x-rcpxhh-sg-survival-1-pod-1\t2026-06-01T10:00:00Z\n\
                    sh-x-rcpxhh-sg-survival-1-pod-31\t2026-06-01T12:30:00Z\n\
                    sh-x-rcpxhh-sg-deepdesert-1-pod-8\t2026-06-01T11:00:00Z\n";
        let starts = parse_server_pod_starts(text);
        assert_eq!(starts.len(), 3);
        assert_eq!(
            starts.get(&31).map(String::as_str),
            Some("2026-06-01T12:30:00Z")
        );
        assert_eq!(
            starts.get(&8).map(String::as_str),
            Some("2026-06-01T11:00:00Z")
        );
    }

    #[test]
    fn ignores_non_server_pods_and_pending_pods() {
        let text = "sh-x-rcpxhh-db-dbdepl-sts-0\t2026-06-01T10:00:00Z\n\
                    sh-x-rcpxhh-sgw-deploy-577498fc65-pmkfb\t2026-06-01T10:00:00Z\n\
                    sh-x-rcpxhh-dump-20260601-020000-pod\t2026-06-01T02:00:00Z\n\
                    sh-x-rcpxhh-sg-overmap-pod-2\t\n\
                    garbage line without tab\n";
        assert!(parse_server_pod_starts(text).is_empty());
    }
}
