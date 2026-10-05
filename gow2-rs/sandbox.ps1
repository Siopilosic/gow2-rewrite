# The plain sandbox: no level, a flat floor with a grid, three training dummies. Same controls as play.ps1 (see crates/gow2-bevy/src/bin/kratos_play.rs).
#   .\sandbox.ps1                 start
#   .\sandbox.ps1 -Aggro          the dummies fight back (V toggles it in the game)
param([switch]$Aggro)
Set-Location $PSScriptRoot
if (-not (Test-Path ".\target\release\kratos-play.exe")) { cargo build --release -p gow2-bevy --bin kratos-play }
if ($Aggro) { $env:GOW_AGGRO = "1" }
# a build made while the game was running goes to target_dev (cargo cannot replace a running exe): take whichever exe is newer
$exe = ".\target\release\kratos-play.exe"
$dev = ".\target_dev\release\kratos-play.exe"
if ((Test-Path $dev) -and ((Get-Item $dev).LastWriteTime -gt (Get-Item $exe).LastWriteTime)) { $exe = $dev }
& $exe "..\extracted\pak\R_HERO01.WAD"
