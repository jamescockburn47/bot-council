param(
    [switch]$Apply
)

$ErrorActionPreference = "Stop"

git fetch origin --prune | Out-Null

$worktreeBranches = @{}
git worktree list --porcelain | ForEach-Object {
    if ($_ -like "branch refs/heads/*") {
        $name = $_.Substring("branch refs/heads/".Length)
        $worktreeBranches[$name] = $true
    }
}

$gone = @()
git for-each-ref --format='%(refname:short)|%(upstream:track)' refs/heads | ForEach-Object {
    $parts = $_ -split '\|', 2
    if ($parts.Count -eq 2 -and $parts[1] -match '\[gone\]') {
        $gone += $parts[0]
    }
}

if (-not $gone.Count) {
    Write-Host "No gone-upstream local branches found."
    exit 0
}

Write-Host "Gone-upstream branches:"
$gone | ForEach-Object { Write-Host " - $_" }

if (-not $Apply) {
    Write-Host ""
    Write-Host "Dry run complete. Re-run with -Apply to delete branches not attached to worktrees."
    exit 0
}

foreach ($branch in $gone) {
    if ($worktreeBranches.ContainsKey($branch)) {
        Write-Host "Skip (checked out in worktree): $branch"
        continue
    }
    git branch -D $branch | Out-Null
    Write-Host "Deleted: $branch"
}
