$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = Split-Path -Parent $PSScriptRoot
$checked = [System.Collections.Generic.HashSet[string]]::new()

function Assert-Icon([string]$relativePath, [int]$size) {
    $path = Join-Path $root $relativePath
    $bytes = [System.IO.File]::ReadAllBytes($path)
    if ($bytes[25] -ne 2) { throw "$relativePath must be an opaque RGB PNG." }
    $image = [System.Drawing.Image]::FromFile($path)
    try {
        if ($image.Width -ne $size -or $image.Height -ne $size) {
            throw "$relativePath must be $size x $size."
        }
    } finally { $image.Dispose() }
    $checked.Add($relativePath) | Out-Null
}

Assert-Icon 'assets\branding\kinetism-icon.png' 1024
Assert-Icon 'web\favicon.png' 32
foreach ($size in @(192, 512)) {
    Assert-Icon "web\icons\Icon-$size.png" $size
    Assert-Icon "web\icons\Icon-maskable-$size.png" $size
}
foreach ($entry in @(
    @{ density = 'mdpi'; size = 48 }, @{ density = 'hdpi'; size = 72 },
    @{ density = 'xhdpi'; size = 96 }, @{ density = 'xxhdpi'; size = 144 },
    @{ density = 'xxxhdpi'; size = 192 }
)) {
    Assert-Icon "android\app\src\main\res\mipmap-$($entry.density)\ic_launcher.png" $entry.size
}
foreach ($catalogPath in @(
    'ios\Runner\Assets.xcassets\AppIcon.appiconset',
    'macos\Runner\Assets.xcassets\AppIcon.appiconset'
)) {
    $catalog = Get-Content -LiteralPath (Join-Path $root "$catalogPath\Contents.json") -Raw | ConvertFrom-Json
    foreach ($entry in $catalog.images) {
        $points = [double]::Parse(($entry.size -split 'x')[0], [cultureinfo]::InvariantCulture)
        $scale = [double]::Parse(($entry.scale -replace 'x$', ''), [cultureinfo]::InvariantCulture)
        Assert-Icon "$catalogPath\$($entry.filename)" ([int]($points * $scale))
    }
}
foreach ($entry in @(
    @{ name = 'LaunchImage.png'; size = 160 },
    @{ name = 'LaunchImage@2x.png'; size = 320 },
    @{ name = 'LaunchImage@3x.png'; size = 480 }
)) {
    Assert-Icon "ios\Runner\Assets.xcassets\LaunchImage.imageset\$($entry.name)" $entry.size
}

$icon = [System.IO.File]::ReadAllBytes((Join-Path $root 'windows\runner\resources\app_icon.ico'))
$sizes = @(16, 24, 32, 48, 64, 128, 256)
if ([BitConverter]::ToUInt16($icon, 2) -ne 1 -or
    [BitConverter]::ToUInt16($icon, 4) -ne $sizes.Length) { throw 'ICO directory is invalid.' }
for ($index = 0; $index -lt $sizes.Length; $index++) {
    $entry = 6 + 16 * $index
    $dimension = if ($sizes[$index] -eq 256) { 0 } else { $sizes[$index] }
    $length = [BitConverter]::ToUInt32($icon, $entry + 8)
    $offset = [BitConverter]::ToUInt32($icon, $entry + 12)
    if ($icon[$entry] -ne $dimension -or $icon[$entry + 1] -ne $dimension -or
        $offset + $length -gt $icon.Length -or $icon[$offset] -ne 137) {
        throw "ICO image $index is invalid."
    }
}

[xml]$svg = Get-Content -LiteralPath (Join-Path $root 'assets\branding\kinetism-mark.svg') -Raw
[xml]$adaptive = Get-Content -LiteralPath (Join-Path $root 'android\app\src\main\res\drawable\ic_launcher_foreground.xml') -Raw
$scale = [double]::Parse($adaptive.vector.group.GetAttribute(
    'scaleX', 'http://schemas.android.com/apk/res/android'), [cultureinfo]::InvariantCulture)
$safeRadius = 1024 * 33 / 108
foreach ($line in $svg.svg.line) {
    foreach ($point in @(@([double]$line.x1, [double]$line.y1), @([double]$line.x2, [double]$line.y2))) {
        $radius = [math]::Sqrt([math]::Pow($point[0] - 512, 2) +
            [math]::Pow($point[1] - 512, 2)) + [double]$line.'stroke-width' / 2
        if ($radius * $scale -gt $safeRadius) { throw 'Adaptive line exceeds the safe zone.' }
    }
}
foreach ($circle in $svg.svg.circle) {
    $radius = [math]::Sqrt([math]::Pow([double]$circle.cx - 512, 2) +
        [math]::Pow([double]$circle.cy - 512, 2)) + [double]$circle.r
    if ($radius * $scale -gt $safeRadius) { throw 'Adaptive sensor node exceeds the safe zone.' }
}
foreach ($xmlFile in Get-ChildItem -LiteralPath (Join-Path $root 'android\app\src\main\res') -Filter '*.xml' -Recurse) {
    [xml](Get-Content -LiteralPath $xmlFile.FullName -Raw) | Out-Null
}
[xml]$manifest = Get-Content -LiteralPath (Join-Path $root 'android\app\src\main\AndroidManifest.xml') -Raw
$permission = $manifest.manifest.'uses-permission' | Where-Object {
    $_.GetAttribute('name', 'http://schemas.android.com/apk/res/android') -eq 'android.permission.INTERNET'
}
if (!$permission) { throw 'Android internet permission is missing.' }
Write-Output "PASS: $($checked.Count) opaque PNG sizes, 7 ICO images, adaptive safe zone and Android resource XML."
