# Run in a Visual Studio developer environment (provided by GitHub's Windows runner).
$ErrorActionPreference = 'Stop'

if ($env:GPUI_FXC_PATH) {
    if (-not (Test-Path -LiteralPath $env:GPUI_FXC_PATH -PathType Leaf)) {
        throw 'GPUI_FXC_PATH does not point to a file'
    }
} else {
    $sdkBin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $compilers = Get-ChildItem -LiteralPath $sdkBin -Directory |
        Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } |
        Sort-Object { [version]$_.Name } -Descending |
        ForEach-Object { Join-Path $_.FullName 'x64\fxc.exe' } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }
    $env:GPUI_FXC_PATH = $compilers | Select-Object -First 1
    if (-not $env:GPUI_FXC_PATH) {
        throw 'Install the Windows SDK including the x64 fxc.exe shader compiler'
    }
}

Get-Command cmake -ErrorAction Stop | Out-Null
Write-Host "GPUI shader compiler: $env:GPUI_FXC_PATH"
if ($env:GITHUB_ENV) {
    "GPUI_FXC_PATH=$env:GPUI_FXC_PATH" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
}
