# release-gate.ps1 — release-process gates (TESTING_ARCHITECTURE_PLAN §3.5,
# incident #8 of the incident map). Two halves, stated honestly:
#
#   ① Defect half (machine-enforced): refuse to proceed while any P1 issue is
#      open. Run before tagging / before publishing. Wired into release.yml
#      before the draft is attached to a release.
#   ② Publication half (verification skeleton, machine-assisted): after the
#      draft->published PATCH (which RELEASE_CHECKLIST §7-2 restricts to an
#      authenticated API call), re-read the release state and assert it really
#      is published — do not trust the HTTP 200 (the platform silently ignores
#      draft=true on already-published releases, §7-2 note). This part is a
#      SKELETON: not executed by CI in this phase; the maintainer runs it
#      manually after the authenticated PATCH, per §7-3 (write-then-read).
#
# What cannot be machine-enforced (stated per plan §3.5): the ban on
# programmatic form submission (§7-1) and the separate GO authorization
# (§7-4) stay manual checklist signatures.
param(
    # ① default: check for open P1 issues
    [switch]$CheckP1,

    # ② write-then-read verification after PATCH draft=false
    [switch]$VerifyPublished,
    [string]$ReleaseTag,   # e.g. v0.1.8 — required with -VerifyPublished
    [string]$Repo          # owner/repo; defaults to gh's current repo
)
$ErrorActionPreference = 'Stop'

# DEPLOYMENT.md convention: curl to api.github.com must carry --ssl-no-revoke;
# authenticated calls take the token from `gh auth token`. gh api honours both.
function Invoke-Gh {
    param([string[]]$GhArgs)
    $token = (& gh auth token).Trim()
    gh api --ssl-no-revoke -H "Authorization: Bearer $token" @GhArgs
}

if ($CheckP1) {
    $args_ = @('issues', 'list', '--state', 'open', '--label', 'P1', '--json', 'number,title')
    if ($Repo) { $args_ += @('-R', $Repo) }
    $open = Invoke-Gh $args_ | ConvertFrom-Json
    if ($open.Count -gt 0) {
        Write-Host 'FAIL: open P1 issues exist — release forbidden (RELEASE_CHECKLIST §7 / incident #8, v0.1.4 shipped with an unclosed P1):'
        $open | ForEach-Object { Write-Host ("  #{0}  {1}" -f $_.number, $_.title) }
        exit 1
    }
    Write-Host 'PASS: no open P1 issues'
    exit 0
}

if ($VerifyPublished) {
    if (-not $ReleaseTag) { Write-Error '-VerifyPublished requires -ReleaseTag'; exit 2 }
    $args_ = @('repos/{0}/releases/tags/{1}' -f ($(if ($Repo) { $Repo } else { (& gh repo view --json nameWithOwner -q .nameWithOwner) })), $ReleaseTag)
    $rel = Invoke-Gh $args_ | ConvertFrom-Json
    Write-Host ("release {0}: draft={1}, published_at={2}" -f $rel.tag_name, $rel.draft, $rel.published_at)
    if ($rel.draft) {
        Write-Host 'FAIL: release is still a draft after the publish PATCH — the status change did NOT take effect (do not trust the HTTP 200; re-do the authenticated PATCH and re-verify, RELEASE_CHECKLIST §7-3)'
        exit 1
    }
    Write-Host 'PASS: write-then-read verified — release is published'
    exit 0
}

Write-Host 'usage: release-gate.ps1 -CheckP1 | -VerifyPublished -ReleaseTag vX.Y.Z'
exit 2
