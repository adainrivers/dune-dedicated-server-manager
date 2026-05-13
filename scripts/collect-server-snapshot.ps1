param(
  [Parameter(Mandatory = $true)]
  [string]$HostName,

  [string]$User = "dune",

  [string]$KeyPath = "",

  [string]$SshPath = "ssh.exe",

  [string]$Label = "server",

  [string]$Phase = "snapshot",

  [string]$OutputRoot = "snapshots",

  [int]$OperatorLogTail = 180
)

$ErrorActionPreference = "Stop"

function Assert-SafeName {
  param([string]$Value, [string]$Fallback)
  $safe = ($Value -replace "[^A-Za-z0-9._-]+", "-").Trim("-")
  if ([string]::IsNullOrWhiteSpace($safe)) {
    return $Fallback
  }
  return $safe
}

function Redact-Text {
  param([string]$Text)
  $redacted = $Text
  $redacted = $redacted -replace "eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+", "<redacted-jwt>"
  $redacted = $redacted -replace "(?i)(password|token|secret|auth|apikey|api_key|serviceauthkey|serviceauthtoken)([A-Za-z0-9_.-]*)(\s*[:=]\s*)(\S+)", '$1$2$3<redacted>'
  $redacted = $redacted -replace "(?i)(postgresql://[^:\s]+:)([^@\s]+)(@)", '$1<redacted>$3'
  return $redacted
}

function Invoke-Remote {
  param([string]$Command)
  $args = @(
    "-o", "BatchMode=yes",
    "-o", "StrictHostKeyChecking=accept-new",
    "-o", "ConnectTimeout=15"
  )
  if (-not [string]::IsNullOrWhiteSpace($KeyPath)) {
    $args += @("-i", $KeyPath)
  }
  $args += "$User@$HostName"
  $args += $Command

  $output = & $SshPath @args 2>&1
  $text = ($output | Out-String)
  return Redact-Text $text
}

function Write-RemoteFile {
  param([string]$FileName, [string]$Command)
  $path = Join-Path $SnapshotDir $FileName
  try {
    Invoke-Remote $Command | Set-Content -LiteralPath $path -Encoding UTF8
  } catch {
    $message = "Snapshot command failed: $($_.Exception.Message)"
    Redact-Text $message | Set-Content -LiteralPath $path -Encoding UTF8
  }
}

$safeLabel = Assert-SafeName $Label "server"
$safePhase = Assert-SafeName $Phase "snapshot"
$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$SnapshotDir = Join-Path (Join-Path $OutputRoot $safeLabel) "$safePhase-$timestamp"
New-Item -ItemType Directory -Force -Path $SnapshotDir | Out-Null

$operatorTail = [Math]::Max(20, $OperatorLogTail)

Write-Host "Collecting snapshot from $User@$HostName into $SnapshotDir"

Write-RemoteFile "00-system.txt" @'
printf '== host ==\n'
hostname || true
date -Iseconds || date || true
id || true
uname -a || true
printf '\n== os ==\n'
cat /etc/os-release 2>/dev/null || true
printf '\n== disk ==\n'
df -h / /home /home/dune/.dune /var/lib/rancher/k3s 2>&1 || df -h 2>&1 || true
printf '\n== memory ==\n'
free -h 2>/dev/null || cat /proc/meminfo 2>/dev/null || true
printf '\n== network ==\n'
ip -4 addr 2>/dev/null || ifconfig 2>/dev/null || true
ip -4 route 2>/dev/null || route -n 2>/dev/null || true
printf '\n== services ==\n'
(systemctl status k3s --no-pager 2>/dev/null || rc-service k3s status 2>/dev/null || service k3s status 2>/dev/null || true)
'@

Write-RemoteFile "01-dune-files.txt" @'
printf '== /home/dune/.dune ==\n'
sudo find /home/dune/.dune -maxdepth 2 -type f -printf '%M %u %g %s %TY-%Tm-%Td %TH:%TM %p\n' 2>/dev/null | sort || find /home/dune/.dune -maxdepth 2 -type f -exec ls -la {} \; 2>/dev/null || true
printf '\n== scripts ==\n'
ls -la /home/dune/.dune/download/scripts 2>/dev/null || true
'@

Write-RemoteFile "02-seed-configs.txt" @'
printf '== seed yamls ==\n'
for file in /home/dune/.dune/*.yaml /home/dune/.dune/*.yml /home/dune/.dune/*.conf /home/dune/.dune/*.ini; do
  [ -f "$file" ] || continue
  printf '\n===== %s =====\n' "$file"
  sudo sed -E 's/(password|token|secret|auth|apikey|api_key|ServiceAuthToken|ServiceAuthKey)([A-Za-z0-9_.-]*)([=:][^[:space:]]+)/\1\2=<redacted>/Ig; s#(postgresql://[^:[:space:]]+:)[^@[:space:]]+(@)#\1<redacted>\2#Ig' "$file" 2>/dev/null || true
done
'@

Write-RemoteFile "03-k8s-overview.txt" @'
sudo kubectl get nodes -o wide 2>&1 || true
printf '\n== namespaces ==\n'
sudo kubectl get ns 2>&1 || true
printf '\n== all resources ==\n'
sudo kubectl get all -A -o wide 2>&1 || true
printf '\n== storage ==\n'
sudo kubectl get pv,pvc -A -o wide 2>&1 || true
'@

Write-RemoteFile "04-igw-resources.yaml" @'
sudo kubectl get battlegroups,battlegroupdirectors,battlegroupstats,databases,databasedeployments,filebrowsers,messagequeues,servergateways,servergroups,serversets,serverstats,textrouters -A -o yaml 2>&1 \
  | sed -E 's/(password|token|secret|auth|apikey|api_key|ServiceAuthToken|ServiceAuthKey)([A-Za-z0-9_.-]*)([=:][^[:space:]]+)/\1\2=<redacted>/Ig; s#(postgresql://[^:[:space:]]+:)[^@[:space:]]+(@)#\1<redacted>\2#Ig'
'@

$operatorLogCommand = @'
printf '== events ==\n'
sudo kubectl get events -A --sort-by=.lastTimestamp 2>&1 || true
printf '\n== operator pods ==\n'
sudo kubectl get pods -n funcom-operators -o wide 2>&1 || true
printf '\n== operator logs ==\n'
for pod in $(sudo kubectl get pods -n funcom-operators -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}' 2>/dev/null); do
  printf '\n===== %s =====\n' "$pod"
  sudo kubectl logs -n funcom-operators "$pod" --all-containers --tail=__OPERATOR_TAIL__ 2>&1 \
    | sed -E 's/(password|token|secret|auth|apikey|api_key|ServiceAuthToken|ServiceAuthKey)([A-Za-z0-9_.-]*)([=:][^[:space:]]+)/\1\2=<redacted>/Ig; s#(postgresql://[^:[:space:]]+:)[^@[:space:]]+(@)#\1<redacted>\2#Ig' || true
done
'@
$operatorLogCommand = $operatorLogCommand.Replace("__OPERATOR_TAIL__", [string]$operatorTail)
Write-RemoteFile "05-events-and-operator-logs.txt" $operatorLogCommand

Write-RemoteFile "06-db-state.txt" @'
for ns in $(sudo kubectl get ns -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}' 2>/dev/null | grep '^funcom-seabass-' || true); do
  printf '\n===== %s =====\n' "$ns"
  sudo kubectl get pods -n "$ns" -o wide 2>&1 | grep -Ei 'db|postgres|pghero|util' || true
done
'@

Write-RemoteFile "07-raw-names.txt" @'
sudo kubectl get battlegroups,battlegroupdirectors,databases,databasedeployments,filebrowsers,messagequeues,servergateways,servergroups,serversets,serverstats,textrouters,pods,svc -A 2>&1 || true
'@

Write-Host "Snapshot complete: $SnapshotDir"
