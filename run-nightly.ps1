$repo = "mach-6"
$branch = "main"
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
    Body = @{ repo = $repo; branch = $branch }
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