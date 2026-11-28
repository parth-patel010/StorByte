<#
.SYNOPSIS
  Times directory listing and first-byte latency on a mounted StorByte drive.

.EXAMPLE
  .\bench.ps1 -Drive V -SampleFile "V:\videos\large.mkv"

.EXAMPLE
  .\bench.ps1 -Drive V -SampleFile "V:\videos\large.mkv" -Runs 10 -Json
#>
[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z]:?$')]
    [string]$Drive = 'V',

    # Folder to list. Defaults to the root of the drive.
    [string]$ListPath,

    # Large remote file used for the first-byte test.
    [Parameter(Mandatory = $true)]
    [string]$SampleFile,

    [ValidateRange(2, 100)]
    [int]$Runs = 5,

    [switch]$Json
)

$ErrorActionPreference = 'Stop'

# Matches the cache block size in apps/desktop/src-tauri/src/stream_seal.rs.
$BlockSize = 4MB

if (-not $ListPath) { $ListPath = $Drive.TrimEnd(':') + ':\' }
if (-not (Test-Path -LiteralPath $ListPath -PathType Container)) {
    throw "Folder not found: $ListPath. Is the drive mounted?"
}
if (-not (Test-Path -LiteralPath $SampleFile -PathType Leaf)) {
    throw "Sample file not found: $SampleFile"
}
$listFull = (Resolve-Path -LiteralPath $ListPath).ProviderPath
$sampleFull = (Resolve-Path -LiteralPath $SampleFile).ProviderPath

function Measure-Listing([string]$Path) {
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $count = 0
    foreach ($entry in [System.IO.Directory]::EnumerateFileSystemEntries($Path)) { $count++ }
    $watch.Stop()
    [pscustomobject]@{ Ms = $watch.Elapsed.TotalMilliseconds; Entries = $count }
}

function Measure-FirstByte([string]$Path, [long]$Offset) {
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    # A buffer size of 1 stops .NET from reading ahead on our behalf.
    $stream = New-Object System.IO.FileStream($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::Read, 1)
    try {
        [void]$stream.Seek($Offset, [System.IO.SeekOrigin]::Begin)
        if ($stream.ReadByte() -lt 0) { throw "No data at offset $Offset" }
        $watch.Stop()
    }
    finally {
        $stream.Dispose()
    }
    $watch.Elapsed.TotalMilliseconds
}

function Get-Summary([double[]]$Values) {
    $sorted = @($Values | Sort-Object)
    $n = $sorted.Count
    $middle = [int][math]::Floor($n / 2)
    if ($n % 2) { $median = $sorted[$middle] }
    else { $median = ($sorted[$middle - 1] + $sorted[$middle]) / 2 }
    [ordered]@{
        min_ms    = [math]::Round($sorted[0], 2)
        median_ms = [math]::Round($median, 2)
        max_ms    = [math]::Round($sorted[$n - 1], 2)
    }
}

$size = (Get-Item -LiteralPath $sampleFull).Length
if ($size -le 0) { throw 'Sample file is empty.' }

# The first listing is reported on its own: it is the only one that can be cold.
$listings = @(for ($i = 0; $i -lt $Runs; $i++) { Measure-Listing $listFull })

# Each cold read starts in a different cache block, spread across the file,
# so an earlier run does not serve a later one from the block cache.
$blocks = [long][math]::Ceiling($size / $BlockSize)
$samples = [int][math]::Min($Runs, $blocks)
$offsets = @(for ($i = 0; $i -lt $samples; $i++) {
        [long][math]::Floor($i * $blocks / $samples) * $BlockSize
    })
$cold = @(foreach ($offset in $offsets) { Measure-FirstByte $sampleFull $offset })
$warm = @(foreach ($offset in $offsets) { Measure-FirstByte $sampleFull $offset })

$report = [ordered]@{
    runs            = $Runs
    listing         = [ordered]@{
        entries  = $listings[0].Entries
        first_ms = [math]::Round($listings[0].Ms, 2)
        repeat   = Get-Summary @($listings | Select-Object -Skip 1 | ForEach-Object { $_.Ms })
    }
    first_byte      = [ordered]@{
        file_bytes = $size
        samples    = $samples
        first_read = Get-Summary $cold
        reread     = Get-Summary $warm
    }
}

if ($Json) {
    $report | ConvertTo-Json -Depth 4
}
else {
    $l = $report.listing
    $f = $report.first_byte
    "Directory listing ($($l.entries) entries)"
    "  first    $($l.first_ms) ms"
    "  repeat   min $($l.repeat.min_ms)  median $($l.repeat.median_ms)  max $($l.repeat.max_ms) ms"
    "First byte ($($f.file_bytes) byte file, $($f.samples) offsets)"
    "  first    min $($f.first_read.min_ms)  median $($f.first_read.median_ms)  max $($f.first_read.max_ms) ms"
    "  reread   min $($f.reread.min_ms)  median $($f.reread.median_ms)  max $($f.reread.max_ms) ms"
}
