# Starts the playtest on RHOD10 (the Rhodes opening) with the default blades.
#   .\play.ps1                 normal start
#   .\play.ps1 -Aggro          the dummies fight back (V toggles it in the game)
#   .\play.ps1 -Region 0       start in the n-th walkable part of the level (P cycles them in the game)
# Controls are listed in crates/gow2-bevy/src/bin/kratos_play.rs (left stick move, Cross jump, Square/Triangle/Circle attack, R1 block,
# right stick evade, mouse drag / Z / X / triggers turn the camera, F2 collision, F4 hide HUD, F5 mute, N noclip, P next region).
param([switch]$Aggro, [int]$Region = -1, [string]$Level = "RHOD10")
Set-Location $PSScriptRoot
if (-not (Test-Path ".\target\release\kratos-play.exe")) { cargo build --release -p gow2-bevy --bin kratos-play }
if ($Aggro) { $env:GOW_AGGRO = "1" }
if ($Region -ge 0) { $env:GOW_REGION = "$Region" }
# a build made while the game was running goes to target_dev (cargo cannot replace a running exe): take whichever exe is newer
$exe = ".\target\release\kratos-play.exe"
$dev = ".\target_dev\release\kratos-play.exe"
if ((Test-Path $dev) -and ((Get-Item $dev).LastWriteTime -gt (Get-Item $exe).LastWriteTime)) { $exe = $dev }
& $exe "..\extracted\pak\R_HERO01.WAD" "..\extracted\pak\$Level.WAD"
