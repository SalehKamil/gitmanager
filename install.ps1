# Installe gitmanager sous Windows : la derniere version publiee sur GitHub,
# verifiee par son empreinte SHA-256, dans %LOCALAPPDATA%\Programs\gitmanager.
# La meme commande met a jour une version deja installee.
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$depot = 'SalehKamil/gitmanager'
$fichier = 'gitmanager-windows-x86_64.exe'
$base = "https://github.com/$depot/releases/latest/download"
$dossier = Join-Path $env:LOCALAPPDATA 'Programs\gitmanager'
$cible = Join-Path $dossier 'gitmanager.exe'
$nouveau = "$cible.nouveau"

New-Item -ItemType Directory -Force -Path $dossier | Out-Null
Write-Host "  Telechargement de $fichier"
Invoke-WebRequest -UseBasicParsing -Uri "$base/$fichier" -OutFile $nouveau
$sommes = (Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS").Content
if ($sommes -is [byte[]]) { $sommes = [Text.Encoding]::UTF8.GetString($sommes) }

$attendue = ($sommes -split "`n" | Where-Object { $_ -match " $([regex]::Escape($fichier))\s*$" } |
    ForEach-Object { ($_ -split ' ')[0] }) | Select-Object -First 1
$obtenue = (Get-FileHash -Algorithm SHA256 $nouveau).Hash.ToLower()
if (-not $attendue -or $attendue.Trim().ToLower() -ne $obtenue) {
    Remove-Item -Force $nouveau
    throw "Empreinte SHA-256 differente de celle publiee : installation annulee."
}
Write-Host "  Empreinte SHA-256 OK : $obtenue"

try {
    Move-Item -Force $nouveau $cible
} catch {
    Remove-Item -Force $nouveau -ErrorAction SilentlyContinue
    throw "gitmanager est ouvert : le fermer, puis relancer l'installation."
}
Write-Host "  Installation OK : $cible"

$path = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not (($path -split ';') -contains $dossier)) {
    [Environment]::SetEnvironmentVariable('Path', "$dossier;$path", 'User')
    $env:Path = "$dossier;$env:Path"
    Write-Host "  Dossier ajoute au PATH : ouvrir un nouveau terminal, puis taper gitmanager."
}
& $cible --version
