# 规范—规划—实现 三方一致性检查（S1–S8）。
# 只读：不修改仓库文件；索引由 scripts/spec_index.ps1 生成。
[CmdletBinding()]
param(
    [string]$Root = '',
    [switch]$Strict,
    [switch]$Json,
    [int]$DetailLimit = 20,
    [switch]$RunTests,
    [switch]$RebuildVerify,
    [string]$TestResults = '',
    [string]$IndexDir = '',
    [switch]$EmitBaseline
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'spec_lib.ps1')

# 状态符号用码位构造，避免脚本依赖源文件编码
$S_OK   = [string][char]0x2705
$S_MID  = [string]([char]0xD83D) + [string]([char]0xDFE1)
$S_OPEN = [string][char]0x2B1C
$S_NO   = [string][char]0x274C
$S_DASH = [string][char]0x2014
$STATUS_SYMBOLS = @($S_OK, $S_MID, $S_OPEN, $S_NO, $S_DASH)

if (-not $Root) { $Root = Split-Path -Parent $PSScriptRoot }
$Root = (Resolve-Path -LiteralPath $Root).Path
if (-not $IndexDir) { $IndexDir = Join-Path $Root 'docs/spec-index' }

$script:checks = @()
$script:baselined = 0
$script:unmatchedViolations = @()

function Add-Check {
    param(
        [string]$Id,
        [string]$Name,
        [string]$Status,
        [string]$Summary,
        [string[]]$Details = @(),
        [int]$Baselined = 0
    )
    $script:checks += [pscustomobject]@{
        id        = $Id
        name      = $Name
        status    = $Status
        summary   = $Summary
        details   = $Details
        baselined = $Baselined
    }
}

function Read-Utf8Lines {
    param([string]$Path)
    $text = [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8)
    return , ($text -split "`n")
}

function Get-RelPath {
    param([string]$Path)
    $full = (Resolve-Path -LiteralPath $Path).Path
    if ($full.StartsWith($Root)) { return $full.Substring($Root.Length).TrimStart('\', '/').Replace('\', '/') }
    return $full.Replace('\', '/')
}

function Split-Cells {
    param([string]$Line)
    # 表格里的 `\|` 是转义竖线；行内代码 `...|...` 里的竖线也不分隔单元格
    $cells = New-Object System.Collections.Generic.List[string]
    $sb = New-Object System.Text.StringBuilder
    $inCode = $false
    $tick = [char]0x60
    for ($i = 0; $i -lt $Line.Length; $i++) {
        $ch = $Line[$i]
        if ($ch -eq $tick) { $inCode = -not $inCode; [void]$sb.Append($ch); continue }
        if ($ch -eq '\' -and ($i + 1) -lt $Line.Length -and $Line[$i + 1] -eq '|') { [void]$sb.Append('|'); $i++; continue }
        if ($ch -eq '|' -and -not $inCode) { $cells.Add($sb.ToString().Trim()); [void]$sb.Clear(); continue }
        [void]$sb.Append($ch)
    }
    $cells.Add($sb.ToString().Trim())
    return , @($cells | Where-Object { $_ -ne '' })
}

# ---------------- 基线 ----------------
function Read-Baseline {
    param([string]$Path)
    $items = @()
    if (-not (Test-Path -LiteralPath $Path)) { return , $items }
    $cur = $null
    foreach ($line in [System.IO.File]::ReadAllLines($Path, [System.Text.Encoding]::UTF8)) {
        if ($line -match '^\s*\[\[violation\]\]\s*$') {
            if ($cur) { $items += [pscustomobject]$cur }
            $cur = @{}
            continue
        }
        if ($null -eq $cur) { continue }
        if ($line -match '^\s*([A-Za-z_]+)\s*=\s*"(.*)"\s*$') {
            $cur[$Matches[1]] = $Matches[2]
        }
    }
    if ($cur) { $items += [pscustomobject]$cur }
    return , $items
}

function Get-ViolationKey {
    param($V)
    return ('{0}|{1}|{2}|{3}|{4}' -f $V.check, $V.file, $V.unit, $V.anchor, $V.key)
}

$baselinePath = Join-Path $Root 'scripts/spec.baseline.toml'
$baseline = Read-Baseline $baselinePath
$baselineMap = @{}
foreach ($b in $baseline) {
    $baselineMap[('{0}|{1}|{2}|{3}|{4}' -f $b.check, $b.file, $b.unit, $b.anchor, $b.key)] = $false
}

# ---------------- 索引 ----------------
$txtPath = Join-Path $IndexDir 'iso32000-1.txt'
$metaPath = Join-Path $IndexDir 'meta.json'
$meta = $null
$indexLines = @()
$model = $null
$indexError = ''
if (-not (Test-Path -LiteralPath $txtPath)) { $indexError = '索引文本不存在：docs/spec-index/iso32000-1.txt' }
elseif (-not (Test-Path -LiteralPath $metaPath)) { $indexError = '索引元数据不存在：docs/spec-index/meta.json' }
else {
    try {
        $meta = [System.IO.File]::ReadAllText($metaPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
        $indexLines = Read-Utf8Lines $txtPath
        $model = Get-SpecModel -Lines $indexLines
    }
    catch { $indexError = ('索引读取失败：' + $_.Exception.Message) }
}

# ---------------- 矩阵与规划解析 ----------------
function Get-MatrixRows {
    $rows = @()
    $dirs = @(Get-ChildItem (Join-Path $Root 'docs/matrix') -Directory -ErrorAction SilentlyContinue | Where-Object { $_.Name -like 'ch*' })
    foreach ($dir in $dirs) {
        foreach ($md in @(Get-ChildItem $dir.FullName -Recurse -File -Filter '*.md')) {
            $rel = Get-RelPath $md.FullName
            $lines = Read-Utf8Lines $md.FullName
            $headingAnchor = ''
            for ($i = 0; $i -lt $lines.Length; $i++) {
                $line = $lines[$i]
                # B 型表没有独立锚点列，条款范围写在文件标题里
                if ($line -match '^#\s+\S') { $headingAnchor = $line.TrimStart('#', ' '); continue }
                if (-not $line.StartsWith('|')) { continue }
                $cells = Split-Cells $line
                if ($cells.Count -lt 6) { continue }
                $status = $null
                $type = ''
                if ($STATUS_SYMBOLS -contains $cells[3] -and $cells.Count -ge 8) { $status = $cells[3]; $type = 'A' }
                elseif ($STATUS_SYMBOLS -contains $cells[2]) { $status = $cells[2]; $type = 'B' }
                if (-not $status) { continue }
                $unit = $cells[0]
                if ($type -eq 'A') {
                    $anchorText = $cells[0]
                    $feature = $cells[1]
                    $evidence = $cells[$cells.Count - 1]
                }
                else {
                    $anchorText = ($headingAnchor + ' ' + $line).Trim()
                    $feature = $cells[$cells.Count - 1]
                    $evidence = $cells[$cells.Count - 1]
                }
                $refs = if ($model) { Get-SpecRefs -Text $anchorText -Model $model } else { $null }
                $rows += [pscustomobject]@{
                    file     = $rel
                    line     = ($i + 1)
                    unit     = $unit
                    type     = $type
                    status   = $status
                    anchor   = $anchorText
                    feature  = $feature
                    evidence = $evidence
                    text     = $line
                    refs     = $refs
                }
            }
        }
    }
    return , $rows
}

function Get-RoadmapItems {
    $items = @()
    $dir = Join-Path $Root 'docs/roadmap/stages'
    if (-not (Test-Path -LiteralPath $dir)) { return , $items }
    foreach ($md in @(Get-ChildItem $dir -File -Filter '*.md')) {
        $rel = Get-RelPath $md.FullName
        $lines = Read-Utf8Lines $md.FullName
        for ($i = 0; $i -lt $lines.Length; $i++) {
            $line = $lines[$i]
            if ($line -notmatch '^\s*-\s*\[([ xX])\]\s*(?:\*\*)?([A-F][0-9]+)') { continue }
            $id = $Matches[2]
            $checked = ($Matches[1] -match '[xX]')
            # 非规范标记：只允许出现在阶段条目；必须写成「（非规范：依据）」并带依据类别
            $marker = [regex]::Match($line, '\uFF08\u975E\u89C4\u8303([^\uFF09]*)\uFF09')
            $hasMarker = $marker.Success
            $markerBody = ''
            $markerValid = $false
            if ($hasMarker) {
                $body = $marker.Groups[1].Value.Trim()
                if ($body.StartsWith([string][char]0xFF1A)) { $markerBody = $body.Substring(1).Trim() }
                if ($markerBody -match '\u5DE5\u7A0B\u51B3\u7B56' -or $markerBody -match '\u5185\u90E8\u7EA6\u5B9A') { $markerValid = $true }
                elseif ($markerBody -match '(?i)(ISO|ECMA|IEEE|RFC|ITU|ANSI|GB|W3C)[\s\-]*\d[^\uFF09]*?(§\s*\S+|第\s*\d+\s*[章节条])') { $markerValid = $true }
            }
            $anchorText = $line
            if ($line -match '\uFF08\u89C4\u8303\uFF1A([^\uFF09]*)\uFF09') { $anchorText = $Matches[1] }
            $refs = if ($model) { Get-SpecRefs -Text $anchorText -Model $model } else { $null }
            $items += [pscustomobject]@{
                file          = $rel
                line          = ($i + 1)
                unit          = $id
                checked       = $checked
                nonSpec       = $hasMarker
                nonSpecValid  = $markerValid
                nonSpecBasis  = $markerBody
                marker        = $(if ($hasMarker) { $marker.Value } else { '' })
                text          = $line
                anchor        = $anchorText
                refs          = $refs
            }
        }
    }
    return , $items
}

$matrixRows = Get-MatrixRows
$roadmapItems = Get-RoadmapItems

# 基线应用：返回未登记违规与已登记条目
function Apply-Baseline {
    param([string]$Check, $Violations)
    $unmatched = @()
    $matched = 0
    $matchedDetails = @()
    foreach ($v in $Violations) {
        $k = Get-ViolationKey $v
        if ($baselineMap.ContainsKey($k)) {
            $baselineMap[$k] = $true
            $matched++
            if ($matchedDetails.Count -lt $DetailLimit) { $matchedDetails += ('[基线] ' + $v.message) }
        }
        else { $unmatched += $v }
    }
    $script:unmatchedViolations += $unmatched
    return [pscustomobject]@{ Unmatched = $unmatched; Matched = $matched; MatchedDetails = $matchedDetails }
}

function Format-Violations {
    param($Violations, [int]$Limit)
    $out = @()
    foreach ($v in $Violations) { if ($out.Count -ge $Limit) { break }; $out += $v.message }
    return , $out
}

# ---------------- S1 索引完整性 ----------------
$s1Details = @()
$s1Fail = $false
if ($indexError) { $s1Fail = $true; $s1Details += $indexError }
else {
    if ([int]$meta.schema -ne 1) { $s1Fail = $true; $s1Details += ('meta.schema 期望 1，实际 ' + $meta.schema) }
    $actualTextHash = Get-Sha256Hex $txtPath
    if ($actualTextHash -ne $meta.text_sha256) { $s1Fail = $true; $s1Details += ('文本哈希不匹配：meta {0}，实际 {1}' -f $meta.text_sha256, $actualTextHash) }
    $pdfPath = Join-Path $Root $meta.source
    if (Test-Path -LiteralPath $pdfPath) {
        $actualPdfHash = Get-Sha256Hex $pdfPath
        if ($actualPdfHash -ne $meta.source_sha256) { $s1Fail = $true; $s1Details += ('源 PDF 哈希不匹配：meta {0}，实际 {1}' -f $meta.source_sha256, $actualPdfHash) }
    }
    else { $s1Details += ('源 PDF 缺失，跳过源哈希：' + $meta.source) }
    if ($model.SectionIds.Count -lt 400) { $s1Fail = $true; $s1Details += ('小节 7-14 数量异常：' + $model.SectionIds.Count) }
    if ($model.Tables.Count -lt 250) { $s1Fail = $true; $s1Details += ('表数量异常：' + $model.Tables.Count) }
    foreach ($ch in 7..14) {
        if (@($model.SectionIds | Where-Object { $_.StartsWith([string]$ch + '.') }).Count -eq 0) { $s1Fail = $true; $s1Details += ('缺少第 ' + $ch + ' 章小节') }
    }
    foreach ($s in $meta.sentinels) {
        if ($s.kind -eq 'anchor') {
            $refs = [pscustomobject]@{ All = @($s.anchor); Tables = @(); Explicit = @($s.anchor); Unresolved = @() }
        }
        else {
            $refs = [pscustomobject]@{ All = @(); Tables = @([int]$s.table); Explicit = @(); Unresolved = @() }
        }
        $windows = Get-SpecWindows -Refs $refs -Model $model
        $actual = Test-SpecKeyInWindows -Key $s.key -Windows $windows -Model $model
        if ($actual -ne [bool]$s.present) {
            $s1Fail = $true
            $label = if ($s.kind -eq 'anchor') { $s.anchor } else { 'Table ' + $s.table }
            $s1Details += ('哨兵不符：{0} /{1} 期望 {2}，实际 {3}' -f $label, $s.key, $s.present, $actual)
        }
    }
}
$s1Status = if ($s1Fail) { 'FAIL' } else { 'PASS' }
$s1Summary = if ($indexError) { '索引不可用' } else { ('小节 {0}｜表 {1}｜哨兵 {2} 条' -f $model.SectionIds.Count, $model.Tables.Count, @($meta.sentinels).Count) }
Add-Check -Id 'S1' -Name '索引完整性与防篡改' -Status $s1Status -Summary $s1Summary -Details @($s1Details | Select-Object -First $DetailLimit)

# ---------------- S2 索引可重建 ----------------
$s2Status = 'SKIP'
$s2Summary = '未指定 -RebuildVerify；日常检查保持离线低耗时'
$s2Details = @()
if ($RebuildVerify) {
    $gs = $null
    $tried = @()
    foreach ($name in @('gswin64c', 'gswin32c', 'gs')) {
        $tried += $name
        $cmd = Get-Command $name -ErrorAction SilentlyContinue
        if ($cmd) { $gs = $cmd.Source; break }
    }
    if (-not $gs) {
        $s2Status = 'SKIP'
        $s2Summary = '未找到 Ghostscript，无法重建比对'
        $s2Details += ('已尝试：' + ($tried -join ', '))
    }
    elseif (-not $model) {
        $s2Status = 'FAIL'
        $s2Summary = '索引不可用，无法重建比对'
    }
    else {
        $tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ('spec_rebuild_' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null
        $tmpTxt = Join-Path $tmpDir 'iso32000-1.txt'
        try {
            & $gs @('-q', '-dNOPAUSE', '-dBATCH', '-sDEVICE=txtwrite', ('-sOutputFile=' + $tmpTxt), (Join-Path $Root $meta.source)) | Out-Null
            if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $tmpTxt)) { throw ('Ghostscript 退出码 ' + $LASTEXITCODE) }
            $rebuiltText = ([System.IO.File]::ReadAllText($tmpTxt, [System.Text.Encoding]::UTF8) -replace "`r`n", "`n") -replace "`r", "`n"
            $rebuilt = Get-SpecModel -Lines ($rebuiltText -split "`n")
            $oldIds = @($model.SectionIds | Sort-Object)
            $newIds = @($rebuilt.SectionIds | Sort-Object)
            $diff = Compare-Object -ReferenceObject $oldIds -DifferenceObject $newIds
            if ($diff) {
                $s2Status = 'FAIL'
                $s2Summary = ('重建后小节集合不一致，差异 ' + @($diff).Count + ' 项')
                $s2Details += @($diff | Select-Object -First $DetailLimit | ForEach-Object { ('{0} {1}' -f $_.SideIndicator, $_.InputObject) })
            }
            else {
                $oldTables = @($model.TableOwner.Keys | Sort-Object)
                $newTables = @($rebuilt.TableOwner.Keys | Sort-Object)
                $tdiff = Compare-Object -ReferenceObject $oldTables -DifferenceObject $newTables
                if ($tdiff) {
                    $s2Status = 'FAIL'
                    $s2Summary = ('重建后表集合不一致，差异 ' + @($tdiff).Count + ' 项')
                }
                else {
                    $s2Status = 'PASS'
                    $s2Summary = ('重建一致：小节 {0}，表 {1}' -f $newIds.Count, $rebuilt.Tables.Count)
                }
            }
        }
        catch {
            $s2Status = 'FAIL'
            $s2Summary = ('重建失败：' + $_.Exception.Message)
        }
        finally {
            if (Test-Path -LiteralPath $tmpDir) { Remove-Item -LiteralPath $tmpDir -Force -Recurse }
        }
    }
}
Add-Check -Id 'S2' -Name '索引可重建' -Status $s2Status -Summary $s2Summary -Details @($s2Details | Select-Object -First $DetailLimit)

# ---------------- S3 规范锚点 ----------------
# 非规范条目上限：防止「标记扩散」把锚点机制架空
$NON_SPEC_LIMIT = 20

# 返回条目文本里能在索引中真正解析到的锚点（条款或已存在的表）
function Get-ResolvableAnchors {
    param($Refs, $Model)
    $hits = @()
    foreach ($a in @($Refs.All)) {
        if ($a -like 'Chapter:*') {
            if ($Model.ChapterIds.ContainsKey($a.Substring(8))) { $hits += $a }
            continue
        }
        if ($Model.ById.ContainsKey($a)) { $hits += $a }
    }
    foreach ($t in @($Refs.Tables)) {
        if ($Model.Tables.ContainsKey([int]$t)) { $hits += ('Table ' + $t) }
    }
    return , @($hits | Select-Object -Unique)
}

$s3Violations = @()
$nonSpecItems = @()
if (-not $model) {
    $s3Violations += [pscustomobject]@{ check = 'S3'; file = ''; unit = ''; anchor = ''; key = ''; message = '索引不可用，无法解析锚点' }
}
else {
    foreach ($row in $matrixRows) {
        # 矩阵行按定义必须对得上规范，出现非规范标记即 FAIL
        if ($row.text -match '\uFF08\u975E\u89C4\u8303') {
            $s3Violations += [pscustomobject]@{ check = 'S3'; file = $row.file; unit = $row.unit; anchor = ''; key = ''; message = ('{0}:{1} [{2}] 矩阵行不允许使用非规范标记' -f $row.file, $row.line, $row.unit) }
        }
        if ($row.refs.Explicit.Count -eq 0 -and $row.refs.Tables.Count -eq 0) {
            $s3Violations += [pscustomobject]@{ check = 'S3'; file = $row.file; unit = $row.unit; anchor = ''; key = ''; message = ('{0}:{1} [{2}] 无规范锚点' -f $row.file, $row.line, $row.unit) }
        }
        foreach ($u in $row.refs.Unresolved) {
            $s3Violations += [pscustomobject]@{ check = 'S3'; file = $row.file; unit = $row.unit; anchor = $u; key = ''; message = ('{0}:{1} [{2}] 锚点不存在：{3}' -f $row.file, $row.line, $row.unit, $u) }
        }
    }
    foreach ($it in $roadmapItems) {
        if ($it.nonSpec) {
            if (-not $it.nonSpecValid) {
                $s3Violations += [pscustomobject]@{ check = 'S3'; file = $it.file; unit = $it.unit; anchor = ''; key = ''; message = ('{0}:{1} [{2}] 非规范标记缺少依据（需写 工程决策 / 内部约定 / 其他规范+条款号）：{3}' -f $it.file, $it.line, $it.unit, $it.marker) }
                continue
            }
            # 反滥用：标记不是免死金牌，能解析到锚点说明它不是非规范条目
            $resolved = Get-ResolvableAnchors -Refs $it.refs -Model $model
            if ($resolved.Count -gt 0) {
                $s3Violations += [pscustomobject]@{ check = 'S3'; file = $it.file; unit = $it.unit; anchor = ''; key = ''; message = ('{0}:{1} [{2}] 非规范标记滥用：条目仍能解析出规范锚点 {3}' -f $it.file, $it.line, $it.unit, ($resolved -join ',')) }
            }
            else {
                $nonSpecItems += [pscustomobject]@{
                    file  = $it.file
                    line  = $it.line
                    unit  = $it.unit
                    basis = $it.nonSpecBasis
                    text  = $it.marker
                }
            }
            continue
        }
        if ($it.refs.Explicit.Count -eq 0 -and $it.refs.Tables.Count -eq 0) {
            $s3Violations += [pscustomobject]@{ check = 'S3'; file = $it.file; unit = $it.unit; anchor = ''; key = ''; message = ('{0}:{1} [{2}] 无规范锚点' -f $it.file, $it.line, $it.unit) }
        }
        foreach ($u in $it.refs.Unresolved) {
            $s3Violations += [pscustomobject]@{ check = 'S3'; file = $it.file; unit = $it.unit; anchor = $u; key = ''; message = ('{0}:{1} [{2}] 锚点不存在：{3}' -f $it.file, $it.line, $it.unit, $u) }
        }
    }
    if ($nonSpecItems.Count -gt $NON_SPEC_LIMIT) {
        $s3Violations += [pscustomobject]@{ check = 'S3'; file = ''; unit = ''; anchor = ''; key = ''; message = ('非规范条目超上限：{0} > {1}，需人工复核' -f $nonSpecItems.Count, $NON_SPEC_LIMIT) }
    }
}
$applied = Apply-Baseline -Check 'S3' -Violations $s3Violations
$s3Status = if (@($applied.Unmatched).Count -gt 0) { 'FAIL' } else { 'PASS' }
$s3Details = @()
$s3Details += Format-Violations $applied.Unmatched $DetailLimit
$s3Details += $applied.MatchedDetails
foreach ($n in @($nonSpecItems | Select-Object -First $DetailLimit)) {
    $s3Details += ('[非规范] {0}:{1} [{2}] {3}' -f $n.file, $n.line, $n.unit, $n.text)
}
$s3Summary = ('矩阵行 {0}｜规划条目 {1}｜未登记违规 {2}｜基线 {3}｜非规范 {4}/{5}' -f $matrixRows.Count, $roadmapItems.Count, @($applied.Unmatched).Count, $applied.Matched, $nonSpecItems.Count, $NON_SPEC_LIMIT)
Add-Check -Id 'S3' -Name '规范锚点可解析' -Status $s3Status -Summary $s3Summary -Details $s3Details -Baselined $applied.Matched
$script:baselined += $applied.Matched

# 基线里的锚点用短形式：B 型表整行过长，退化为显式条款/表号
function Format-AnchorShort {
    param([string]$Anchor, $Refs)
    if ($Anchor.Length -le 60) { return $Anchor }
    $parts = @()
    if ($Refs) {
        $parts += @($Refs.Explicit)
        foreach ($t in @($Refs.Tables)) { $parts += ('Table ' + $t) }
    }
    $parts = @($parts | Select-Object -Unique)
    if ($parts.Count -eq 0) { return '-' }
    return ($parts -join ',')
}

# ---------------- S4 键名溯源 ----------------
$s4Violations = @()
$s4Keys = 0
if ($model) {
    foreach ($row in $matrixRows) {
        if ($row.refs.Explicit.Count -eq 0) { continue }
        $windows = Get-SpecWindows -Refs $row.refs -Model $model
        foreach ($key in (Get-SpecKeys $row.feature)) {
            $s4Keys++
            if (-not (Test-SpecKeyInWindows -Key $key.TrimStart('/') -Windows $windows -Model $model)) {
                $s4Violations += [pscustomobject]@{ check = 'S4'; file = $row.file; unit = $row.unit; anchor = (Format-AnchorShort $row.anchor $row.refs); key = $key; message = ('{0}:{1} [{2}] 键 {3} 不在所引条款/表内（锚点 {4}）' -f $row.file, $row.line, $row.unit, $key, (Format-AnchorShort $row.anchor $row.refs)) }
            }
        }
    }
    foreach ($it in $roadmapItems) {
        if ($it.nonSpec -or $it.refs.Explicit.Count -eq 0) { continue }
        $windows = Get-SpecWindows -Refs $it.refs -Model $model
        $itAnchor = Format-AnchorShort $it.anchor $it.refs
        foreach ($key in (Get-SpecKeys $it.text)) {
            $s4Keys++
            if (-not (Test-SpecKeyInWindows -Key $key.TrimStart('/') -Windows $windows -Model $model)) {
                $s4Violations += [pscustomobject]@{ check = 'S4'; file = $it.file; unit = $it.unit; anchor = $itAnchor; key = $key; message = ('{0}:{1} [{2}] 键 {3} 不在所引条款/表内（锚点 {4}）' -f $it.file, $it.line, $it.unit, $key, $itAnchor) }
            }
        }
    }
}
$applied = Apply-Baseline -Check 'S4' -Violations $s4Violations
$s4Status = if (@($applied.Unmatched).Count -gt 0) { 'FAIL' } else { 'PASS' }
$s4Details = @()
$s4Details += Format-Violations $applied.Unmatched $DetailLimit
$s4Details += $applied.MatchedDetails
$s4Summary = ('参与溯源键 {0}｜未登记违规 {1}｜基线 {2}' -f $s4Keys, @($applied.Unmatched).Count, $applied.Matched)
Add-Check -Id 'S4' -Name '键名溯源' -Status $s4Status -Summary $s4Summary -Details $s4Details -Baselined $applied.Matched
$script:baselined += $applied.Matched

# ---------------- S5 规划 → 矩阵回执 ----------------
$s5Violations = @()
$s5Covered = 0
if ($model) {
    foreach ($it in $roadmapItems) {
        if ($it.nonSpec) { continue }
        if ($it.refs.Explicit.Count -eq 0) { continue }
        $itemOk = $true
        foreach ($ref in $it.refs.Explicit) {
            $covering = @()
            foreach ($row in $matrixRows) {
                if (-not $row.refs) { continue }
                foreach ($ra in $row.refs.All) {
                    $hit = $false
                    if ($ref -like 'Chapter:*') {
                        $prefix = $ref.Substring(8) + '.'
                        $hit = $ra.StartsWith($prefix)
                    }
                    elseif ($ref -like 'Annex *') {
                        $hit = ($ra -eq $ref) -or $ra.StartsWith($ref + '.')
                    }
                    else {
                        $hit = ($ra -eq $ref) -or $ra.StartsWith($ref + '.') -or $ref.StartsWith($ra + '.')
                    }
                    if ($hit) { $covering += $row; break }
                }
            }
            if ($covering.Count -eq 0) {
                $s5Violations += [pscustomobject]@{ check = 'S5'; file = $it.file; unit = $it.unit; anchor = $ref; key = ''; message = ('{0}:{1} [{2}] 锚点 {3} 无对应矩阵行' -f $it.file, $it.line, $it.unit, $ref) }
                $itemOk = $false
                continue
            }
            if ($it.checked) {
                $receipt = @($covering | Where-Object { $_.status -eq $S_OK -or $_.status -eq $S_MID })
                if ($receipt.Count -eq 0) {
                    $s5Violations += [pscustomobject]@{ check = 'S5'; file = $it.file; unit = $it.unit; anchor = $ref; key = ''; message = ('{0}:{1} [{2}] 已完成但锚点 {3} 的矩阵行均为 ⬜/❌' -f $it.file, $it.line, $it.unit, $ref) }
                    $itemOk = $false
                }
            }
        }
        if ($itemOk) { $s5Covered++ }
    }
}
$applied = Apply-Baseline -Check 'S5' -Violations $s5Violations
$s5Status = if (@($applied.Unmatched).Count -gt 0) { 'FAIL' } else { 'PASS' }
$s5Details = @()
$s5Details += Format-Violations $applied.Unmatched $DetailLimit
$s5Details += $applied.MatchedDetails
$s5Summary = ('有回执条目 {0}｜未登记违规 {1}｜基线 {2}' -f $s5Covered, @($applied.Unmatched).Count, $applied.Matched)
Add-Check -Id 'S5' -Name '规划 → 矩阵回执' -Status $s5Status -Summary $s5Summary -Details $s5Details -Baselined $applied.Matched
$script:baselined += $applied.Matched

# ---------------- 代码索引（S6/S7 共用） ----------------
$script:codeIndex = $null
function Get-CodeIndex {
    if ($script:codeIndex) { return $script:codeIndex }
    $defs = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::Ordinal)
    $tests = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::Ordinal)
    $suffixes = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::Ordinal)
    $sb = New-Object System.Text.StringBuilder
    $srcRoot = [System.IO.Path]::Combine($Root, 'src')
    $coreRoot = [System.IO.Path]::Combine($srcRoot, 'core')
    $testsRoot = [System.IO.Path]::Combine($Root, 'tests')
    $roots = @($Root, $srcRoot, $coreRoot, $testsRoot) | ForEach-Object { ($_ -replace '\\', '/').TrimEnd('/') }
    foreach ($base in @($srcRoot, $testsRoot)) {
        if (-not (Test-Path -LiteralPath $base)) { continue }
        foreach ($f in @(Get-ChildItem $base -Recurse -File)) {
            $full = $f.FullName -replace '\\', '/'
            foreach ($r in $roots) {
                if ($full.StartsWith($r + '/')) {
                    $rel = $full.Substring($r.Length + 1)
                    $segs = $rel -split '/'
                    for ($i = 0; $i -lt $segs.Count; $i++) { [void]$suffixes.Add(($segs[$i..($segs.Count - 1)] -join '/')) }
                    if ($segs.Count -gt 1) {
                        for ($i = 0; $i -lt ($segs.Count - 1); $i++) { [void]$suffixes.Add(($segs[$i..($segs.Count - 2)] -join '/')) }
                    }
                }
            }
            if ($f.Extension -ne '.rs') { continue }
            $text = [System.IO.File]::ReadAllText($f.FullName, [System.Text.Encoding]::UTF8)
            [void]$sb.AppendLine($text)
            foreach ($m in [regex]::Matches($text, '(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:const\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)')) { [void]$defs.Add($m.Groups[1].Value) }
            foreach ($m in [regex]::Matches($text, '(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait|type|const|static|mod|union)\s+([A-Za-z_][A-Za-z0-9_]*)')) { [void]$defs.Add($m.Groups[1].Value) }
            foreach ($m in [regex]::Matches($text, '(?s)#\[(?:[\w:]+::)?(?:test|rstest|test_case)\b[^\]]*\]\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)')) { [void]$tests.Add($m.Groups[1].Value) }
            if ($f.FullName -match '\\tests\\') {
                foreach ($m in [regex]::Matches($text, '(?m)^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)')) { [void]$tests.Add($m.Groups[1].Value) }
            }
        }
    }
    $script:codeIndex = [pscustomobject]@{ Defs = $defs; Tests = $tests; Suffixes = $suffixes; Text = $sb.ToString(); Cache = @{} }
    return $script:codeIndex
}

function Resolve-CodeToken {
    param([string]$Token, [bool]$IsMatrix)
    $t = $Token.Trim()
    if (-not $t) { return $null }
    if ($t.StartsWith('/')) { return $null }
    if ($t -match '\s') { return $null }
    if ($t -match '^[-#]') { return $null }
    if ($t -match '[<>+*=!?@$"''|&%]') { return $null }
    if ($t -match '^\.+$') { return $null }
    # 非 ASCII（中文、×、≤、箭头等）不是代码证据 token
    if ($t -match '[^\x20-\x7E]') { return $null }
    # 形如 q/Q 的短操作符对，不是模块路径
    if ($t -match '^[A-Za-z]{1,3}(/[A-Za-z]{1,3})+$') { return $null }

    $ci = Get-CodeIndex

    # 路径：含分隔符或已知扩展名；`::` 形式归符号分支（否则 Join-Path/Test-Path 会把前缀当 provider 名）
    $pathToken = $t -replace ':\d+(?:[-,]\d+)*$', ''
    if ($pathToken -notmatch '::' -and ($pathToken -match '[/\\]' -or $pathToken -match '\.(rs|toml|json|ps1|md|txt)$')) {
        if ($IsMatrix -and $pathToken -match '\.rs$') { return [pscustomobject]@{ Ok = $true; Kind = 'path'; Name = $pathToken; Skip = $true } }
        $variants = @($pathToken)
        if ($pathToken -match '\{([^}]*)\}') {
            $variants = @()
            $inside = $Matches[1]
            foreach ($alt in $inside.Split(',')) {
                $variants += ($pathToken -replace '\{[^}]*\}', $alt.Trim())
            }
        }
        $srcRoot = [System.IO.Path]::Combine($Root, 'src')
        $coreRoot = [System.IO.Path]::Combine([System.IO.Path]::Combine($Root, 'src'), 'core')
        $testsRoot = [System.IO.Path]::Combine($Root, 'tests')
        foreach ($v in $variants) {
            $v = $v.Replace('\', '/').TrimEnd('/')
            $rel = $v -replace '/', '\'
            $cands = @(
                [System.IO.Path]::Combine($Root, $rel),
                [System.IO.Path]::Combine($srcRoot, $rel),
                [System.IO.Path]::Combine($coreRoot, $rel),
                [System.IO.Path]::Combine($testsRoot, $rel)
            )
            if ($v -notmatch '\.[A-Za-z0-9]+$') {
                $cands += [System.IO.Path]::Combine($Root, $rel + '.rs')
                $cands += [System.IO.Path]::Combine($srcRoot, $rel + '.rs')
                $cands += [System.IO.Path]::Combine($coreRoot, $rel + '.rs')
                $cands += [System.IO.Path]::Combine($coreRoot, $rel, 'mod.rs')
                $cands += [System.IO.Path]::Combine($testsRoot, $rel + '.rs')
                $cands += [System.IO.Path]::Combine($testsRoot, $rel, 'mod.rs')
            }
            foreach ($c in $cands) { if (Test-Path -LiteralPath $c) { return [pscustomobject]@{ Ok = $true; Kind = 'path'; Name = $v; Skip = $false } } }
            # 子模块相对写法（如 assemble/annotation、navigation/structure）按仓库路径后缀命中
            if ($ci.Suffixes.Contains($v)) { return [pscustomobject]@{ Ok = $true; Kind = 'path'; Name = $v; Skip = $false } }
        }
        # 外部标识（CMap 名、第三方仓库名）不是本仓路径：首段不是已知模块且无源码扩展名时不计为违规
        $first = ($pathToken -split '[/\\]')[0]
        $knownRoots = @('src', 'tests', 'scripts', 'docs', 'core', 'reader', 'builder', 'types', 'content', 'filters', 'functions', 'writer', 'ccitt', 'assemble')
        if ($pathToken -notmatch '\.[A-Za-z0-9]+$' -and $knownRoots -notcontains $first) {
            return [pscustomobject]@{ Ok = $true; Kind = 'external'; Name = $pathToken; Skip = $true }
        }
        return [pscustomobject]@{ Ok = $false; Kind = 'path'; Name = $pathToken; Skip = $false }
    }

    # 符号：取最后一段
    $name = $t
    if ($name -match '::') {
        $prefix = ($name -split '::')[0]
        $name = ($name -split '::')[-1]
        # `路径::符号`：前缀路径也须存在（矩阵内 .rs 路径归 C2，不重复检查）
        if (($prefix -match '[/\\]' -or $prefix -match '\.rs$') -and -not ($IsMatrix -and $prefix -match '\.rs$')) {
            $prel = $prefix.Replace('\', '/').Replace('/', '\')
            $preads = @(
                [System.IO.Path]::Combine($Root, $prel),
                [System.IO.Path]::Combine([System.IO.Path]::Combine($Root, 'src'), $prel),
                [System.IO.Path]::Combine([System.IO.Path]::Combine([System.IO.Path]::Combine($Root, 'src'), 'core'), $prel),
                [System.IO.Path]::Combine([System.IO.Path]::Combine($Root, 'tests'), $prel)
            )
            $found = $false
            foreach ($c in $preads) { if (Test-Path -LiteralPath $c) { $found = $true; break } }
            if (-not $found) { return [pscustomobject]@{ Ok = $false; Kind = 'path'; Name = $prefix; Skip = $false } }
        }
    }
    elseif ($name -match '\.') { $name = ($name -split '\.')[-1] }
    $name = ($name -replace '\(.*$', '').Trim()
    if (-not $name) { return $null }

    if ($ci.Tests.Contains($name)) { return [pscustomobject]@{ Ok = $true; Kind = 'test'; Name = $name; Skip = $false } }
    if ($ci.Defs.Contains($name)) { return [pscustomobject]@{ Ok = $true; Kind = 'symbol'; Name = $name; Skip = $false } }
    if ($name -match '^[A-Za-z_][A-Za-z0-9_]*$' -and $name.Length -ge 4) {
        if (-not $ci.Cache.ContainsKey($name)) {
            $ci.Cache[$name] = $ci.Text -match ('(?<![A-Za-z0-9_])' + [regex]::Escape($name) + '(?![A-Za-z0-9_])')
        }
        if ($ci.Cache[$name]) { return [pscustomobject]@{ Ok = $true; Kind = 'symbol'; Name = $name; Skip = $false } }
        if ($t -match '::' -or $t -match '_' -or $t -match '\.') { return [pscustomobject]@{ Ok = $false; Kind = 'symbol'; Name = $name; Skip = $false } }
    }
    return $null
}

function Get-CodeTokens {
    param([string]$Text, [bool]$IsMatrix)
    $out = @()
    foreach ($m in [regex]::Matches($Text, '`([^`]*)`')) {
        $r = Resolve-CodeToken -Token $m.Groups[1].Value.Trim() -IsMatrix $IsMatrix
        if ($r -and -not $r.Skip) { $out += $r }
    }
    return , $out
}

# ---------------- S6 矩阵 → 实现回执 ----------------
$s6Violations = @()
$s6Rows = 0
foreach ($row in $matrixRows) {
    if ($row.status -ne $S_OK -and $row.status -ne $S_MID) { continue }
    $s6Rows++
    $tokens = Get-CodeTokens -Text $row.evidence -IsMatrix $true
    $hasTest = @($tokens | Where-Object { $_.Kind -eq 'test' }).Count -gt 0
    $hasSymbol = @($tokens | Where-Object { $_.Kind -eq 'symbol' -or $_.Kind -eq 'path' }).Count -gt 0
    $ok = if ($row.status -eq $S_OK) { $hasTest -and $hasSymbol } else { $hasTest -or $hasSymbol }
    if (-not $ok) {
        $s6Violations += [pscustomobject]@{ check = 'S6'; file = $row.file; unit = $row.unit; anchor = (Format-AnchorShort $row.anchor $row.refs); key = 'evidence'; message = ('{0}:{1} [{2}] {3} 行证据缺少{4}' -f $row.file, $row.line, $row.unit, $row.status, $(if ($row.status -eq $S_OK) { '可解析测试名或符号' } else { '可解析符号/测试名' })) }
    }
}
$applied = Apply-Baseline -Check 'S6' -Violations $s6Violations
$s6Status = if (@($applied.Unmatched).Count -gt 0) { 'FAIL' } else { 'PASS' }
$s6Details = @()
$s6Details += Format-Violations $applied.Unmatched $DetailLimit
$s6Details += $applied.MatchedDetails
$s6Summary = ('✅/🟡 行 {0}｜未登记违规 {1}｜基线 {2}' -f $s6Rows, @($applied.Unmatched).Count, $applied.Matched)
Add-Check -Id 'S6' -Name '矩阵 → 实现回执' -Status $s6Status -Summary $s6Summary -Details $s6Details -Baselined $applied.Matched
$script:baselined += $applied.Matched

# ---------------- S7 证据 token 命中 ----------------
$s7Violations = @()
$s7Tokens = 0
foreach ($row in $matrixRows) {
    if ($row.status -ne $S_OK -and $row.status -ne $S_MID) { continue }
    foreach ($tok in (Get-CodeTokens -Text $row.evidence -IsMatrix $true)) {
        $s7Tokens++
        if (-not $tok.Ok) {
            $s7Violations += [pscustomobject]@{ check = 'S7'; file = $row.file; unit = $row.unit; anchor = ''; key = $tok.Name; message = ('{0}:{1} [{2}] 证据 token 未命中：{3}' -f $row.file, $row.line, $row.unit, $tok.Name) }
        }
    }
}
foreach ($it in $roadmapItems) {
    foreach ($tok in (Get-CodeTokens -Text $it.text -IsMatrix $false)) {
        $s7Tokens++
        if (-not $tok.Ok) {
            $s7Violations += [pscustomobject]@{ check = 'S7'; file = $it.file; unit = $it.unit; anchor = ''; key = $tok.Name; message = ('{0}:{1} [{2}] 证据 token 未命中：{3}' -f $it.file, $it.line, $it.unit, $tok.Name) }
        }
    }
}
$applied = Apply-Baseline -Check 'S7' -Violations $s7Violations
$s7Status = if (@($applied.Unmatched).Count -gt 0) { 'FAIL' } else { 'PASS' }
$s7Details = @()
$s7Details += Format-Violations $applied.Unmatched $DetailLimit
$s7Details += $applied.MatchedDetails
$s7Summary = ('证据 token {0}｜未登记违规 {1}｜基线 {2}' -f $s7Tokens, @($applied.Unmatched).Count, $applied.Matched)
Add-Check -Id 'S7' -Name '证据 token 命中' -Status $s7Status -Summary $s7Summary -Details $s7Details -Baselined $applied.Matched
$script:baselined += $applied.Matched

# ---------------- S8 测试执行 ----------------
$s8Status = 'SKIP'
$s8Summary = '未提供 -TestResults；日常检查不执行测试'
$s8Details = @()
if ($TestResults) {
    if (-not (Test-Path -LiteralPath $TestResults)) {
        $s8Status = 'FAIL'
        $s8Summary = '测试结果文件不存在'
        $s8Details += $TestResults
    }
    else {
        $passed = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::Ordinal)
        $failed = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::Ordinal)
        foreach ($line in [System.IO.File]::ReadAllLines($TestResults, [System.Text.Encoding]::UTF8)) {
            if ($line -match '^test\s+(\S+)\s+\.\.\.\s+ok') { [void]$passed.Add($Matches[1]) }
            elseif ($line -match '^test\s+(\S+)\s+\.\.\.\s+FAILED') { [void]$failed.Add($Matches[1]) }
        }
        $s8Violations = @()
        $claim = 0
        foreach ($row in $matrixRows) {
            if ($row.status -ne $S_OK) { continue }
            $claimed = @(Get-CodeTokens -Text $row.evidence -IsMatrix $true | Where-Object { $_.Kind -eq 'test' })
            foreach ($t in $claimed) {
                $claim++
                $hit = @($passed | Where-Object { $_ -eq $t.Name -or $_.EndsWith('::' + $t.Name) }).Count -gt 0
                if (-not $hit) {
                    $s8Violations += ('{0}:{1} [{2}] 测试未通过或未运行：{3}' -f $row.file, $row.line, $row.unit, $t.Name)
                }
            }
        }
        if ($s8Violations.Count -gt 0) { $s8Status = 'FAIL'; $s8Details = @($s8Violations | Select-Object -First $DetailLimit) }
        else { $s8Status = 'PASS' }
        $s8Summary = ('✅ 行声明测试 {0}｜未通过 {1}' -f $claim, $s8Violations.Count)
    }
}
Add-Check -Id 'S8' -Name '测试执行' -Status $s8Status -Summary $s8Summary -Details $s8Details

# ---------------- 失效基线 ----------------
$stale = @()
foreach ($k in $baselineMap.Keys) {
    if (-not $baselineMap[$k]) { $stale += $k }
}
if ($stale.Count -gt 0) {
    $ids = @($stale | ForEach-Object { ($_ -split '\|')[0] } | Select-Object -Unique)
    foreach ($id in $ids) {
        $target = @($script:checks | Where-Object { $_.id -eq $id })
        if ($target.Count -gt 0) {
            $target[0].status = 'FAIL'
            $target[0].details = @($target[0].details) + @($stale | Where-Object { $_ -like ($id + '|*') } | Select-Object -First $DetailLimit | ForEach-Object { '[失效基线] ' + $_ })
            $target[0].summary = $target[0].summary + ('｜失效基线 ' + @($stale | Where-Object { $_ -like ($id + '|*') }).Count)
        }
        else {
            $s1 = @($script:checks | Where-Object { $_.id -eq 'S1' })
            if ($s1.Count -gt 0) {
                $s1[0].status = 'FAIL'
                $s1[0].details = @($s1[0].details) + @($stale | Select-Object -First $DetailLimit | ForEach-Object { '[失效基线] ' + $_ })
            }
        }
    }
}

# ---------------- 自测（-RunTests） ----------------
$selftestResult = $null
if ($RunTests) {
    $selftestScript = Join-Path $PSScriptRoot 'spec_check.selftest.ps1'
    if (-not (Test-Path -LiteralPath $selftestScript)) {
        $selftestResult = [pscustomobject]@{ status = 'FAIL'; summary = '缺少 scripts/spec_check.selftest.ps1'; details = @() }
    }
    else {
        $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $selftestScript -Root $Root 2>&1
        $code = $LASTEXITCODE
        $selftestResult = [pscustomobject]@{
            status  = if ($code -eq 0) { 'PASS' } else { 'FAIL' }
            summary = if ($code -eq 0) { '注入式自测全部通过' } else { '注入式自测存在失败用例' }
            details = @($out | ForEach-Object { [string]$_ } | Select-Object -Last $DetailLimit)
        }
    }
}

# ---------------- 输出 ----------------
$failed = @($script:checks | Where-Object { $_.status -eq 'FAIL' }).Count
$skipped = @($script:checks | Where-Object { $_.status -eq 'SKIP' }).Count

if ($EmitBaseline) {
    # 生成待登记基线条目草稿：只列未登记违规，reason 由人工补写
    $today = (Get-Date).ToString('yyyy-MM-dd')
    foreach ($v in $script:unmatchedViolations) {
        Write-Output '[[violation]]'
        Write-Output ('check = "{0}"' -f $v.check)
        Write-Output ('file = "{0}"' -f $v.file)
        Write-Output ('unit = "{0}"' -f $v.unit)
        Write-Output ('anchor = "{0}"' -f $v.anchor)
        Write-Output ('key = "{0}"' -f $v.key)
        Write-Output 'reason = ""'
        Write-Output ('since = "{0}"' -f $today)
        Write-Output ''
    }
    exit 0
}

if ($Json) {
    $payload = [ordered]@{
        root     = $Root
        strict   = [bool]$Strict
        summary  = [ordered]@{
            total         = $script:checks.Count
            passed        = @($script:checks | Where-Object { $_.status -eq 'PASS' }).Count
            failed        = $failed
            skipped       = $skipped
            baselined     = $script:baselined
            staleBaseline = $stale.Count
        }
        checks   = $script:checks
        nonSpec  = [ordered]@{
            limit = $NON_SPEC_LIMIT
            count = $nonSpecItems.Count
            items = @($nonSpecItems)
        }
        selftest = $selftestResult
    }
    $payload | ConvertTo-Json -Depth 8
}
else {
    foreach ($c in $script:checks) {
        Write-Host ('{0} {1} {2}：{3}' -f $c.id, $c.status, $c.name, $c.summary)
        foreach ($d in $c.details) { Write-Host ('    - ' + $d) }
    }
    if ($selftestResult) { Write-Host ('自测 {0}：{1}' -f $selftestResult.status, $selftestResult.summary) }
    Write-Host ('合计：{0} 项检查，PASS {1}，FAIL {2}，SKIP {3}，基线 {4}' -f $script:checks.Count, ($script:checks.Count - $failed - $skipped), $failed, $skipped, $script:baselined)
}

if ($selftestResult -and $selftestResult.status -eq 'FAIL') { exit 1 }
if ($Strict -and $failed -gt 0) { exit 1 }
exit 0
