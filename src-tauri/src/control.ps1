$ErrorActionPreference = 'Stop'
$env:PSModulePath = "$PSHOME\Modules;$env:PSModulePath"
try {
  $p = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__PAYLOAD__')) | ConvertFrom-Json
  $serviceName = 'ZapretManagerWinws'
  $key = 'HKLM:\SYSTEM\CurrentControlSet\Services\ZapretManagerWinws'
  $defaultBase = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Zapret Manager\versions'
  $base = $defaultBase
  if ($p.base) {
    # Custom folder from settings: local drive path, always ends with our own ZapretManager subfolder.
    if ($p.base -notmatch '^[a-zA-Z]:\\[^"%<>|?*]*\\ZapretManager$' -or $p.base -match '\\\.\.?(\\|$)') { exit 13 }
    $base = $p.base
  }
  $existing = Get-CimInstance Win32_Service -Filter "Name='ZapretManagerWinws'"
  if ($existing) {
    $props = Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue
    # The install folder may have been changed since the service was created, so check ownership and the winws binary only.
    if ($props.ZapretManagerOwner -ne 'RetockerZapretManager' -or $props.ImagePath -notmatch '(?i)^"[a-z]:\\[^"]+\\bin\\winws\.exe"') { exit 12 }
  }
  if ($p.action -eq 'connect') {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ZapretServiceNative {
  [DllImport("advapi32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr OpenSCManager(string machine, string database, uint access);
  [DllImport("advapi32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr OpenService(IntPtr manager, string name, uint access);
  [DllImport("advapi32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool ChangeServiceConfig(IntPtr service, uint type, uint start, uint error, string path, string group, IntPtr tag, string deps, string account, string password, string display);
  [DllImport("advapi32.dll", SetLastError=true)] static extern bool CloseServiceHandle(IntPtr handle);
  public static void Change(string name, string path, uint start) {
    IntPtr m = OpenSCManager(null, null, 1);
    if (m == IntPtr.Zero) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
    try {
      IntPtr s = OpenService(m, name, 2);
      if (s == IntPtr.Zero) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
      try { if (!ChangeServiceConfig(s, 0xffffffff, start, 0xffffffff, path, null, IntPtr.Zero, null, null, null, null)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error()); }
      finally { CloseServiceHandle(s); }
    } finally { CloseServiceHandle(m); }
  }
}
'@
    $old = Get-CimInstance Win32_Service -Filter "Name='zapret'"
    if ($old) { $oldProps=Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\zapret'; if ($old.State -ne 'Stopped' -or $oldProps.Start -eq 2) {exit 10} }
    $ourPid = if ($existing) { [int]$existing.ProcessId } else { 0 }
    $other = @(Get-CimInstance Win32_Process -Filter "Name='winws.exe'" | Where-Object { $_.ProcessId -ne $ourPid })
    if ($other.Count -gt 0) { exit 11 }
    if ($p.sha -notmatch '^[a-f0-9]{64}$' -or $p.root -notmatch '^[a-zA-Z0-9._-]+$' -or $p.root -in @('.','..')) { exit 13 }
    New-Item -ItemType Directory -Force -Path $base | Out-Null
    if ($base -ne $defaultBase) {
      # The service runs as SYSTEM: a user-writable folder would let any program swap winws.exe. Lock it like Program Files.
      & icacls.exe $base /inheritance:r /grant:r '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-545:(OI)(CI)RX' /T /C /Q | Out-Null
      if ($LASTEXITCODE -ne 0) { exit 13 }
      & icacls.exe $base /setowner '*S-1-5-32-544' /T /C /Q | Out-Null
    }
    $version = Join-Path $base $p.sha
    $ready = Join-Path $version '.ready'
    $copy = Join-Path $version $p.root
    $whole = (Test-Path $ready) -and (Test-Path (Join-Path $copy 'bin\winws.exe')) -and (Test-Path (Join-Path $copy 'service.bat'))
    if ((Test-Path $version) -and -not $whole) {
      # Broken copy: unpacking was interrupted or an antivirus removed winws.exe. Unpack it again from scratch.
      $img = if ($existing) { [string]$props.ImagePath } else { '' }
      if ($img -and $img.StartsWith('"' + $version + '\', [StringComparison]::OrdinalIgnoreCase) -and (Get-Service -Name $serviceName).Status -ne 'Stopped') {
        Stop-Service -Name $serviceName -Force; (Get-Service -Name $serviceName).WaitForStatus('Stopped',[TimeSpan]::FromSeconds(20))
      }
      try { Remove-Item -LiteralPath $version -Recurse -Force -ErrorAction Stop } catch { exit 16 }
    }
    if (-not (Test-Path $version)) {
      New-Item -ItemType Directory -Path $version | Out-Null
      $archive = Join-Path $version 'official.zip'
      Copy-Item -LiteralPath $p.source -Destination $archive
      $hasher=[Security.Cryptography.SHA256]::Create()
      $stream=[IO.File]::OpenRead($archive)
      try {$actual=([BitConverter]::ToString($hasher.ComputeHash($stream))).Replace('-','').ToLowerInvariant()} finally {$stream.Dispose();$hasher.Dispose()}
      if ($actual -ne $p.sha) { exit 13 }
      [Reflection.Assembly]::LoadWithPartialName('System.IO.Compression.FileSystem') | Out-Null
      try { [IO.Compression.ZipFile]::ExtractToDirectory($archive,$version) } catch { exit 16 }
      $folder = Join-Path $version $p.root
      if (-not (Test-Path (Join-Path $folder 'bin\winws.exe')) -or -not (Test-Path (Join-Path $folder 'service.bat'))) { exit 16 }
      $lists=Join-Path $folder 'lists'
      foreach ($entry in @(@('ipset-exclude-user.txt','203.0.113.113/32'),@('list-general-user.txt',"# Never leave this file empty`r`ndomain.example.abc"),@('list-exclude-user.txt','domain.example.abc'))) {
        $file=Join-Path $lists $entry[0]; if (-not (Test-Path $file)) { [IO.File]::WriteAllText($file,$entry[1]+"`r`n") }
      }
      [IO.File]::WriteAllText($ready, $p.sha)
    }
    $folder=Join-Path $version $p.root
    $exe=Join-Path $folder 'bin\winws.exe'
    if (-not (Test-Path $exe)) { exit 16 }
    # User's own sites go to list-general-user.txt (read by every general*.bat next to the stock list-general.txt).
    # Rewritten on every connect, so a fresh release keeps its new stock list and gets the user's additions on top.
    $userDomains = @($p.domains | Where-Object { $_ -match '^[a-z0-9-]+(\.[a-z0-9-]+)+$' })
    $userText = if ($userDomains.Count) { ($userDomains -join "`r`n") } else { "# Never leave this file empty`r`ndomain.example.abc" }
    [IO.File]::WriteAllText((Join-Path $folder 'lists\list-general-user.txt'), $userText + "`r`n")
    $binary='"'+$exe+'" '+$p.args
    # Same as the official service.bat: fooling=ts strategies need TCP timestamps.
    try { $g = (netsh interface tcp show global) -join "`n"; if ($g -notmatch '(?im)1323[^:\r\n]*:\s*enabled') { netsh interface tcp set global timestamps=enabled | Out-Null } } catch {}
    $oldPath = $null; $oldMode = $null; $wasRunning=$false
    if ($existing) {
      $oldPath=$props.ImagePath; $oldMode=[int]$props.Start; $wasRunning=((Get-Service -Name $serviceName).Status -eq 'Running')
      if ($wasRunning) {Stop-Service -Name $serviceName; (Get-Service -Name $serviceName).WaitForStatus('Stopped',[TimeSpan]::FromSeconds(20))}
      # The user's remembered autostart choice wins; without one keep what the service had.
      $mode=if($null -ne $p.auto){if($p.auto){2}else{3}}elseif($oldMode -eq 2){2}else{3}
      try { [ZapretServiceNative]::Change($serviceName,$binary,$mode) }
      catch { if ($env:ZAPRET_SERVICE_TEST -eq '1') {[Console]::Error.WriteLine('ChangeService error: '+$_.Exception.ToString())}; if ($wasRunning) {Start-Service -Name $serviceName -ErrorAction SilentlyContinue}; exit 14 }
    } else {
      New-Service -Name $serviceName -BinaryPathName $binary -DisplayName 'Zapret Manager' -StartupType Manual | Out-Null
      New-ItemProperty -LiteralPath $key -Name ZapretManagerOwner -PropertyType String -Value 'RetockerZapretManager' -Force | Out-Null
    }
    try {
      Start-Service -Name $serviceName
      (Get-Service -Name $serviceName).WaitForStatus('Running',[TimeSpan]::FromSeconds(20))
      Start-Sleep -Seconds 3
      if ((Get-Service -Name $serviceName).Status -ne 'Running') {throw 'Service stopped immediately after startup'}
      New-ItemProperty -LiteralPath $key -Name ZapretManagerStrategy -PropertyType String -Value $p.strategy -Force | Out-Null
      if (-not $existing -and $p.auto -eq $true) { Set-Service -Name $serviceName -StartupType Automatic }
    } catch {
      if ($env:ZAPRET_SERVICE_TEST -eq '1') {
        [Console]::Error.WriteLine("Start error: " + $_.Exception.ToString())
        $debugSvc=Get-CimInstance Win32_Service -Filter "Name='ZapretManagerWinws'"
        [Console]::Error.WriteLine(($debugSvc | Select-Object State,StartMode,ExitCode,ServiceSpecificExitCode,ProcessId,PathName | ConvertTo-Json -Compress))
      }
      if ($oldPath) {
        $mode=if($oldMode -eq 2){2}elseif($oldMode -eq 4){4}else{3}
        [ZapretServiceNative]::Change($serviceName,$oldPath,$mode)
        if ($wasRunning -and $mode -ne 4) {Start-Service -Name $serviceName -ErrorAction SilentlyContinue}
      }
      exit 14
    }
  } elseif ($p.action -eq 'kill_foreign') {
    # Close winws started by hand (general*.bat). Never touches the old service process: it must be disabled first.
    $legacySvc = Get-CimInstance Win32_Service -Filter "Name='zapret'"
    if ($legacySvc -and $legacySvc.State -ne 'Stopped') { exit 10 }
    $keep = if ($existing) { [int]$existing.ProcessId } else { 0 }
    Get-CimInstance Win32_Process -Filter "Name='winws.exe'" | Where-Object { [int]$_.ProcessId -ne $keep } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
  } elseif ($p.action -eq 'cleanup') {
    # Delete unpacked old versions chosen by the user. Never the folder the service runs from.
    $inUse = if ($existing) { [string]$props.ImagePath } else { '' }
    foreach ($sha in @($p.remove)) {
      if ([string]$sha -notmatch '^[a-f0-9]{64}$') { exit 13 }
      if ($inUse -and $inUse.IndexOf('\' + $sha + '\', [StringComparison]::OrdinalIgnoreCase) -ge 0) { continue }
      $dir = Join-Path $base $sha
      if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force }
    }
  } elseif (-not $existing) {exit 15}
  elseif ($p.action -eq 'disconnect') {
    Set-Service -Name $serviceName -StartupType Manual
    if ((Get-Service -Name $serviceName).Status -ne 'Stopped') {Stop-Service -Name $serviceName; (Get-Service -Name $serviceName).WaitForStatus('Stopped',[TimeSpan]::FromSeconds(20))}
  } elseif ($p.action -eq 'auto_on') {Set-Service -Name $serviceName -StartupType Automatic}
  elseif ($p.action -eq 'auto_off') {Set-Service -Name $serviceName -StartupType Manual}
  else {exit 1}
  exit 0
} catch { [Console]::Error.WriteLine($_.Exception.ToString()); exit 1 }
