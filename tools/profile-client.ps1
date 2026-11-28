# Measures the client on this computer: every scene with the Low and the High preset,
# $Secs seconds each (after a 5 s warm-up), one Markdown report per run in $Out.
#   powershell -ExecutionPolicy Bypass -File tools\profile-client.ps1 [-Out profile] [-Secs 30] [-Bin toon-warfare.exe]
# The game must be closed. Your saved settings are not changed (--video is for one run).
param([string]$Out = "profile", [int]$Secs = 30, [string]$Bin = "target\release\toon-warfare.exe")
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$common = @("--no-sound", "--no-update", "--no-vsync", "--profile", "$Secs")
$game = @("--host", "--seed", "7", "--auto")
function Scene($name, $more) {
    foreach ($v in @("low", "high")) {
        Write-Host "== $name ($v)"
        & $Bin @common --video $v --profile-out "$Out\$name-$v.md" @more | Out-Null
    }
}
Scene "menu" @()
Scene "tdm" ($game + @("--map", "depot", "--mode", "tdm", "--bots", "9", "--gun", "uar15"))
Scene "br-town" ($game + @("--map", "island", "--mode", "br", "--bots", "31", "--team", "spec", "--spec-at", "-180,9.6,-80", "--look-at", "-60,9,-80"))
Scene "br-field" ($game + @("--map", "island", "--mode", "br", "--bots", "31", "--team", "spec", "--spec-at", "60,11.2,-700", "--look-at", "-120,9,-80"))
Scene "br-drop" ($game + @("--map", "island", "--mode", "br", "--bots", "31", "--br-jump"))
Scene "scope8x" ($game + @("--map", "site", "--mode", "tdm", "--bots", "9", "--gun", "awm", "--ads"))
Write-Host "Reports: $Out\"
