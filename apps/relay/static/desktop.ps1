& {
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'
  if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'the sombre app needs 64-bit windows'
  }
  if (Get-Process -Name 'sombre-desktop' -ErrorAction SilentlyContinue) {
    throw 'quit the sombre app first, then run this again'
  }
  $installer = 'sombre-setup.exe'
  $base = 'https://github.com/visible/sombre/releases/latest/download'
  $temp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
  New-Item -ItemType Directory -Path $temp | Out-Null
  try {
    Write-Host 'downloading the sombre app'
    Invoke-WebRequest -UseBasicParsing -Uri "$base/$installer" -OutFile (Join-Path $temp $installer)
    Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile (Join-Path $temp 'SHA256SUMS')
    $line = Get-Content (Join-Path $temp 'SHA256SUMS') | Where-Object { $_ -match "\s$([regex]::Escape($installer))$" } | Select-Object -First 1
    if (-not $line) {
      throw "the checksums don't list $installer"
    }
    $expected = ($line -split '\s+')[0]
    $actual = (Get-FileHash -Algorithm SHA256 -Path (Join-Path $temp $installer)).Hash
    if ($expected -ne $actual) {
      throw "the download doesn't match its checksum, so nothing was installed"
    }
    $setup = Start-Process -FilePath (Join-Path $temp $installer) -ArgumentList '/S' -Wait -PassThru
    if ($setup.ExitCode -ne 0) {
      throw "the installer stopped with code $($setup.ExitCode)"
    }
    $app = Join-Path $env:LOCALAPPDATA 'sombre\sombre-desktop.exe'
    Write-Host "installed the sombre app to $app. it updates itself from now on"
    Start-Process -FilePath $app
  } finally {
    Remove-Item -Recurse -Force -Path $temp -ErrorAction SilentlyContinue
  }
}
