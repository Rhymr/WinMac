<#
  Single source of truth for Rhymr's SemVer string, derived from git history.
  Windows counterpart of Build/version.sh — same formula (see CLAUDE.md
  § Versioning). Run via Build/version.bat, or:

    pwsh Build/version.ps1            # core string, e.g. 0.137.4
    pwsh Build/version.ps1 -Full      # full string
    pwsh Build/version.ps1 -Json
#>
[CmdletBinding()]
param(
  [switch]$Full,
  [switch]$Json
)
$ErrorActionPreference = 'Stop'

Set-Location (git -C $PSScriptRoot rev-parse --show-toplevel)

$featRe = '^feat(\(.+\))?!?: '
$fixRe  = '^fix(\(.+\))?!?: '

# base tag
$baseTag = (git describe --tags --match 'v[1-9]*.*.*' --abbrev=0 2>$null)
if ($LASTEXITCODE -ne 0) { $baseTag = '' }

if ($baseTag) {
  $major = ($baseTag.TrimStart('v') -split '\.')[0]
  $range = "$baseTag..HEAD"
} else {
  $major = '0'
  $range = 'HEAD'
}

$subjects = @(git log $range --format='%s')
$minor = @($subjects | Where-Object { $_ -match $featRe }).Count

$lastFeat = @(git log $range --format='%H %s' |
  Where-Object { $_ -match " feat(\(.+\))?!?: " } | Select-Object -First 1)
if ($lastFeat) {
  $sha0 = ($lastFeat -split ' ')[0]
  $patch = @(git log "$sha0..HEAD" --format='%s' | Where-Object { $_ -match $fixRe }).Count
} else {
  $patch = @($subjects | Where-Object { $_ -match $fixRe }).Count
}

$count = (git rev-list --count HEAD).Trim()
$sha   = (git rev-parse --short=7 HEAD).Trim()
git diff --quiet --ignore-submodules HEAD 2>$null
$dirty = if ($LASTEXITCODE -ne 0) { '.dirty' } else { '' }

$pre = ''
if ($major -ne '0' -and $env:RHYMR_VERSION_PRERELEASE) { $pre = "-$($env:RHYMR_VERSION_PRERELEASE)" }

$core = "$major.$minor.$patch$pre"
$full = "$core+build.$count.g$sha$dirty"

if ($Json) {
  [pscustomobject]@{
    core = $core; full = $full; major = [int]$major; minor = $minor
    patch = $patch; count = [int]$count; sha = $sha; dirty = [bool]$dirty
  } | ConvertTo-Json -Compress
} elseif ($Full) {
  $full
} else {
  $core
}
