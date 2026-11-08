$ErrorActionPreference = 'SilentlyContinue'
# Read-only snapshot of everything that affects zapret. Output is ASCII JSON; the UI translates it.
$r = [ordered]@{}

$legacy = Get-CimInstance Win32_Service -Filter "Name='zapret'"
if ($legacy) {
  $lp = Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\zapret'
  $r.legacy = [ordered]@{
    present = $true
    running = ($legacy.State -ne 'Stopped')
    automatic = ($lp.Start -eq 2)
    flowseal = [bool]($lp.'zapret-discord-youtube') -and ($lp.ImagePath -match '(?i)winws\.exe')
    strategy = [string]$lp.'zapret-discord-youtube'
    restorable = ($null -ne $lp.ZapretManagerPreviousStart)
    pid = [int]$legacy.ProcessId
  }
} else { $r.legacy = [ordered]@{ present = $false; running = $false; automatic = $false; flowseal = $false; strategy = ''; restorable = $false; pid = 0 } }

$mgr = Get-CimInstance Win32_Service -Filter "Name='ZapretManagerWinws'"
if ($mgr) {
  $mp = Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\ZapretManagerWinws'
  $r.manager = [ordered]@{
    present = $true
    owned = ($mp.ZapretManagerOwner -eq 'RetockerZapretManager')
    running = ($mgr.State -eq 'Running')
    state = [string]$mgr.State
    automatic = ($mp.Start -eq 2)
    strategy = [string]$mp.ZapretManagerStrategy
    pid = [int]$mgr.ProcessId
  }
} else { $r.manager = [ordered]@{ present = $false; owned = $false; running = $false; state = ''; automatic = $false; strategy = ''; pid = 0 } }

# winws.exe that belongs neither to our service nor to the old one: a manually started general*.bat.
$known = @($r.manager.pid, $r.legacy.pid) | Where-Object { $_ -gt 0 }
$r.foreign = @(Get-CimInstance Win32_Process -Filter "Name='winws.exe'" | Where-Object { $known -notcontains [int]$_.ProcessId } | ForEach-Object { [int]$_.ProcessId })

# Other bypasses that grab WinDivert or reroute traffic.
$svc = @()
foreach ($n in @('GoodbyeDPI','discordfix_zapret','winws1','winws2')) { if (Get-Service -Name $n) { $svc += $n } }
$r.services = $svc

# A VPN only matters while it is really on (an active tunnel adapter); an app just sitting in the tray is fine.
$vpn = @(Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' -and (($_.InterfaceDescription + ' ' + $_.Name) -match '(?i)(wintun|wireguard|tap-windows|openvpn|\btun\b|vpn|amnezia|outline|sing-?box|hiddify|happ|nekoray|xray|v2ray|clash|mihomo|warp)') } | ForEach-Object { [string]$_.Name })
# These intercept traffic whenever they run.
$always = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -match '(?i)^(goodbyedpi|adguardsvc)$' } | ForEach-Object { $_.ProcessName })
$r.apps = @(@($vpn) + @($always) | Sort-Object -Unique)

$ts = (netsh interface tcp show global) -join "`n"
$r.timestamps = if ($ts -match '(?im)1323[^:\r\n]*:\s*(\w+)') { $Matches[1] -eq 'enabled' } else { $null }
$r.proxy = ((Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings').ProxyEnable -eq 1)

$r | ConvertTo-Json -Compress -Depth 4
