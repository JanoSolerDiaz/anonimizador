# Genera el MSI de Windows (PRD, «Empaquetado y despliegue»).
#
# Lleva dentro el modelo, ONNX Runtime, PDFium y el instalador sin conexión
# de WebView2. Si se define ANONIMIZADOR_CERT_HUELLA con la huella SHA-1 de un
# certificado de firma de código instalado en el almacén del usuario, firma
# el ejecutable y el MSI; si no, salen sin firmar.
#
# Uso, desde la raíz del repositorio:
#   powershell -ExecutionPolicy Bypass -File scripts\empaquetar-windows.ps1
#
# Resultado: target\release\bundle\msi\Anonimizador_<versión>_x64_es-ES.msi
# Instalación silenciosa: msiexec /i <archivo.msi> /qn

$ErrorActionPreference = "Stop"
$raiz = Split-Path -Parent $PSScriptRoot
Set-Location $raiz

# Los recursos se comprueban con su SHA-256; si ya están, no se descargan.
node scripts/descargar-modelo.mjs
node scripts/descargar-binarios.mjs

Set-Location (Join-Path $raiz "app")
npm ci --no-audit --no-fund

$argumentos = @("run", "tauri", "build", "--")
if ($env:ANONIMIZADOR_CERT_HUELLA) {
    $firma = @{
        bundle = @{
            windows = @{
                certificateThumbprint = $env:ANONIMIZADOR_CERT_HUELLA
                timestampUrl = "http://timestamp.digicert.com"
            }
        }
    } | ConvertTo-Json -Depth 5 -Compress
    $argumentos += @("--config", $firma)
    Write-Host "Se firmará con el certificado $($env:ANONIMIZADOR_CERT_HUELLA)."
} else {
    Write-Warning "Sin ANONIMIZADOR_CERT_HUELLA: el MSI sale sin firmar."
}
& npm @argumentos
if ($LASTEXITCODE -ne 0) { throw "La compilación ha fallado." }

Get-ChildItem (Join-Path $raiz "target\release\bundle\msi\*.msi") | ForEach-Object {
    $huella = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
    Write-Host "$($_.Name)  $([math]::Round($_.Length / 1MB)) MB  sha256 $huella"
}
