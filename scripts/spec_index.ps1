# 生成 ISO 32000-1 规范文本索引与元数据。
# 依赖本机 Ghostscript（gswin64c / gswin32c / gs）的 txtwrite 设备；只在显式 -Update 时改写仓库文件。
[CmdletBinding()]
param(
    [string]$Root = '',
    [switch]$Update,
    [switch]$AcceptEditionChange
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'spec_lib.ps1')

if (-not $Root) { $Root = Split-Path -Parent $PSScriptRoot }
$Root = (Resolve-Path -LiteralPath $Root).Path

$pdfRel = 'docs/PDF32000_2008.pdf'
$pdfPath = Join-Path $Root $pdfRel
$indexDir = Join-Path $Root 'docs/spec-index'
$txtPath = Join-Path $indexDir 'iso32000-1.txt'
$metaPath = Join-Path $indexDir 'meta.json'

if (-not (Test-Path -LiteralPath $pdfPath)) { throw ('规范 PDF 不存在：' + $pdfRel) }

function Find-Ghostscript {
    foreach ($name in @('gswin64c', 'gswin32c', 'gs')) {
        $cmd = Get-Command $name -ErrorAction SilentlyContinue
        if ($cmd) { return $cmd.Source }
    }
    return $null
}

function Get-SentinelResult {
    param($Model, $Sentinel)
    if ($Sentinel.kind -eq 'anchor') {
        $refs = [pscustomobject]@{ All = @($Sentinel.anchor); Tables = @(); Explicit = @($Sentinel.anchor); Unresolved = @() }
    }
    else {
        $refs = [pscustomobject]@{ All = @(); Tables = @([int]$Sentinel.table); Explicit = @(); Unresolved = @() }
    }
    $windows = Get-SpecWindows -Refs $refs -Model $Model
    return (Test-SpecKeyInWindows -Key $Sentinel.key -Windows $windows -Model $Model)
}

$gs = Find-Ghostscript
if (-not $gs) { throw '未找到 Ghostscript（gswin64c / gswin32c / gs），无法抽取规范文本。' }
$gsVersion = ((& $gs --version) 2>&1 | Select-Object -First 1)

$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ('spec_index_' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null
$tmpTxt = Join-Path $tmpDir 'iso32000-1.txt'
try {
    Write-Host ('抽取规范文本：{0}' -f $pdfRel)
    $flags = @('-q', '-dNOPAUSE', '-dBATCH', '-sDEVICE=txtwrite', ('-sOutputFile=' + $tmpTxt), $pdfPath)
    & $gs @flags | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $tmpTxt)) { throw ('Ghostscript 抽取失败，退出码 ' + $LASTEXITCODE) }

    $raw = [System.IO.File]::ReadAllText($tmpTxt, [System.Text.Encoding]::UTF8)
    $text = ($raw -replace "`r`n", "`n") -replace "`r", "`n"
    $lines = $text -split "`n"
    $model = Get-SpecModel -Lines $lines

    $chapters = @{}
    foreach ($id in $model.SectionIds) {
        $ch = ($id -split '\.')[0]
        if (-not $chapters.ContainsKey($ch)) { $chapters[$ch] = 0 }
        $chapters[$ch]++
    }
    foreach ($ch in 7..14) {
        if (-not $chapters.ContainsKey([string]$ch)) { throw ('索引缺少第 ' + $ch + ' 章小节，抽取结果不完整') }
    }

    $sentinels = @(
        [pscustomobject]@{ kind = 'anchor'; anchor = '8.9.5'; key = 'Matrix'; present = $false },
        [pscustomobject]@{ kind = 'anchor'; anchor = '8.9.5'; key = 'Width'; present = $true },
        [pscustomobject]@{ kind = 'anchor'; anchor = '8.10.2'; key = 'Matrix'; present = $true },
        [pscustomobject]@{ kind = 'table'; table = 89; key = 'Matrix'; present = $false },
        [pscustomobject]@{ kind = 'table'; table = 95; key = 'Matrix'; present = $true }
    )
    $badSentinels = @()
    foreach ($s in $sentinels) {
        $actual = Get-SentinelResult -Model $model -Sentinel $s
        if ($actual -ne [bool]$s.present) {
            $label = if ($s.kind -eq 'anchor') { $s.anchor } else { 'Table ' + $s.table }
            $badSentinels += ('{0} /{1} 期望 {2}，实际 {3}' -f $label, $s.key, $s.present, $actual)
        }
    }
    if ($badSentinels.Count -gt 0) {
        throw ('哨兵校验失败，拒绝写入索引：' + ($badSentinels -join '；'))
    }

    $sourceHash = Get-Sha256Hex $pdfPath
    $textHash = Get-Sha256OfText $text

    if ((Test-Path -LiteralPath $metaPath) -and -not $AcceptEditionChange) {
        $old = [System.IO.File]::ReadAllText($metaPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
        if ($old.source_sha256 -ne $sourceHash) {
            throw ('源 PDF 哈希已变化（旧 {0}，新 {1}）。确认是标准换版后加 -AcceptEditionChange 重新生成。' -f $old.source_sha256, $sourceHash)
        }
    }

    if (-not $Update) {
        Write-Host ('校验通过：小节 {0} 个，表 {1} 张；未指定 -Update，不写入。' -f $model.SectionIds.Count, $model.Tables.Count)
        exit 0
    }

    New-Item -ItemType Directory -Force -Path $indexDir | Out-Null
    [System.IO.File]::WriteAllText($txtPath, $text, (New-Object System.Text.UTF8Encoding($false)))

    $meta = [ordered]@{
        schema           = 1
        source           = $pdfRel
        source_sha256    = $sourceHash
        source_bytes     = (Get-Item -LiteralPath $pdfPath).Length
        text             = 'docs/spec-index/iso32000-1.txt'
        text_sha256      = $textHash
        text_bytes       = (Get-Item -LiteralPath $txtPath).Length
        generator        = 'scripts/spec_index.ps1'
        backend          = 'ghostscript txtwrite'
        backend_version  = [string]$gsVersion
        flags            = '-q -dNOPAUSE -dBATCH -sDEVICE=txtwrite'
        sections_7_14    = $model.SectionIds.Count
        tables           = $model.Tables.Count
        annexes          = @($model.ById.Keys | Where-Object { $_ -like 'Annex *' }).Count
        sentinels        = $sentinels
    }
    $json = ($meta | ConvertTo-Json -Depth 6)
    [System.IO.File]::WriteAllText($metaPath, $json + "`n", (New-Object System.Text.UTF8Encoding($false)))

    Write-Host ('已写入：{0}（{1} 字节）、{2}' -f 'docs/spec-index/iso32000-1.txt', (Get-Item -LiteralPath $txtPath).Length, 'docs/spec-index/meta.json')
    Write-Host ('小节 7-14：{0}；表：{1}；哨兵：全部通过' -f $model.SectionIds.Count, $model.Tables.Count)
}
finally {
    if (Test-Path -LiteralPath $tmpDir) { Remove-Item -LiteralPath $tmpDir -Force -Recurse }
}
