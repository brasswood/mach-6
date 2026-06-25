<#
.SYNOPSIS
Triggers a nightly benchmark run for this repository.

.DESCRIPTION
Posts a request to the nightly benchmark service for the `mach-6` repository.
If `-Branch` is not provided, the script uses the current Git branch's upstream
tracking branch name.

The script expects a `nightly-credentials.ps1` file in the same directory that
defines `$User` and `$Password`.

.PARAMETER Branch
The branch to run on the nightly service. If omitted, the script determines the
current Git branch and uses the name of its configured upstream branch.

.EXAMPLE
./run-nightly.ps1

Triggers a nightly run for the current branch's upstream branch.

.EXAMPLE
./run-nightly.ps1 -Branch main

Triggers a nightly run for the `main` branch.
#>
param(
    [string]$Branch
)

$ErrorActionPreference = "Stop"

$repo = "mach-6"

if (-not $Branch) {
    $currentBranch = & git -C $PSScriptRoot branch --show-current
    if ($LASTEXITCODE -ne 0) {
        throw "No branch specified and unable to determine the current branch. Pass -Branch <branch>."
    }

    if (-not $currentBranch) {
        throw "No branch specified and Git is in a detached HEAD state. Check out a branch or pass -Branch <branch>."
    }

    $upstream = & git -C $PSScriptRoot rev-parse --abbrev-ref --symbolic-full-name "@{u}" 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $upstream) {
        throw "No branch specified and the current branch does not have a remote tracking branch. Pass -Branch <branch>."
    }

    $Branch = $upstream -replace "^[^/]+/", ""
}

$credsPath = Join-Path $PSScriptRoot "nightly-credentials.ps1"
if (-not (Test-Path $credsPath)) {
    throw "Missing credentials file: $credsPath"
}
. $credsPath
$basic = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${User}:${Password}"))
$headers = @{ Authorization = "Basic $basic" }

$params = @{
    Method = 'Post'
    Uri = 'https://nightly.cs.washington.edu//runnow'
    Headers = $headers
    Body = @{ repo = $repo; branch = $Branch }
    MaximumRedirection = 0
}
try {
    # Commands invoked on their own print to the screen
    Invoke-WebRequest @params
}
catch {
    if ($_.Exception.Response.StatusCode.value__ -ne 302) {
        $_.Exception.Response
        throw
    }
}
