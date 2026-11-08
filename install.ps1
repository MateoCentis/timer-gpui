# Compila en release, copia la app a %LOCALAPPDATA%\Programs\Temporizador, crea el
# acceso directo del menú Inicio y la abre. Vuelve a ejecutarlo después de cada cambio.
# "Iniciar con Windows" se activa desde la app (Ajustes).
$ErrorActionPreference = 'Stop'

$dest = Join-Path $env:LOCALAPPDATA 'Programs\Temporizador'
$exe = Join-Path $dest 'temporizador.exe'

cargo build --release --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'cargo build falló' }

Get-Process temporizador -ErrorAction SilentlyContinue | Stop-Process -Force
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item (Join-Path $PSScriptRoot 'target\release\temporizador.exe') $exe -Force

$shell = New-Object -ComObject WScript.Shell
$lnk = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Programs')) 'Temporizador.lnk'))
$lnk.TargetPath = $exe
$lnk.WorkingDirectory = $dest
$lnk.Save()

Start-Process $exe
Write-Host "Instalado en $dest. Está en el menú Inicio como 'Temporizador'."
