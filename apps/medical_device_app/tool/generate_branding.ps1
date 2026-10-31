$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = Split-Path -Parent $PSScriptRoot
[xml]$svg = Get-Content -LiteralPath (Join-Path $root 'assets\branding\kinetism-mark.svg') -Raw
$namespace = [System.Xml.XmlNamespaceManager]::new($svg.NameTable)
$namespace.AddNamespace('s', 'http://www.w3.org/2000/svg')

function New-BrandBitmap([int]$size) {
    $canvasSize = $size * 4
    $canvas = [System.Drawing.Bitmap]::new($canvasSize, $canvasSize)
    $graphics = [System.Drawing.Graphics]::FromImage($canvas)
    try {
        $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
        $graphics.Clear([System.Drawing.ColorTranslator]::FromHtml($svg.svg.rect.fill))
        $graphics.ScaleTransform($canvasSize / 1024.0, $canvasSize / 1024.0)
        foreach ($line in $svg.SelectNodes('//s:line', $namespace)) {
            $pen = [System.Drawing.Pen]::new(
                [System.Drawing.ColorTranslator]::FromHtml($line.stroke), [float]$line.'stroke-width')
            try {
                $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
                $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
                $graphics.DrawLine($pen, [float]$line.x1, [float]$line.y1, [float]$line.x2, [float]$line.y2)
            } finally { $pen.Dispose() }
        }
        foreach ($circle in $svg.SelectNodes('//s:circle', $namespace)) {
            $brush = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml($circle.fill))
            try {
                $radius = [float]$circle.r
                $graphics.FillEllipse($brush, [float]$circle.cx - $radius,
                    [float]$circle.cy - $radius, 2 * $radius, 2 * $radius)
            } finally { $brush.Dispose() }
        }
        $result = [System.Drawing.Bitmap]::new(
            $size, $size, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
        $resize = [System.Drawing.Graphics]::FromImage($result)
        try {
            $resize.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $resize.Clear([System.Drawing.ColorTranslator]::FromHtml($svg.svg.rect.fill))
            $resize.DrawImage($canvas, 0, 0, $size, $size)
        } finally { $resize.Dispose() }
        return $result
    } finally {
        $graphics.Dispose()
        $canvas.Dispose()
    }
}

function Write-BrandPng([string]$relativePath, [int]$size) {
    $path = Join-Path $root $relativePath
    [System.IO.Directory]::CreateDirectory((Split-Path -Parent $path)) | Out-Null
    $bitmap = New-BrandBitmap $size
    try {
        $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally { $bitmap.Dispose() }
    Write-Output "$relativePath ($size x $size)"
}

Write-BrandPng 'assets\branding\kinetism-icon.png' 1024
Write-BrandPng 'web\favicon.png' 32
foreach ($size in @(192, 512)) {
    Write-BrandPng "web\icons\Icon-$size.png" $size
    Write-BrandPng "web\icons\Icon-maskable-$size.png" $size
}
foreach ($entry in @(
    @{ density = 'mdpi'; size = 48 },
    @{ density = 'hdpi'; size = 72 },
    @{ density = 'xhdpi'; size = 96 },
    @{ density = 'xxhdpi'; size = 144 },
    @{ density = 'xxxhdpi'; size = 192 }
)) {
    Write-BrandPng "android\app\src\main\res\mipmap-$($entry.density)\ic_launcher.png" $entry.size
}
foreach ($catalogPath in @(
    'ios\Runner\Assets.xcassets\AppIcon.appiconset',
    'macos\Runner\Assets.xcassets\AppIcon.appiconset'
)) {
    $catalog = Get-Content -LiteralPath (Join-Path $root "$catalogPath\Contents.json") -Raw | ConvertFrom-Json
    foreach ($entry in $catalog.images) {
        if ([System.IO.Path]::GetFileName($entry.filename) -ne $entry.filename) {
            throw 'Icon catalog filename must not contain a directory.'
        }
        $points = [double]::Parse(($entry.size -split 'x')[0], [cultureinfo]::InvariantCulture)
        $scale = [double]::Parse(($entry.scale -replace 'x$', ''), [cultureinfo]::InvariantCulture)
        Write-BrandPng "$catalogPath\$($entry.filename)" ([int]($points * $scale))
    }
}
foreach ($entry in @(
    @{ name = 'LaunchImage.png'; size = 160 },
    @{ name = 'LaunchImage@2x.png'; size = 320 },
    @{ name = 'LaunchImage@3x.png'; size = 480 }
)) {
    Write-BrandPng "ios\Runner\Assets.xcassets\LaunchImage.imageset\$($entry.name)" $entry.size
}

$sizes = @(16, 24, 32, 48, 64, 128, 256)
$images = foreach ($size in $sizes) {
    $bitmap = New-BrandBitmap $size
    $memory = [System.IO.MemoryStream]::new()
    try {
        $bitmap.Save($memory, [System.Drawing.Imaging.ImageFormat]::Png)
        ,$memory.ToArray()
    } finally {
        $memory.Dispose()
        $bitmap.Dispose()
    }
}
$iconPath = Join-Path $root 'windows\runner\resources\app_icon.ico'
$stream = [System.IO.File]::Create($iconPath)
$writer = [System.IO.BinaryWriter]::new($stream)
try {
    $writer.Write([uint16]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]$sizes.Length)
    $offset = 6 + 16 * $sizes.Length
    for ($index = 0; $index -lt $sizes.Length; $index++) {
        $dimension = if ($sizes[$index] -eq 256) { 0 } else { $sizes[$index] }
        $writer.Write([byte]$dimension)
        $writer.Write([byte]$dimension)
        $writer.Write([byte]0)
        $writer.Write([byte]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]24)
        $writer.Write([uint32]$images[$index].Length)
        $writer.Write([uint32]$offset)
        $offset += $images[$index].Length
    }
    foreach ($image in $images) { $writer.Write([byte[]]$image) }
} finally { $writer.Dispose() }
Write-Output 'windows\runner\resources\app_icon.ico (7 sizes)'

$paths = foreach ($node in $svg.SelectNodes('//s:line | //s:circle', $namespace)) {
    if ($node.LocalName -eq 'line') {
        $color = '#FF' + $node.stroke.TrimStart('#')
        '<path android:pathData="M{0},{1} L{2},{3}" android:strokeColor="{4}" android:strokeWidth="{5}" android:strokeLineCap="round" android:fillColor="#00000000"/>' -f `
            $node.x1, $node.y1, $node.x2, $node.y2, $color, $node.'stroke-width'
    } else {
        $color = '#FF' + $node.fill.TrimStart('#')
        $left = [int]$node.cx - [int]$node.r
        $right = [int]$node.cx + [int]$node.r
        '<path android:pathData="M{0},{1} A{2},{2} 0,1 0,{3} {1} A{2},{2} 0,1 0,{0} {1}" android:fillColor="{4}"/>' -f `
            $left, $node.cy, $node.r, $right, $color
    }
}
$vector = '<vector xmlns:android="http://schemas.android.com/apk/res/android" android:width="108dp" android:height="108dp" android:viewportWidth="1024" android:viewportHeight="1024">' +
    '<group android:pivotX="512" android:pivotY="512" android:scaleX="0.9" android:scaleY="0.9">' +
    ($paths -join "`n") + '</group></vector>'
[System.IO.File]::WriteAllText(
    (Join-Path $root 'android\app\src\main\res\drawable\ic_launcher_foreground.xml'), $vector)
Write-Output 'android\app\src\main\res\drawable\ic_launcher_foreground.xml (SVG, adaptive safe zone)'
