param(
    [string]$AppData = (Join-Path $PSScriptRoot '../.local-state'),
    [int]$Port = 8787
)
$ErrorActionPreference = 'Stop'
$previousKey = $env:MINIMAX_API_KEY
try {
    if (-not $env:MINIMAX_API_KEY) {
        $secureKey = Read-Host '输入 MiniMax API Key（不会显示或写入文件）' -AsSecureString
        $keyPointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secureKey)
        try { $env:MINIMAX_API_KEY = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($keyPointer) }
        finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($keyPointer) }
    }
    if (-not $env:MINIMAX_API_KEY) { throw 'MiniMax API Key 不能为空' }
    python (Join-Path $PSScriptRoot 'server.py') --app-data $AppData --port $Port
} finally {
    $env:MINIMAX_API_KEY = $previousKey
}
