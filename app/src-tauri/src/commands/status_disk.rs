use dune_manager_core::orchestration::{RemoteCommandRunner, RusshRunner};

use crate::dto::RemoteDiskUsage;

/// Filesystems worth watching: the root filesystem and, when it lives on its
/// own mount, the k3s storage that holds game data, images, and dumps.
const DISK_USAGE_SCRIPT: &str = r#"
for path in / /var/lib/rancher/k3s; do
  [ -e "$path" ] && df -Pk "$path" 2>/dev/null | tail -n +2
done
true
"#;

/// Reads disk usage for the server's root and k3s storage filesystems (#38).
/// Best-effort: any failure yields an empty list and the dashboard simply
/// hides the tile.
pub(crate) fn read_disk_usage(runner: &RusshRunner) -> Vec<RemoteDiskUsage> {
    runner
        .run_script(DISK_USAGE_SCRIPT)
        .map(|text| parse_df_output(&text))
        .unwrap_or_default()
}

/// Parses POSIX `df -Pk` data lines (no header), dropping duplicate mounts.
/// Columns: filesystem, 1024-blocks, used, available, capacity, mounted on.
pub(crate) fn parse_df_output(text: &str) -> Vec<RemoteDiskUsage> {
    let mut out: Vec<RemoteDiskUsage> = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 6 {
            continue;
        }
        let (Ok(total_kb), Ok(used_kb), Ok(available_kb)) = (
            cols[1].parse::<u64>(),
            cols[2].parse::<u64>(),
            cols[3].parse::<u64>(),
        ) else {
            continue;
        };
        // Mount points can contain spaces; everything after capacity is the mount.
        let mount = cols[5..].join(" ");
        if total_kb == 0 || out.iter().any(|disk| disk.mount == mount) {
            continue;
        }
        out.push(RemoteDiskUsage {
            mount,
            total_kb,
            used_kb,
            available_kb,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_root_and_separate_k3s_mount() {
        let text = "/dev/sda1 102687672 46341296 51087596 48% /\n\
                    /dev/sdb1 515928320 120000000 369684992 25% /var/lib/rancher\n";
        let disks = parse_df_output(text);
        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].mount, "/");
        assert_eq!(disks[0].total_kb, 102_687_672);
        assert_eq!(disks[0].used_kb, 46_341_296);
        assert_eq!(disks[0].available_kb, 51_087_596);
        assert_eq!(disks[1].mount, "/var/lib/rancher");
    }

    #[test]
    fn same_filesystem_is_reported_once() {
        // Typical single-disk host: both paths resolve to the root mount.
        let text = "/dev/sda1 102687672 46341296 51087596 48% /\n\
                    /dev/sda1 102687672 46341296 51087596 48% /\n";
        assert_eq!(parse_df_output(text).len(), 1);
    }

    #[test]
    fn skips_headers_and_malformed_lines() {
        let text = "Filesystem 1024-blocks Used Available Capacity Mounted on\n\
                    overlay broken\n\
                    tmpfs 0 0 0 - /dev/shm\n";
        assert!(parse_df_output(text).is_empty());
    }
}
