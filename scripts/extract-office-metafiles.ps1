param(
    [Parameter(Mandatory = $true)][string]$Document,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [Parameter(Mandatory = $true)][string]$Provenance,
    [Parameter(Mandatory = $true)][string]$LicenseOrPermission
)

$ErrorActionPreference = 'Stop'
$documentPath = (Resolve-Path -LiteralPath $Document).Path
$outputPath = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $outputPath -Force | Out-Null

Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead($documentPath)
try {
    $entries = $archive.Entries | Where-Object {
        ($_.FullName -replace '\\', '/') -match '^(word|ppt)/media/[^/]+\.(wmf|emf)$'
    }
    $manifest = @()
    $sourceHash = ((Get-FileHash -Algorithm SHA256 -LiteralPath $documentPath).Hash).ToLowerInvariant()
    foreach ($entry in $entries) {
        $entryName = $entry.FullName -replace '\\', '/'
        $owner = ($entryName -split '/')[0]
        $fileName = [IO.Path]::GetFileName($entry.FullName)
        $targetName = "$owner-$fileName"
        $target = Join-Path $outputPath $targetName
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $target, $true)
        $format = [IO.Path]::GetExtension($targetName).TrimStart('.').ToLowerInvariant()
        $manifest += [ordered]@{
            name = "Office-extracted $targetName"
            file = "real-world/$targetName"
            origin = $Provenance
            licenseOrPermission = $LicenseOrPermission
            generator = "Extracted without modification from $([IO.Path]::GetFileName($documentPath))::$entryName"
            generatedByUs = $false
            extractedFromOwnedDocument = $true
            sourceUrl = $null
            acquiredDate = (Get-Date -Format 'yyyy-MM-dd')
            sha256 = ((Get-FileHash -Algorithm SHA256 -LiteralPath $target).Hash).ToLowerInvariant()
            expectedFormat = $format
            expectedSupport = 'render'
            placeable = $null
            notableFeatures = @('Office document extraction; record audit pending')
            notes = "Source document SHA-256: $sourceHash"
            referenceRenderer = 'Windows qualification pending'
            referenceImage = $null
            referenceImageProvenance = $null
        }
    }
    $fragment = Join-Path $outputPath 'manifest-fragment.json'
    ConvertTo-Json -InputObject @($manifest) -Depth 6 | Set-Content -LiteralPath $fragment -Encoding utf8
    Write-Host "Extracted $($manifest.Count) WMF/EMF files. Review $fragment before updating fixtures/manifest.json."
}
finally {
    $archive.Dispose()
}
