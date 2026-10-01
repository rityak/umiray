# Blink the address table on the adapter that carries the default route (D-112), and leave
# the adapter exactly as it was. Started elevated by addr-blip.cmd; the result goes to -Mark.
#
# New-NetIPAddress on a DHCP interface switches DHCP OFF, and Remove-NetIPAddress does not
# switch it back: the adapter is left with "manual" IP and empty DNS, and the machine is
# offline (GOTCHAS). So DHCP and the DNS source are remembered first and restored after,
# and the script checks the result instead of trusting it.
param([Parameter(Mandatory = $true)][string]$Mark)

$ErrorActionPreference = 'Stop'
$route = Get-NetRoute -DestinationPrefix 0.0.0.0/0 -ErrorAction SilentlyContinue |
  Sort-Object RouteMetric | Select-Object -First 1
$adapter = if ($route) { Get-NetAdapter -InterfaceIndex $route.ifIndex } else {
  Get-NetAdapter | Where-Object { $_.Status -eq 'Up' } | Select-Object -First 1
}
if (-not $adapter) { Set-Content -Encoding utf8 $Mark 'no-adapter'; exit 1 }
$index = $adapter.ifIndex

# Before: DHCP on IPv4, and whether DNS was typed by hand (NameServer set) or came from DHCP.
$key = 'HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\' + $adapter.InterfaceGuid
$dhcp = (Get-NetIPInterface -InterfaceIndex $index -AddressFamily IPv4).Dhcp
$typedDns = (Get-ItemProperty -Path $key -Name NameServer -ErrorAction SilentlyContinue).NameServer

try {
  New-NetIPAddress -InterfaceIndex $index -IPAddress 169.254.77.77 -PrefixLength 16 `
    -SkipAsSource $true -ErrorAction SilentlyContinue | Out-Null
  Start-Sleep -Seconds 2
} finally {
  Remove-NetIPAddress -IPAddress 169.254.77.77 -Confirm:$false -ErrorAction SilentlyContinue
  if ($dhcp -eq 'Enabled') {
    Set-NetIPInterface -InterfaceIndex $index -AddressFamily IPv4 -Dhcp Enabled
  }
  if (-not $typedDns) {
    Set-DnsClientServerAddress -InterfaceIndex $index -ResetServerAddresses
  }
}

# After: the same DHCP state and DNS source, or a loud failure - never a quiet broken network.
$dhcpAfter = (Get-NetIPInterface -InterfaceIndex $index -AddressFamily IPv4).Dhcp
$typedAfter = (Get-ItemProperty -Path $key -Name NameServer -ErrorAction SilentlyContinue).NameServer
$same = ($dhcpAfter -eq $dhcp) -and ([string]$typedAfter -eq [string]$typedDns)
$state = "dhcp $dhcp -> $dhcpAfter, dns typed '$typedDns' -> '$typedAfter'"
if (-not $same) {
  Set-Content -Encoding utf8 $Mark "NOT RESTORED on $($adapter.Name): $state"
  exit 1
}
Set-Content -Encoding utf8 $Mark "blinked on $($adapter.Name); restored: $state"
