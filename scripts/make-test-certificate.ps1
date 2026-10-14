<#
.SYNOPSIS
  Creates a self-signed code-signing certificate for MarkDraft test builds.

.DESCRIPTION
  Self-signed certificates sign the installer and app so they are not
  tampered with, but Windows does not trust them: SmartScreen and the UAC
  prompt still say "Unknown publisher" on machines that have not been told to
  trust the certificate. Use this for your own and your team's machines while
  MarkDraft is not sold yet; switch to Azure Artifact Signing or an OV
  certificate (docs/SIGNING.md) before public releases.

  The script:
    1. creates the certificate in your user certificate store,
    2. exports it as a password-protected .pfx (private key, keep secret),
    3. writes the .pfx as Base64 for the WINDOWS_TEST_CERTIFICATE GitHub secret,
    4. exports the public .cer that other machines import to trust it.

.EXAMPLE
  pwsh scripts/make-test-certificate.ps1 -OutDir $HOME\markdraft-cert
#>
param(
  [string]$Subject = "CN=MarkDraft (test signing)",
  [string]$OutDir = (Join-Path $HOME "markdraft-cert"),
  [int]$Years = 3
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$cert = New-SelfSignedCertificate `
  -Type CodeSigningCert `
  -Subject $Subject `
  -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 `
  -KeyExportPolicy Exportable `
  -CertStoreLocation Cert:\CurrentUser\My `
  -NotAfter (Get-Date).AddYears($Years)

$password = Read-Host -AsSecureString "Choose a password for the .pfx file"
$pfx = Join-Path $OutDir "markdraft-test.pfx"
$cer = Join-Path $OutDir "markdraft-test.cer"
Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $password | Out-Null
Export-Certificate -Cert $cert -FilePath $cer | Out-Null
[Convert]::ToBase64String([IO.File]::ReadAllBytes($pfx)) | Set-Content -NoNewline (Join-Path $OutDir "markdraft-test.pfx.base64.txt")

Write-Host ""
Write-Host "Created certificate $($cert.Thumbprint) ($Subject), valid until $($cert.NotAfter.ToShortDateString())."
Write-Host ""
Write-Host "GitHub (Settings > Secrets and variables > Actions):"
Write-Host "  WINDOWS_TEST_CERTIFICATE          = contents of markdraft-test.pfx.base64.txt"
Write-Host "  WINDOWS_TEST_CERTIFICATE_PASSWORD = the password you just chose"
Write-Host ""
Write-Host "To trust it on a test machine (run there, as that user):"
Write-Host "  Import-Certificate -FilePath markdraft-test.cer -CertStoreLocation Cert:\CurrentUser\Root"
Write-Host "  Import-Certificate -FilePath markdraft-test.cer -CertStoreLocation Cert:\CurrentUser\TrustedPublisher"
Write-Host ""
Write-Host "Keep markdraft-test.pfx and its password private. Files are in $OutDir"
