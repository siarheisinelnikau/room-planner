#requires -Version 7.0
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'Publication runs inside GitHub Actions only' }
if ($env:GITHUB_REPOSITORY -ne 'siarheisinelnikau/room-planner') { throw 'Unexpected canonical repository' }
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    $manifest = Get-Content -LiteralPath Cargo.toml -Raw
    $version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'A stable semantic version is required' }
    if ($version -ne '0.1.1') { throw 'Prepare reviewed release notes for this version before publishing' }
    $tag = "v$version"
    if ($env:GITHUB_REF_TYPE -eq 'tag' -and $env:GITHUB_REF_NAME -ne $tag) { throw 'Tag and installed version differ' }
    $commit = git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve the verified source commit' }
    $name = "RoomPlanner-v$version.exe"
    $binary = Join-Path 'dist' $name
    python (Join-Path $PSScriptRoot 'verify_package.py') $binary
    if ($LASTEXITCODE -ne 0) { throw 'Package verification failed' }
    $archive = "dist/RoomPlanner-v$version-source.zip"
    git archive --format=zip --output=$archive HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Source archive failed' }
    $notes = @"
RoomPlanner is a native Windows room-planning workspace with an editable 2D plan and a synchronized software-rendered 3D view.

### Included
- Dimensioned simple polygon rooms, wall thickness, doors, windows and openings.
- Original parametric desks, beds, sofas and cabinets with exact dimensions and rotation.
- Grid and edge snapping, undo/redo, overlap and door-swing checks.
- Versioned project files with atomic saves and recovery copies, plus dimensioned SVG export.
- English, Russian, French, German, Spanish and Italian interfaces, saved light/dark themes and reduced motion.
- Startup and manual update checks, SHA-256 verification and user-initiated portable installation.

### Startup fix
The Windows accessibility adapter is now initialized while the window is hidden, before the first show. This fixes the immediate startup failure in the initial local build. Startup failures also have a diagnostic log and an error message.

### Verification and platform
- Windows x64 portable executable with a statically linked C runtime; no separate runtime installation is required.
- Automated geometry, history, storage, update, localization, startup-layout and software-raster regression tests run before publication.
- A local bounded Windows diagnostic completed through plan, hidden and restored 3D states; settled and hidden intervals produced no additional application frames.
- The geometric checks are planning estimates, not architectural or building-code certification. Interactive workflows and update replacement have not received a full manual acceptance test.

The executable includes the MIT license and third-party notices. The adjacent SHA-256 file verifies the download.
"@
    $body = Join-Path $env:RUNNER_TEMP 'room-planner-release-notes.txt'
    [System.IO.File]::WriteAllText($body, $notes, [System.Text.UTF8Encoding]::new($false))
    gh release create $tag --repo $env:GITHUB_REPOSITORY --target $commit --title "RoomPlanner $version" --notes-file $body --draft
    if ($LASTEXITCODE -ne 0) { throw 'Could not create a new draft release; no existing release was modified' }
    gh release upload $tag $binary "$binary.sha256" $archive 'dist/LICENSE' 'dist/THIRD_PARTY_LICENSES.txt' --repo $env:GITHUB_REPOSITORY
    if ($LASTEXITCODE -ne 0) { throw 'Asset upload failed; the release remains a draft' }
    gh release edit $tag --repo $env:GITHUB_REPOSITORY --draft=false
    if ($LASTEXITCODE -ne 0) { throw 'Release publication failed; review the draft release' }
} finally { Pop-Location }
