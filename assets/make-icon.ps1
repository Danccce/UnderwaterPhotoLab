# Zeichnet das Programmsymbol (PNG je Größe) und packt es als icon.ico (PNG-Einträge, ab Vista).
# Aufruf im Repo: powershell -File assets\make-icon.ps1
Add-Type -AssemblyName System.Drawing
$dir = $PSScriptRoot
$sizes = 256, 64, 48, 32, 24, 16

function Draw([int]$n) {
    $bmp = New-Object System.Drawing.Bitmap $n, $n, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)
    $s = $n / 256.0

    # Abgerundetes Quadrat, Verlauf Tiefblau → Türkis.
    $m = 8 * $s; $w = $n - 2 * $m; $r = 56 * $s
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddArc($m, $m, 2 * $r, 2 * $r, 180, 90)
    $path.AddArc($m + $w - 2 * $r, $m, 2 * $r, 2 * $r, 270, 90)
    $path.AddArc($m + $w - 2 * $r, $m + $w - 2 * $r, 2 * $r, 2 * $r, 0, 90)
    $path.AddArc($m, $m + $w - 2 * $r, 2 * $r, 2 * $r, 90, 90)
    $path.CloseFigure()
    $rect = New-Object System.Drawing.RectangleF $m, $m, $w, $w
    $grad = New-Object System.Drawing.Drawing2D.LinearGradientBrush $rect, ([System.Drawing.Color]::FromArgb(255, 18, 116, 160)), ([System.Drawing.Color]::FromArgb(255, 8, 36, 84)), 90.0
    $g.FillPath($grad, $path)

    # Ring: oben weiß, unten korallenorange (wiederhergestelltes Rot).
    $c = 128 * $s; $rad = 66 * $s; $pen = [Math]::Max(2.0, 26 * $s)
    $box = New-Object System.Drawing.RectangleF ($c - $rad), ($c - $rad + 6 * $s), (2 * $rad), (2 * $rad)
    $white = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), $pen
    $coral = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 255, 112, 72)), $pen
    $g.DrawArc($white, $box, 180, 180)
    $g.DrawArc($coral, $box, 0, 180)

    # Luftblasen oben rechts (bei sehr kleinen Größen weggelassen).
    if ($n -ge 32) {
        $b = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(230, 255, 255, 255))
        $g.FillEllipse($b, 184 * $s, 40 * $s, 22 * $s, 22 * $s)
        $g.FillEllipse($b, 206 * $s, 70 * $s, 13 * $s, 13 * $s)
    }
    $g.Dispose()
    return $bmp
}

$pngs = @()
foreach ($n in $sizes) {
    $bmp = Draw $n
    $file = Join-Path $dir "icon-$n.png"
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    $pngs += , [System.IO.File]::ReadAllBytes($file)
}

# ICO: Kopf, Verzeichnis (16 Byte je Bild), dann die PNG-Daten.
$ms = New-Object System.IO.MemoryStream
$bw = New-Object System.IO.BinaryWriter $ms
$bw.Write([UInt16]0); $bw.Write([UInt16]1); $bw.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $n = $sizes[$i]; $len = $pngs[$i].Length
    $bw.Write([byte]($(if ($n -ge 256) { 0 } else { $n }))); $bw.Write([byte]($(if ($n -ge 256) { 0 } else { $n })))
    $bw.Write([byte]0); $bw.Write([byte]0); $bw.Write([UInt16]1); $bw.Write([UInt16]32)
    $bw.Write([UInt32]$len); $bw.Write([UInt32]$offset)
    $offset += $len
}
foreach ($p in $pngs) { $bw.Write($p) }
$bw.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $dir "icon.ico"), $ms.ToArray())
"icon.ico mit $($sizes.Count) Größen geschrieben"
