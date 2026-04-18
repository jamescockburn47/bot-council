$ErrorActionPreference = "Stop"

Write-Host "Fetching remotes..."
git fetch origin --prune | Out-Null

$currentBranch = git rev-parse --abbrev-ref HEAD
Write-Host "Current branch: $currentBranch"

if ($currentBranch -eq "main" -or $currentBranch -eq "master") {
    throw "Do not implement directly on $currentBranch. Create a feature branch from origin/main."
}

$mergeBase = git merge-base HEAD origin/main
if ([string]::IsNullOrWhiteSpace($mergeBase)) {
    throw "Could not find merge-base with origin/main."
}

Write-Host ""
Write-Host "Open pull requests:"
gh pr list --state open --json number,title,headRefName,baseRefName,url

Write-Host ""
Write-Host "Local branches with gone upstream:"
$goneBranches = git for-each-ref --format='%(refname:short) %(upstream:track)' refs/heads | Select-String '\[gone\]'
if ($goneBranches) {
    $goneBranches | ForEach-Object { Write-Host $_.Line }
} else {
    Write-Host "(none)"
}

Write-Host ""
Write-Host "Branch preflight complete."
