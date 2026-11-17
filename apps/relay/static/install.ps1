& {
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'
  if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'sombre needs 64-bit windows'
  }
  $target = 'x86_64-pc-windows-msvc'
  $archive = "sombre-$target.zip"
  $base = 'https://github.com/visible/sombre/releases/latest/download'
  $folder = if ($env:SOMBRE_INSTALL) { $env:SOMBRE_INSTALL } else { Join-Path $env:LOCALAPPDATA 'sombre\bin' }
  $temp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
  New-Item -ItemType Directory -Path $temp | Out-Null
  try {
    Write-Host "downloading sombre for $target"
    Invoke-WebRequest -UseBasicParsing -Uri "$base/$archive" -OutFile (Join-Path $temp $archive)
    Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile (Join-Path $temp 'SHA256SUMS')
    $line = Get-Content (Join-Path $temp 'SHA256SUMS') | Where-Object { $_ -match "\s$([regex]::Escape($archive))$" } | Select-Object -First 1
    if (-not $line) {
      throw "the checksums don't list $archive"
    }
    $expected = ($line -split '\s+')[0]
    $actual = (Get-FileHash -Algorithm SHA256 -Path (Join-Path $temp $archive)).Hash
    if ($expected -ne $actual) {
      throw "the download doesn't match its checksum, so nothing was installed"
    }
    Expand-Archive -Path (Join-Path $temp $archive) -DestinationPath $temp -Force
    New-Item -ItemType Directory -Force -Path $folder | Out-Null
    Move-Item -Force -Path (Join-Path $temp 'sombre.exe') -Destination (Join-Path $folder 'sombre.exe')
    $path = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ((($path -split ';') | Where-Object { $_ }) -notcontains $folder) {
      [Environment]::SetEnvironmentVariable('Path', ((@($path, $folder) | Where-Object { $_ }) -join ';'), 'User')
      $env:Path = "$env:Path;$folder"
      Write-Host "added $folder to your PATH. open a new terminal to use sombre everywhere"
    }
    Write-Host "installed $(& (Join-Path $folder 'sombre.exe') --version) to $folder"
    Write-Host 'run sombre in a project folder to start'
  } finally {
    Remove-Item -Recurse -Force -Path $temp -ErrorAction SilentlyContinue
  }
}
