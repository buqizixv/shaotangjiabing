$ErrorActionPreference = 'Stop'
$taskApp = Split-Path $PSScriptRoot -Parent
$taskRuntime = Join-Path $taskApp '_debug/gateway-tools'
$taskPython = Join-Path $taskRuntime 'Scripts/python.exe'
if (!(Test-Path -LiteralPath $taskPython)) {
    python -m venv $taskRuntime
    if ($LASTEXITCODE -ne 0) { throw 'Could not prepare deployment Python.' }
    & $taskPython -m pip install --quiet 'paramiko>=3,<5'
    if ($LASTEXITCODE -ne 0) { throw 'Could not install SSH deployment dependency.' }
}
& $taskPython -X utf8 (Join-Path $PSScriptRoot 'deploy_ip.py')
if ($LASTEXITCODE -ne 0) { Write-Host 'Deployment did not complete. No credentials were printed.' }
Read-Host 'Press Enter to close'
