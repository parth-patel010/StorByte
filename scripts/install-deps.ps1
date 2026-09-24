#Requires -RunAsAdministrator
<#
.SYNOPSIS
  Install Stroex desktop build dependencies: WinFsp + VS 2022 Build Tools (C++).
#>
$ErrorActionPreference = "Stop"

$bt = "$env:TEMP\vs_buildtools.exe"
$winfsp = "$env:TEMP\winfsp.msi"

Write-Host "Downloading WinFsp..."
$rel = Invoke-RestMethod "https://api.github.com/repos/winfsp/winfsp/releases/latest"
$msiUrl = ($rel.assets | Where-Object { $_.name -like "*.msi" } | Select-Object -First 1).browser_download_url
Invoke-WebRequest -Uri $msiUrl -OutFile $winfsp

Write-Host "Installing WinFsp..."
$p = Start-Process msiexec.exe -ArgumentList "/i `"$winfsp`" /qn /norestart" -Wait -PassThru
if ($p.ExitCode -ne 0 -and $p.ExitCode -ne 3010) {
  throw "WinFsp install failed: exit $($p.ExitCode)"
}

Write-Host "Downloading VS Build Tools..."
Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vs_buildtools.exe" -OutFile $bt

Write-Host "Installing Visual Studio Build Tools (C++). This can take a long time..."
$p2 = Start-Process -FilePath $bt -ArgumentList @(
  "--wait", "--quiet",
  "--add", "Microsoft.VisualStudio.Workload.VCTools",
  "--includeRecommended",
  "--add", "Microsoft.VisualStudio.Component.Windows11SDK.22621",
  "--norestart"
) -Wait -PassThru
Write-Host "VS installer exit: $($p2.ExitCode)"

Write-Host "Done. Reboot if WinFsp requests it, then:"
Write-Host "  rustup default stable-x86_64-pc-windows-msvc"
Write-Host "  cd apps\desktop && npm run tauri dev"
