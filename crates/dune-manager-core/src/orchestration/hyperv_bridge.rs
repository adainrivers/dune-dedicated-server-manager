use serde::Deserialize;

use crate::{
    errors::failure,
    models::CommandResult,
    orchestration::{
        powershell_json_command, DriveCandidate, EnsureSwitchRequest, ExternalSwitch, HostProvider,
        HostReadiness, NetworkAdapterCandidate, StrictCommandRunner, VmCompatibilityReport,
        VmImportRequest, VmInventoryRecord, VmPowerState, VmProvider,
    },
    shell::ps_single_quoted,
};

/// Hyper-V provider implemented through strict JSON PowerShell commands.
#[derive(Debug, Clone)]
pub struct StrictPowerShellHyperV {
    runner: StrictCommandRunner,
}

impl StrictPowerShellHyperV {
    /// Creates a Hyper-V bridge that invokes local PowerShell.
    pub fn new() -> Self {
        Self {
            runner: StrictCommandRunner,
        }
    }

    fn run_json<T: for<'de> Deserialize<'de>>(
        &self,
        id: &'static str,
        script: String,
    ) -> CommandResult<T> {
        self.runner.run_json(&powershell_json_command(id, &script))
    }

    fn run_unit(&self, id: &'static str, script: String) -> CommandResult<()> {
        let output: UnitOutput = self.run_json(id, script)?;
        if !output.ok {
            return Err(failure(format!("{id} returned ok=false")));
        }
        Ok(())
    }
}

impl Default for StrictPowerShellHyperV {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnitOutput {
    ok: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawVmRecord {
    name: String,
    state: String,
    configuration_location: String,
    path: String,
    memory_assigned_bytes: Option<u64>,
    processor_count: Option<u32>,
    uptime_seconds: Option<u64>,
    ipv4_addresses: Vec<String>,
    hard_disk_paths: Vec<String>,
    disk_size_bytes: Option<u64>,
    disk_file_size_bytes: Option<u64>,
    switch_names: Vec<String>,
}

impl From<RawVmRecord> for VmInventoryRecord {
    fn from(value: RawVmRecord) -> Self {
        Self {
            name: value.name,
            state: VmPowerState::from_hyperv_state(&value.state),
            raw_state: value.state,
            configuration_location: value.configuration_location,
            path: value.path,
            memory_assigned_bytes: value.memory_assigned_bytes.unwrap_or_default(),
            processor_count: value.processor_count.unwrap_or_default(),
            uptime_seconds: value.uptime_seconds.unwrap_or_default(),
            ipv4_addresses: value.ipv4_addresses,
            hard_disk_paths: value.hard_disk_paths,
            disk_size_bytes: value.disk_size_bytes.unwrap_or_default(),
            disk_file_size_bytes: value.disk_file_size_bytes.unwrap_or_default(),
            switch_names: value.switch_names,
        }
    }
}

impl HostProvider for StrictPowerShellHyperV {
    fn readiness(&self) -> CommandResult<HostReadiness> {
        self.run_json(
            "hyperv.host.readiness",
            r#"
$ErrorActionPreference = 'Stop'
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
$vmms = Get-Service -Name vmms -ErrorAction SilentlyContinue
$cpu = Get-CimInstance -ClassName Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1
$os = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction SilentlyContinue | Select-Object -First 1
$processors = @(Get-CimInstance -ClassName Win32_Processor -ErrorAction SilentlyContinue)
$vmHost = Get-VMHost -ErrorAction SilentlyContinue
$logicalProcessorCount = [uint32]0
foreach ($processor in $processors) {
  if ($processor.NumberOfLogicalProcessors) {
    $logicalProcessorCount += [uint32]$processor.NumberOfLogicalProcessors
  }
}
[pscustomobject]@{
  elevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  hypervAvailable = [bool](Get-Command Get-VM -ErrorAction SilentlyContinue)
  vmmsRunning = if ($vmms) { $vmms.Status.ToString() -eq 'Running' } else { $false }
  virtualizationFirmwareEnabled = if ($vmHost) { $true } elseif ($cpu) { [bool]$cpu.VirtualizationFirmwareEnabled } else { $null }
  totalPhysicalMemoryBytes = if ($os) { [uint64]$os.TotalVisibleMemorySize * 1024 } else { 0 }
  availablePhysicalMemoryBytes = if ($os) { [uint64]$os.FreePhysicalMemory * 1024 } else { 0 }
  logicalProcessorCount = $logicalProcessorCount
} | ConvertTo-Json -Compress -Depth 4
"#
            .to_string(),
        )
    }

    fn drives_with_minimum_free_space(
        &self,
        minimum_free_bytes: u64,
    ) -> CommandResult<Vec<DriveCandidate>> {
        self.run_json(
            "hyperv.host.drives",
            format!(
                r#"
$ErrorActionPreference = 'Stop'
$items = @(Get-PSDrive -PSProvider FileSystem |
  Where-Object {{ $_.Free -ge {minimum_free_bytes} }} |
  Sort-Object Name |
  ForEach-Object {{
    [pscustomobject]@{{
      name = $_.Name
      root = $_.Root
      freeBytes = [uint64]$_.Free
    }}
  }})
ConvertTo-Json -InputObject $items -Compress -Depth 4
"#
            ),
        )
    }

    fn active_physical_adapters(&self) -> CommandResult<Vec<NetworkAdapterCandidate>> {
        self.run_json(
            "hyperv.host.network-adapters",
            r#"
$ErrorActionPreference = 'Stop'
function ConvertTo-IPv4Int($address) {
  $bytes = [System.Net.IPAddress]::Parse($address).GetAddressBytes()
  [Array]::Reverse($bytes)
  return [BitConverter]::ToUInt32($bytes, 0)
}
function ConvertFrom-IPv4Int([uint32]$value) {
  $bytes = [BitConverter]::GetBytes($value)
  [Array]::Reverse($bytes)
  return ([System.Net.IPAddress]::new($bytes)).ToString()
}
function Get-SuggestedIPv4Address($address, [int]$prefixLength, $gateway) {
  if ([string]::IsNullOrWhiteSpace($address) -or $prefixLength -lt 1 -or $prefixLength -gt 30) {
    return ''
  }
  $ipInt = ConvertTo-IPv4Int $address
  $mask = [uint32]::MaxValue -shl (32 - $prefixLength)
  $network = $ipInt -band $mask
  $broadcast = $network -bor (-bnot $mask)
  $reserved = @{}
  @($address, $gateway) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | ForEach-Object {
    $reserved[$_] = $true
  }
  Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
    ForEach-Object { $reserved[$_.IPAddress] = $true }
  Get-NetNeighbor -AddressFamily IPv4 -ErrorAction SilentlyContinue |
    Where-Object { $_.State -in @('Reachable', 'Stale', 'Delay', 'Probe', 'Permanent') } |
    ForEach-Object { $reserved[$_.IPAddress] = $true }
  for ($candidate = [uint32]($broadcast - 2); $candidate -gt [uint32]($network + 1); $candidate--) {
    $candidateIp = ConvertFrom-IPv4Int $candidate
    if (-not $reserved.ContainsKey($candidateIp)) {
      return $candidateIp
    }
  }
  return ''
}
$switches = @(Get-VMSwitch -SwitchType External -ErrorAction SilentlyContinue)
$adapters = @(Get-NetAdapter)
$items = @(Get-NetAdapter |
  Where-Object { $_.Status -eq 'Up' -and $_.HardwareInterface -eq $true } |
  ForEach-Object {
    $adapter = $_
    $boundSwitch = $switches | Where-Object { $_.NetAdapterInterfaceDescription -eq $adapter.InterfaceDescription } | Select-Object -First 1
    $ipAdapter = $adapter
    if ($boundSwitch) {
      $managementAdapterName = "vEthernet ($($boundSwitch.Name))"
      $managementAdapter = $adapters | Where-Object { $_.Name -eq $managementAdapterName } | Select-Object -First 1
      if ($managementAdapter) {
        $ipAdapter = $managementAdapter
      }
    }
    $ip = Get-NetIPAddress -InterfaceIndex $ipAdapter.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue |
      Where-Object { $_.IPAddress -notlike '169.254.*' } |
      Select-Object -First 1
    $route = Get-NetRoute -InterfaceIndex $ipAdapter.ifIndex -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue |
      Sort-Object RouteMetric |
      Select-Object -First 1
    if ($ip -and $route -and -not [string]::IsNullOrWhiteSpace($route.NextHop) -and $route.NextHop -ne '0.0.0.0') {
      [pscustomobject]@{
        name = $adapter.Name
        interfaceDescription = $adapter.InterfaceDescription
        ipv4Address = $ip.IPAddress
        prefixLength = [int]$ip.PrefixLength
        gateway = $route.NextHop
        suggestedIpv4Address = Get-SuggestedIPv4Address $ip.IPAddress ([int]$ip.PrefixLength) $route.NextHop
        existingExternalSwitch = if ($boundSwitch) { $boundSwitch.Name } else { '' }
      }
    }
  })
ConvertTo-Json -InputObject $items -Compress -Depth 5
"#
            .to_string(),
        )
    }
}

impl VmProvider for StrictPowerShellHyperV {
    fn list_vms(&self) -> CommandResult<Vec<VmInventoryRecord>> {
        let raw: Vec<RawVmRecord> = self.run_json(
            "hyperv.vm.list",
            r#"
$ErrorActionPreference = 'Stop'
$items = @(Get-VM | Sort-Object Name | ForEach-Object {
  $vm = $_
  $adapters = @(Get-VMNetworkAdapter -VMName $vm.Name -ErrorAction SilentlyContinue)
  $ips = @($adapters.IPAddresses | Where-Object { $_ -match '^\d+\.\d+\.\d+\.\d+$' })
  $switches = @($adapters | ForEach-Object { $_.SwitchName } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object -Unique)
  $disks = @(Get-VMHardDiskDrive -VMName $vm.Name -ErrorAction SilentlyContinue | ForEach-Object { $_.Path })
  $diskSizeBytes = [uint64]0
  $diskFileSizeBytes = [uint64]0
  foreach ($diskPath in $disks) {
    $vhd = Get-VHD -Path $diskPath -ErrorAction SilentlyContinue
    if ($vhd) {
      $diskSizeBytes += [uint64]$vhd.Size
      $diskFileSizeBytes += [uint64]$vhd.FileSize
    }
  }
  [pscustomobject]@{
    name = $vm.Name
    state = $vm.State.ToString()
    configurationLocation = $vm.ConfigurationLocation
    path = $vm.Path
    memoryAssignedBytes = [uint64]$vm.MemoryAssigned
    processorCount = [uint32]$vm.ProcessorCount
    uptimeSeconds = [uint64]$vm.Uptime.TotalSeconds
    ipv4Addresses = $ips
    hardDiskPaths = $disks
    diskSizeBytes = $diskSizeBytes
    diskFileSizeBytes = $diskFileSizeBytes
    switchNames = $switches
  }
})
ConvertTo-Json -InputObject $items -Compress -Depth 6
"#
            .to_string(),
        )?;
        Ok(raw.into_iter().map(Into::into).collect())
    }

    fn get_vm(&self, name: &str) -> CommandResult<Option<VmInventoryRecord>> {
        let raw: Option<RawVmRecord> = self.run_json(
            "hyperv.vm.get",
            format!(
                r#"
$ErrorActionPreference = 'Stop'
$vmName = {name}
$vm = Get-VM -Name $vmName -ErrorAction SilentlyContinue
if (-not $vm) {{
  [Console]::Out.Write('null')
  exit 0
}}
$adapters = @(Get-VMNetworkAdapter -VMName $vm.Name -ErrorAction SilentlyContinue)
$ips = @($adapters.IPAddresses | Where-Object {{ $_ -match '^\d+\.\d+\.\d+\.\d+$' }})
$switches = @($adapters | ForEach-Object {{ $_.SwitchName }} | Where-Object {{ -not [string]::IsNullOrWhiteSpace($_) }} | Sort-Object -Unique)
$disks = @(Get-VMHardDiskDrive -VMName $vm.Name -ErrorAction SilentlyContinue | ForEach-Object {{ $_.Path }})
$diskSizeBytes = [uint64]0
$diskFileSizeBytes = [uint64]0
foreach ($diskPath in $disks) {{
  $vhd = Get-VHD -Path $diskPath -ErrorAction SilentlyContinue
  if ($vhd) {{
    $diskSizeBytes += [uint64]$vhd.Size
    $diskFileSizeBytes += [uint64]$vhd.FileSize
  }}
}}
[pscustomobject]@{{
  name = $vm.Name
  state = $vm.State.ToString()
  configurationLocation = $vm.ConfigurationLocation
  path = $vm.Path
  memoryAssignedBytes = [uint64]$vm.MemoryAssigned
  processorCount = [uint32]$vm.ProcessorCount
  uptimeSeconds = [uint64]$vm.Uptime.TotalSeconds
  ipv4Addresses = $ips
  hardDiskPaths = $disks
  diskSizeBytes = $diskSizeBytes
  diskFileSizeBytes = $diskFileSizeBytes
  switchNames = $switches
}} | ConvertTo-Json -Compress -Depth 5
"#,
                name = ps_single_quoted(name)
            ),
        )?;
        Ok(raw.map(Into::into))
    }

    fn compare_import(&self, request: &VmImportRequest) -> CommandResult<VmCompatibilityReport> {
        self.run_json(
            "hyperv.vm.compare-import",
            format!(
                r#"
$ErrorActionPreference = 'Stop'
$report = Compare-VM -Path {vmcx} -Copy -VirtualMachinePath {dest} -VhdDestinationPath (Join-Path {dest} 'Virtual Hard Disks') -ErrorAction Stop
$messages = @($report.Incompatibilities | ForEach-Object {{ $_.Message }})
[pscustomobject]@{{
  compatible = $messages.Count -eq 0
  incompatibilities = $messages
}} | ConvertTo-Json -Compress -Depth 6
"#,
                vmcx = ps_single_quoted(&request.vmcx_path),
                dest = ps_single_quoted(&request.destination_path)
            ),
        )
    }

    fn import_vm(
        &self,
        request: &VmImportRequest,
    ) -> CommandResult<crate::orchestration::ImportedVm> {
        self.run_json(
            "hyperv.vm.import",
            format!(
                r#"
$ErrorActionPreference = 'Stop'
$report = Compare-VM -Path {vmcx} -Copy -VirtualMachinePath {dest} -VhdDestinationPath (Join-Path {dest} 'Virtual Hard Disks') -ErrorAction Stop
$vm = Import-VM -CompatibilityReport $report -ErrorAction Stop
[pscustomobject]@{{
  name = $vm.Name
  configurationLocation = $vm.ConfigurationLocation
}} | ConvertTo-Json -Compress -Depth 4
"#,
                vmcx = ps_single_quoted(&request.vmcx_path),
                dest = ps_single_quoted(&request.destination_path)
            ),
        )
    }

    fn remove_vm(&self, name: &str) -> CommandResult<()> {
        self.run_unit(
            "hyperv.vm.remove",
            format!(
                "Remove-VM -Name {} -Force -ErrorAction Stop; [pscustomobject]@{{ ok = $true }} | ConvertTo-Json -Compress",
                ps_single_quoted(name)
            ),
        )
    }

    fn start_vm(&self, name: &str) -> CommandResult<()> {
        self.run_unit(
            "hyperv.vm.start",
            format!(
                "Start-VM -Name {} -ErrorAction Stop | Out-Null; [pscustomobject]@{{ ok = $true }} | ConvertTo-Json -Compress",
                ps_single_quoted(name)
            ),
        )
    }

    fn stop_vm(&self, name: &str, turn_off: bool) -> CommandResult<()> {
        let flag = if turn_off { " -TurnOff" } else { "" };
        self.run_unit(
            "hyperv.vm.stop",
            format!(
                "Stop-VM -Name {}{flag} -Force -ErrorAction Stop | Out-Null; [pscustomobject]@{{ ok = $true }} | ConvertTo-Json -Compress",
                ps_single_quoted(name)
            ),
        )
    }

    fn connect_network_adapter(&self, vm_name: &str, switch_name: &str) -> CommandResult<()> {
        self.run_unit(
            "hyperv.vm.connect-network-adapter",
            format!(
                "Connect-VMNetworkAdapter -VMName {} -SwitchName {} -ErrorAction Stop; [pscustomobject]@{{ ok = $true }} | ConvertTo-Json -Compress",
                ps_single_quoted(vm_name),
                ps_single_quoted(switch_name)
            ),
        )
    }

    fn ensure_external_switch(
        &self,
        request: &EnsureSwitchRequest,
    ) -> CommandResult<ExternalSwitch> {
        self.run_json(
            "hyperv.switch.ensure-external",
            format!(
                r#"
$ErrorActionPreference = 'Stop'
$switchName = {switch_name}
$adapterName = {adapter_name}
$adapter = Get-NetAdapter -Name $adapterName -ErrorAction Stop
$switch = Get-VMSwitch -SwitchType External -ErrorAction SilentlyContinue |
  Where-Object {{ $_.NetAdapterInterfaceDescription -eq $adapter.InterfaceDescription }} |
  Select-Object -First 1
if (-not $switch) {{
  $switch = New-VMSwitch -Name $switchName -NetAdapterName $adapterName -AllowManagementOS $true -ErrorAction Stop
}}
[pscustomobject]@{{
  name = $switch.Name
  netAdapterInterfaceDescription = $switch.NetAdapterInterfaceDescription
}} | ConvertTo-Json -Compress -Depth 4
"#,
                switch_name = ps_single_quoted(&request.switch_name),
                adapter_name = ps_single_quoted(&request.adapter_name)
            ),
        )
    }

    fn resize_first_vhd(&self, vm_name: &str, size_bytes: u64) -> CommandResult<()> {
        self.run_unit(
            "hyperv.vhd.resize-first",
            format!(
                r#"
$ErrorActionPreference = 'Stop'
$drive = Get-VMHardDiskDrive -VMName {vm_name} | Select-Object -First 1
if (-not $drive) {{ throw 'VM has no hard disk drive' }}
Resize-VHD -Path $drive.Path -SizeBytes {size_bytes} -ErrorAction Stop
[pscustomobject]@{{ ok = $true }} | ConvertTo-Json -Compress
"#,
                vm_name = ps_single_quoted(vm_name)
            ),
        )
    }

    fn set_first_boot_disk(&self, vm_name: &str) -> CommandResult<()> {
        self.run_unit(
            "hyperv.vm.set-first-boot-disk",
            format!(
                r#"
$ErrorActionPreference = 'Stop'
$drive = Get-VMHardDiskDrive -VMName {vm_name} | Select-Object -First 1
if (-not $drive) {{ throw 'VM has no hard disk drive' }}
Set-VMFirmware -VMName {vm_name} -FirstBootDevice $drive -ErrorAction Stop
[pscustomobject]@{{ ok = $true }} | ConvertTo-Json -Compress
"#,
                vm_name = ps_single_quoted(vm_name)
            ),
        )
    }

    fn set_startup_memory(&self, vm_name: &str, bytes: u64) -> CommandResult<()> {
        if bytes == 0 {
            return Err(failure("VM memory must be greater than zero"));
        }
        self.run_unit(
            "hyperv.vm.set-startup-memory",
            format!(
                "Set-VMMemory -VMName {} -StartupBytes {bytes} -ErrorAction Stop; [pscustomobject]@{{ ok = $true }} | ConvertTo-Json -Compress",
                ps_single_quoted(vm_name)
            ),
        )
    }

    fn set_processor_count(&self, vm_name: &str, count: u32) -> CommandResult<()> {
        if count == 0 {
            return Err(failure("VM processor count must be greater than zero"));
        }
        self.run_unit(
            "hyperv.vm.set-processor-count",
            format!(
                "Set-VMProcessor -VMName {} -Count {count} -ErrorAction Stop; [pscustomobject]@{{ ok = $true }} | ConvertTo-Json -Compress",
                ps_single_quoted(vm_name)
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::StrictCommandSpec;

    #[test]
    fn powershell_json_command_uses_noninteractive_mode() {
        let spec: StrictCommandSpec =
            powershell_json_command("test", "[pscustomobject]@{ok=$true}|ConvertTo-Json");
        assert_eq!(spec.program, "powershell");
        assert!(spec.args.contains(&"-NonInteractive".to_string()));
        assert!(spec.args.iter().any(|arg| arg.contains("ConvertTo-Json")));
    }

    #[test]
    fn bridge_escapes_single_quotes_in_vm_name() {
        let script = format!(
            "Start-VM -Name {} -ErrorAction Stop",
            ps_single_quoted("bad'name")
        );
        assert!(script.contains("'bad''name'"));
    }

    #[test]
    fn missing_vm_script_emits_json_null() {
        let script = format!(
            r#"
$vmName = {}
$vm = Get-VM -Name $vmName -ErrorAction SilentlyContinue
if (-not $vm) {{
  [Console]::Out.Write('null')
  exit 0
}}
"#,
            ps_single_quoted("sample")
        );
        assert!(script.contains("[Console]::Out.Write('null')"));
    }
}
