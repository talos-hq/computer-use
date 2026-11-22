param(
    [Parameter(Mandatory)][string] $Binary,
    [Parameter(Mandatory)][string] $Archive
)
$ErrorActionPreference = 'Stop'
$stage = New-Item -ItemType Directory -Path ([IO.Path]::Combine([IO.Path]::GetTempPath(), [guid]::NewGuid()))
try {
    Copy-Item -LiteralPath $Binary -Destination "$stage/computer-use.exe"
    Copy-Item -LiteralPath $Binary -Destination "$stage/cua-driver.exe"
    Copy-Item -LiteralPath "$PSScriptRoot/../LICENSE.md" -Destination "$stage/LICENSE"
    Copy-Item -LiteralPath "$PSScriptRoot/../docs/release-windows.md" -Destination "$stage/README.md"
    Compress-Archive -Path "$stage/*" -DestinationPath $Archive
} finally {
    Remove-Item -LiteralPath $stage -Recurse -Force
}
