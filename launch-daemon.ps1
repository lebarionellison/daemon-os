Write-Host ""
Write-Host "===================================="
Write-Host "        DAEMON OS MVP"
Write-Host "     AI SECURITY COMMAND CENTER"
Write-Host "===================================="
Write-Host ""

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

Write-Host "[1/2] Starting Daemon API..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; rustup run stable-x86_64-pc-windows-gnu cargo run --target x86_64-pc-windows-gnu --bin daemon-api"

Start-Sleep -Seconds 3

Write-Host "[2/2] Starting Daemon Dashboard..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; py -m http.server 8080 --directory .\daemon-mobile\src"

Start-Sleep -Seconds 2

Write-Host ""
Write-Host "Daemon OS MVP is launching..."
Write-Host ""
Write-Host "Dashboard: http://127.0.0.1:8080"
Write-Host "API:       http://127.0.0.1:8787"
Write-Host ""
Start-Process "http://127.0.0.1:8080"
