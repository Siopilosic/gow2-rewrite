# Ghidra headless driver. Never deletes previous outputs: every run exports to
# analysis\exports\<Pass>. Close the GoW2_Analysis project in the GUI before running.
#
#   .\tools\run_ghidra.ps1 -Pass pass3                # apply model to existing DB, then export
#   .\tools\run_ghidra.ps1 -Pass passN -Reimport      # fresh import + full auto-analysis first
#
# Snapshot ghidra\ (see snapshots\) before -Reimport: it replaces the program in the project.
param(
    [Parameter(Mandatory = $true)][string]$Pass,
    [switch]$Reimport,
    [string]$Ghidra = "D:\Downloads\ghidra_12.1.4_PUBLIC_20260921\ghidra_12.1.4_PUBLIC"
)
$root = Split-Path $PSScriptRoot -Parent
$out = "$root\analysis\exports\$Pass"
if (Test-Path $out) { throw "export folder $out already exists; choose a new -Pass label" }
New-Item -ItemType Directory -Force $out | Out-Null
$ext = "$env:APPDATA\ghidra\ghidra_12.1.4_PUBLIC\Extensions\ghidra-emotionengine-reloaded\ghidra_scripts"
$common = @("-scriptPath", "$root\tools\ghidra_scripts;$ext",
    "-postScript", "ApplyCore.java", "$root\analysis",
    "-postScript", "ExportGoW2.java", $out,
    "-log", "$out\headless.log", "-max-cpu", "8")
if ($Reimport) {
    & "$Ghidra\support\analyzeHeadless.bat" "$root\ghidra" GoW2_Analysis `
        -import "$root\extracted\SCUS_974.81" -overwrite -processor "r5900:LE:32:default" `
        -preScript EnableParamId.java -postScript ImportMMIORegisterLabels.java @common
}
else {
    & "$Ghidra\support\analyzeHeadless.bat" "$root\ghidra" GoW2_Analysis `
        -process SCUS_974.81 -noanalysis @common
}
Copy-Item "$root\analysis\apply_core.log" $out -ErrorAction SilentlyContinue
