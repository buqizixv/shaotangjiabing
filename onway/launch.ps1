param(
    [Parameter(Mandatory=$true)][string]$HostExe,
    [Parameter(Mandatory=$true)][string]$DataDir,
    [Parameter(Mandatory=$true)][string]$Anchor,
    [switch]$CheckOnly
)
$ErrorActionPreference = 'Stop'
$hostPath = (Resolve-Path -LiteralPath $HostExe).Path
$dataPath = (Resolve-Path -LiteralPath $DataDir).Path
$pythonExe = (Get-Command python -ErrorAction Stop).Source
& $pythonExe -c "import sys; sys.exit(0 if sys.version_info >= (3,11) else 1)"
if ($LASTEXITCODE -ne 0) { throw 'Python 3.11+ is required.' }
$manifest = Get-Content -LiteralPath (Join-Path $dataPath 'onway/bundle/manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if ($manifest.version -ne '1.0.0') { throw 'Install the Onway 1.0.0 bundle first.' }
if ($CheckOnly) { Write-Host 'Onway 1.0.0 launcher configuration checked.'; exit 0 }
if (Get-Process octosense -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $hostPath }) { throw 'Close this host instance normally before launching.' }
$env:OCTOSENSE_APP_DATA = $dataPath
$env:OCTOSENSE_HUB = $dataPath
$env:OCTOSENSE_HUB_ANCHOR = $Anchor
$env:OCTOSENSE_HOME = Join-Path $dataPath '.octosense-home'
$env:ONWAY_APP_DIR = $PSScriptRoot
$env:ONWAY_PYTHON = $pythonExe
$env:PYTHONUTF8 = '1'
Start-Process -FilePath $hostPath -WorkingDirectory (Split-Path $hostPath) -ArgumentList '--test-action','launch-hub:onway' -WindowStyle Hidden
