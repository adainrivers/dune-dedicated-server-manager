use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use crate::{
    errors::failure,
    models::CommandResult,
    orchestration::{GuestNetworkConfig, GuestProvider, OpenSshRunner, OpenSshTarget},
};

#[derive(Debug, Clone)]
pub struct OpenSshGuestProvider {
    pub ssh_path: PathBuf,
    pub key_path: PathBuf,
    pub user: String,
}

impl OpenSshGuestProvider {
    pub fn new(
        ssh_path: impl Into<PathBuf>,
        key_path: impl Into<PathBuf>,
        user: impl Into<String>,
    ) -> Self {
        Self {
            ssh_path: ssh_path.into(),
            key_path: key_path.into(),
            user: user.into(),
        }
    }

    fn runner(&self, ip: &str) -> OpenSshRunner {
        OpenSshRunner::new(OpenSshTarget::new(
            self.ssh_path.clone(),
            self.key_path.clone(),
            self.user.clone(),
            ip,
        ))
    }
}

impl GuestProvider for OpenSshGuestProvider {
    fn wait_for_ssh(&self, ip: &str, timeout_seconds: u64) -> CommandResult<()> {
        validate_ip_present(ip)?;
        let started = Instant::now();
        let runner = self.runner(ip);
        while started.elapsed().as_secs() < timeout_seconds {
            if crate::orchestration::RemoteCommandRunner::run(&runner, "true").is_ok() {
                return Ok(());
            }
            thread::sleep(Duration::from_secs(2));
        }
        Err(failure(format!(
            "Guest did not become reachable on {ip} within {timeout_seconds} seconds"
        )))
    }

    fn upload_bytes(
        &self,
        ip: &str,
        remote_path: &str,
        bytes: &[u8],
        mode: u32,
    ) -> CommandResult<()> {
        validate_ip_present(ip)?;
        validate_remote_path(remote_path)?;
        let encoded = base64_encode(bytes);
        let mode_text = format!("{:o}", mode & 0o777);
        let mut script = String::from("set -euo pipefail\n");
        script.push_str(&shell_value("REMOTE_PATH", remote_path));
        script.push_str(&shell_value("MODE", &mode_text));
        script.push_str(&shell_value("PAYLOAD_B64", &encoded));
        script.push_str(
            r#"
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
printf '%s' "$PAYLOAD_B64" | base64 -d > "$tmp"
sudo install -D -m "$MODE" "$tmp" "$REMOTE_PATH"
"#,
        );
        crate::orchestration::RemoteCommandRunner::run_script(&self.runner(ip), &script)?;
        Ok(())
    }

    fn write_player_settings(&self, ip: &str, player_ip: &str) -> CommandResult<()> {
        validate_ip_present(ip)?;
        validate_ipv4ish(player_ip, "player-facing IP")?;
        let mut script = String::from("set -euo pipefail\n");
        script.push_str(&shell_value("PLAYER_IP", player_ip.trim()));
        script.push_str(
            r#"
mkdir -p /home/dune/.dune
printf '\n\n\n%s\n' "$PLAYER_IP" > /home/dune/.dune/settings.conf
"#,
        );
        crate::orchestration::RemoteCommandRunner::run_script(&self.runner(ip), &script)?;
        Ok(())
    }

    fn apply_static_network(&self, ip: &str, config: &GuestNetworkConfig) -> CommandResult<()> {
        validate_ip_present(ip)?;
        validate_static_network(config)?;
        let mut script = String::from("set -euo pipefail\n");
        script.push_str(&shell_value("IFACE", &config.interface));
        script.push_str(&shell_value("ADDRESS_CIDR", &config.address_cidr));
        script.push_str(&shell_value("GATEWAY", &config.gateway));
        script.push_str(&shell_value("DNS", &config.dns));
        script.push_str(
            r#"
cat > /tmp/dune-interfaces <<EOF
auto lo
iface lo inet loopback

auto ${IFACE}
iface ${IFACE} inet static
    address ${ADDRESS_CIDR}
    gateway ${GATEWAY}
EOF
printf 'nameserver %s\n' "$DNS" > /tmp/dune-resolv
sudo cp /tmp/dune-interfaces /etc/network/interfaces
sudo cp /tmp/dune-resolv /etc/resolv.conf
nohup sudo sh -c 'sleep 2; rc-service networking restart' </dev/null >/dev/null 2>&1 &
"#,
        );
        crate::orchestration::RemoteCommandRunner::run_script(&self.runner(ip), &script)?;
        Ok(())
    }

    fn detect_public_ip(&self, ip: &str) -> CommandResult<Option<String>> {
        validate_ip_present(ip)?;
        let output = crate::orchestration::RemoteCommandRunner::run(
            &self.runner(ip),
            "wget -qO- https://api.ipify.org || true",
        )?;
        let candidate = output.trim();
        if candidate.is_empty() {
            return Ok(None);
        }
        validate_ipv4ish(candidate, "detected public IP")?;
        Ok(Some(candidate.to_string()))
    }
}

fn validate_ip_present(ip: &str) -> CommandResult<()> {
    if ip.trim().is_empty() {
        Err(failure("Guest IP is required"))
    } else {
        Ok(())
    }
}

fn validate_remote_path(path: &str) -> CommandResult<()> {
    if path.trim().is_empty()
        || path.contains('\n')
        || path.contains('\r')
        || !path.starts_with('/')
    {
        Err(failure("Remote path must be an absolute single-line path"))
    } else {
        Ok(())
    }
}

fn validate_static_network(config: &GuestNetworkConfig) -> CommandResult<()> {
    if config.interface.trim().is_empty()
        || config.interface.contains('/')
        || config.interface.contains(char::is_whitespace)
    {
        return Err(failure("Guest network interface is invalid"));
    }
    validate_ipv4_cidr(&config.address_cidr)?;
    validate_ipv4ish(&config.gateway, "static gateway")?;
    validate_ipv4ish(&config.dns, "static DNS")?;
    Ok(())
}

fn validate_ipv4_cidr(value: &str) -> CommandResult<()> {
    let Some((ip, prefix)) = value.split_once('/') else {
        return Err(failure("Static address must include a CIDR prefix"));
    };
    validate_ipv4ish(ip, "static address")?;
    let prefix = prefix
        .parse::<u8>()
        .map_err(|_| failure("Static CIDR prefix is invalid"))?;
    if prefix > 32 {
        return Err(failure("Static CIDR prefix must be 0-32"));
    }
    Ok(())
}

fn validate_ipv4ish(value: &str, label: &str) -> CommandResult<()> {
    let parts = value.trim().split('.').collect::<Vec<_>>();
    if parts.len() == 4 && parts.iter().all(|part| part.parse::<u8>().is_ok()) {
        Ok(())
    } else {
        Err(failure(format!("{label} must be an IPv4 address")))
    }
}

fn shell_value(name: &str, value: &str) -> String {
    let delimiter = format!("__DUNE_MANAGER_{name}__");
    format!("{name}=$(cat <<'{delimiter}'\n{value}\n{delimiter}\n)\n")
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        encoded.push(TABLE[(b0 >> 2) as usize] as char);
        encoded.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            encoded.push('=');
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encoder_matches_standard_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn validates_static_network_shape() {
        validate_static_network(&GuestNetworkConfig {
            interface: "eth0".to_string(),
            address_cidr: "10.0.0.4/24".to_string(),
            gateway: "10.0.0.1".to_string(),
            dns: "1.1.1.1".to_string(),
        })
        .unwrap();

        assert!(validate_static_network(&GuestNetworkConfig {
            interface: "eth0".to_string(),
            address_cidr: "10.0.0.4/99".to_string(),
            gateway: "10.0.0.1".to_string(),
            dns: "1.1.1.1".to_string(),
        })
        .is_err());
    }
}
