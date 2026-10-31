# Pixel-exact comparison of two sets of same-state screenshots.
#   powershell -NoProfile -File _tools/_diff-pixel.ps1 _tmp/color/before _tmp/color/after
param(
  [Parameter(Mandatory = $true)][string]$A,
  [Parameter(Mandatory = $true)][string]$B,
  # SkipLeft: the fixed 160px rail column has a 2px rendering artifact that also
  # shows up between two captures of the SAME build, so it can be excluded.
  [int]$SkipLeft = 0
)
Add-Type -AssemblyName System.Drawing

function Get-Pixels([string]$path) {
  $bmp = [System.Drawing.Bitmap]::FromFile((Resolve-Path $path))
  $rect = New-Object System.Drawing.Rectangle 0, 0, $bmp.Width, $bmp.Height
  $data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
  $bytes = New-Object byte[] ($data.Stride * $bmp.Height)
  [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
  $bmp.UnlockBits($data)
  $info = [pscustomobject]@{ Width = $bmp.Width; Height = $bmp.Height; Stride = $data.Stride; Bytes = $bytes }
  $bmp.Dispose()
  return $info
}

$files = Get-ChildItem $A -Filter *.png | Sort-Object Name
$totalDiff = 0
$bad = 0
foreach ($f in $files) {
  $other = Join-Path $B $f.Name
  if (-not (Test-Path $other)) {
    "  {0,-26} MISSING in second set" -f $f.Name
    $bad++
    continue
  }
  $pa = Get-Pixels $f.FullName
  $pb = Get-Pixels $other
  if ($pa.Width -ne $pb.Width -or $pa.Height -ne $pb.Height) {
    "  {0,-26} SIZE {1}x{2} vs {3}x{4}" -f $f.Name, $pa.Width, $pa.Height, $pb.Width, $pb.Height
    $bad++
    continue
  }
  $diff = 0
  $first = $null
  for ($y = 0; $y -lt $pa.Height; $y++) {
    $rowA = $y * $pa.Stride
    $rowB = $y * $pb.Stride
    for ($x = ($SkipLeft * 3); $x -lt $pa.Stride; $x += 3) {
      $i = $rowA + $x
      $j = $rowB + $x
      if ($pa.Bytes[$i] -ne $pb.Bytes[$j] -or $pa.Bytes[$i + 1] -ne $pb.Bytes[$j + 1] -or $pa.Bytes[$i + 2] -ne $pb.Bytes[$j + 2]) {
        $diff++
        if ($null -eq $first) { $first = "(" + [int]($x / 3) + "," + $y + ")" }
      }
    }
  }
  $totalDiff += $diff
  if ($diff -eq 0) {
    "  {0,-26} 0 pixels differ" -f $f.Name
  }
  else {
    "  {0,-26} {1} pixels differ (first at {2})" -f $f.Name, $diff, $first
    $bad++
  }
}

""
"TOTAL differing pixels: $totalDiff ; screenshots with differences: $bad / $($files.Count)"
if ($totalDiff -gt 0 -or $bad -gt 0) { exit 1 }
