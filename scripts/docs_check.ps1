<#
.SYNOPSIS
  docs 目录 C1–C6 一致性检查（只读，不改任何文件）。

.DESCRIPTION
  自动探测文档布局：
    新结构：docs/matrix/README.md + docs/roadmap/README.md
    旧结构：docs/FEATURE_MATRIX.md + docs/ROADMAP.md
  C1 四态计数对账（逐章 + 总计 + 四态合并规则）
  C2 证据路径可定位（✅/🟡 行的证据列）
  C3 阶段复选框与阶段总览对账
  C4 Markdown 相对链接完整性
  C5 易漂移数字与阶段标题状态（非 changelog/归档文件）
  C6 单文件体量（默认 600 行 / 40 KiB）

.PARAMETER Root
  仓库根目录，默认取脚本所在目录的父目录。

.PARAMETER Strict
  任一检查 FAIL 时退出码 1；迁移完成后作为门禁使用。

.PARAMETER Json
  以 JSON 输出，便于迁移前后逐项对比。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/docs_check.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/docs_check.ps1 -Strict
#>
[CmdletBinding()]
param(
    [string]$Root = '',
    [switch]$Strict,
    [switch]$Json,
    [int]$MaxLines = 600,
    [int]$MaxKiB = 40,
    [int]$DetailLimit = 10
)

$ErrorActionPreference = 'Stop'

# 状态符号用码位构造，避免脚本依赖源文件编码
$S_OK   = [string][char]0x2705
$S_MID  = [string]([char]0xD83D) + [string]([char]0xDFE1)
$S_OPEN = [string][char]0x2B1C
$S_NO   = [string][char]0x274C
$S_DASH = [string][char]0x2014
$S_WIP  = [string]([char]0xD83D) + [string]([char]0xDEA7)
$STAGE_WORD = [string][char]0x9636 + [string][char]0x6BB5
$STATUS_SYMBOLS = @($S_OK, $S_MID, $S_OPEN, $S_NO, $S_DASH)

if (-not $Root) { $Root = Split-Path -Parent $PSScriptRoot }
$Root = (Resolve-Path -LiteralPath $Root).Path

$script:checks = @()

function Add-Check {
    param(
        [string]$Id,
        [string]$Name,
        [string]$Status,
        [string]$Summary,
        [string[]]$Details = @()
    )
    $script:checks += [pscustomobject]@{
        id      = $Id
        name    = $Name
        status  = $Status
        summary = $Summary
        details = $Details
    }
}

function Read-Utf8Lines {
    param([string]$Path)
    $text = [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8)
    return ,($text -split "`n")
}

function Get-RelPath {
    param([string]$Path)
    $full = (Resolve-Path -LiteralPath $Path).Path
    if ($full.StartsWith($Root)) {
        return $full.Substring($Root.Length).TrimStart('\', '/').Replace('\', '/')
    }
    return $full.Replace('\', '/')
}

function Split-Cells {
    param([string]$Line)
    $cells = $Line.Split('|') | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne '' }
    return ,@($cells)
}

# 解析矩阵数据行；章节号由文件所在目录给定，旧结构则由 "## 第 N 章" 标题推进
function Parse-MatrixRows {
    param(
        [string[]]$Lines,
        [string]$Source,
        [int]$FixedChapter = -1
    )

    $rows = @()
    $chapter = $FixedChapter
    for ($i = 0; $i -lt $Lines.Length; $i++) {
        $line = $Lines[$i]

        if ($FixedChapter -lt 0 -and $line -match '^##\s+\u7B2C\s*([0-9]+)\s*\u7AE0') {
            $chapter = [int]$Matches[1]
            continue
        }
        if (-not $line.StartsWith('|')) { continue }

        $cells = Split-Cells $line
        if ($cells.Count -lt 6) { continue }
        if ($cells[0] -match '^\u7AE0\u8282$|^\u6CE8\u89E3\u7C7B\u578B$') { continue }
        $sepOnly = $true
        foreach ($c in $cells) { if ($c -notmatch '^[-: ]+$') { $sepOnly = $false; break } }
        if ($sepOnly) { continue }
        if ($cells[0] -match '^\u7B2C|\*\*\u603B\u8BA1') { continue }

        if ($cells.Count -ge 8) {
            $status = $cells[3]
            $reader = $cells[4]
            $writer = $cells[5]
            $evidence = $cells[$cells.Count - 1]
        }
        else {
            $status = $cells[2]
            $reader = $cells[3]
            $writer = $cells[4]
            $evidence = $cells[$cells.Count - 1]
        }
        if ($STATUS_SYMBOLS -notcontains $status) { continue }
        if ($chapter -lt 0) { continue }

        $rows += [pscustomobject]@{
            source   = $Source
            line     = $i + 1
            chapter  = $chapter
            key      = $cells[0]
            status   = $status
            reader   = $reader
            writer   = $writer
            evidence = $evidence
        }
    }
    return ,$rows
}

# 解析总览表：返回按章四态计数与总计行
function Read-Overview {
    param([string[]]$Lines)

    $chapters = @{}
    $total = $null
    foreach ($line in $Lines) {
        if (-not $line.StartsWith('|')) { continue }

        if ($line -match '^\|\s*\*\*\u603B\u8BA1\*\*') {
            $nums = [regex]::Matches($line, '[0-9]+') | ForEach-Object { [int]$_.Value }
            if ($nums.Count -ge 6) { $total = @($nums[0..5]) }
            continue
        }
        if ($line -match '^\|\s*\u7B2C\s*([0-9]+)\s*\u7AE0') {
            $ch = [int]$Matches[1]
            $nums = [regex]::Matches($line, '[0-9]+') | ForEach-Object { [int]$_.Value }
            if ($nums.Count -ge 7) {
                $start = $nums.Count - 6
                $chapters[$ch] = @($nums[$start..($nums.Count - 1)])
            }
        }
    }
    return @{ Chapters = $chapters; Total = $total }
}

# ---------------- 布局探测 ----------------
$docsRoot = Join-Path $Root 'docs'
$newLayout = (Test-Path -LiteralPath (Join-Path $docsRoot 'matrix/README.md')) -and
             (Test-Path -LiteralPath (Join-Path $docsRoot 'roadmap/README.md'))
$layout = if ($newLayout) { 'new' } else { 'legacy' }

$matrixRows = @()
$overviewLines = @()
$overviewSource = ''

if ($newLayout) {
    $overviewSource = Join-Path $docsRoot 'matrix/README.md'
    $overviewLines = Read-Utf8Lines $overviewSource
    $chapterDirs = Get-ChildItem -LiteralPath (Join-Path $docsRoot 'matrix') -Directory -Filter 'ch*'
    foreach ($dir in $chapterDirs) {
        $ch = -1
        if ($dir.Name -match '^ch([0-9]+)$') { $ch = [int]$Matches[1] }
        $files = Get-ChildItem -LiteralPath $dir.FullName -Recurse -File -Filter '*.md'
        foreach ($f in $files) {
            $rows = Parse-MatrixRows -Lines (Read-Utf8Lines $f.FullName) -Source $f.FullName -FixedChapter $ch
            $matrixRows += $rows
        }
    }
}
else {
    $overviewSource = Join-Path $docsRoot 'FEATURE_MATRIX.md'
    if (-not (Test-Path -LiteralPath $overviewSource)) {
        Write-Error "未找到 docs/FEATURE_MATRIX.md，也未发现 docs/matrix/README.md"
    }
    $overviewLines = Read-Utf8Lines $overviewSource
    $matrixRows = Parse-MatrixRows -Lines $overviewLines -Source $overviewSource -FixedChapter -1
}

# ---------------- C1 四态计数对账 ----------------
$overview = Read-Overview $overviewLines
$actual = @{}
$mergeViolations = @()

foreach ($row in $matrixRows) {
    $ch = $row.chapter
    if (-not $actual.ContainsKey($ch)) {
        $actual[$ch] = @{ ok = 0; mid = 0; open = 0; no = 0; dash = 0; total = 0 }
    }
    switch ($row.status) {
        $S_OK   { $actual[$ch].ok++ }
        $S_MID  { $actual[$ch].mid++ }
        $S_OPEN { $actual[$ch].open++ }
        $S_NO   { $actual[$ch].no++ }
        $S_DASH { $actual[$ch].dash++ }
    }
    $actual[$ch].total++

    # 四态合并规则
    $r = $row.reader
    $w = $row.writer
    if ($row.status -eq $S_OK) {
        if ($r -eq $S_OPEN) {
            $mergeViolations += ('{0}:{1} [{2}] Reader ⬜ 却整行 ✅' -f (Get-RelPath $row.source), $row.line, $row.key)
        }
        if ($r -notin @($S_OK, $S_DASH) -and $w -notin @($S_OK, $S_DASH)) {
            $mergeViolations += ('{0}:{1} [{2}] 整行 ✅ 要求 Reader/Writer 均 ✅ 或 —' -f (Get-RelPath $row.source), $row.line, $row.key)
        }
    }
    elseif ($row.status -eq $S_NO) {
        if ($r -notin @($S_NO, $S_DASH) -or $w -notin @($S_NO, $S_DASH)) {
            $mergeViolations += ('{0}:{1} [{2}] 整行 ❌ 要求所有适用侧均 ❌' -f (Get-RelPath $row.source), $row.line, $row.key)
        }
    }
    $sides = @($r, $w) | Where-Object { $_ -notin @($S_DASH, '') }
    if (($sides | Select-Object -Unique).Count -gt 1 -and $row.status -eq $S_OK) {
        $mergeViolations += ('{0}:{1} [{2}] 两侧混合 ({3}/{4}) 至多 🟡' -f (Get-RelPath $row.source), $row.line, $row.key, $r, $w)
    }
}

$c1Details = @()
$mismatchCount = 0
$allChapters = @($overview.Chapters.Keys + $actual.Keys | Sort-Object -Unique)
foreach ($ch in $allChapters) {
    $doc = $overview.Chapters[$ch]
    $act = $actual[$ch]
    if (-not $act) { $act = @{ ok = 0; mid = 0; open = 0; no = 0; dash = 0; total = 0 } }
    $docArr = if ($doc) { $doc } else { @(0, 0, 0, 0, 0, 0) }
    $actArr = @($act.ok, $act.mid, $act.open, $act.no, $act.dash, $act.total)
    $diff = @()
    for ($k = 0; $k -lt 6; $k++) { if ($docArr[$k] -ne $actArr[$k]) { $diff += ('第 {0} 章第 {1} 列 文档 {2} / 实际 {3}' -f $ch, ($k + 1), $docArr[$k], $actArr[$k]) } }
    if ($diff.Count -gt 0) {
        $mismatchCount += $diff.Count
        $c1Details += $diff
    }
}
$sum = @{ ok = 0; mid = 0; open = 0; no = 0; dash = 0; total = 0 }
foreach ($a in $actual.Values) {
    $sum.ok += $a.ok; $sum.mid += $a.mid; $sum.open += $a.open; $sum.no += $a.no; $sum.dash += $a.dash; $sum.total += $a.total
}
$sumArr = @($sum.ok, $sum.mid, $sum.open, $sum.no, $sum.dash, $sum.total)
if ($overview.Total) {
    $docTotal = @($overview.Total)
    for ($k = 0; $k -lt 6; $k++) {
        if ($docTotal[$k] -ne $sumArr[$k]) {
            $mismatchCount++
            $c1Details += ('总计第 {0} 列 文档 {1} / 实际 {2}' -f ($k + 1), $docTotal[$k], $sumArr[$k])
        }
    }
}
else {
    $mismatchCount++
    $c1Details += '总览缺少「总计」行，无法对账'
}
if ($mergeViolations.Count -gt 0) {
    $c1Details += $mergeViolations
}
$c1Status = if ($mismatchCount -gt 0 -or $mergeViolations.Count -gt 0) { 'FAIL' } else { 'PASS' }
$c1Summary = ('{0} 行｜✅{1} 🟡{2} ⬜{3} ❌{4} —{5}｜计数不一致 {6} 处｜合并规则违规 {7} 处' -f `
        $sum.total, $sum.ok, $sum.mid, $sum.open, $sum.no, $sum.dash, $mismatchCount, $mergeViolations.Count)
Add-Check -Id 'C1' -Name '四态计数对账' -Status $c1Status -Summary $c1Summary -Details ($c1Details | Select-Object -First $DetailLimit)

# ---------------- C2 证据路径 ----------------
$rsIndex = @{}
$rsAll = @()
$rsCount = 0
foreach ($baseDir in @('src', 'tests')) {
    $dir = Join-Path $Root $baseDir
    if (-not (Test-Path -LiteralPath $dir)) { continue }
    $files = Get-ChildItem -LiteralPath $dir -Recurse -File -Filter '*.rs'
    foreach ($f in $files) {
        $rel = (Get-RelPath $f.FullName)
        $rsCount++
        $rsAll += $rel
        $rsIndex[$rel] = $true
        $parts = $rel.Split('/')
        for ($i = 1; $i -lt $parts.Length; $i++) {
            $suffix = ($parts[$i..($parts.Length - 1)] -join '/')
            $rsIndex[$suffix] = $true
        }
    }
}

$c2Details = @()
$evidenceTokens = 0
$missingTokens = 0
foreach ($row in $matrixRows) {
    if ($row.status -notin @($S_OK, $S_MID)) { continue }
    if ([string]::IsNullOrWhiteSpace($row.evidence)) { continue }

    $tokens = [regex]::Matches($row.evidence, '[A-Za-z0-9_][A-Za-z0-9_./-]*\.rs') | ForEach-Object { $_.Value }
    foreach ($token in $tokens) {
        $evidenceTokens++
        $norm = $token.Trim('/').Replace('\', '/')
        $exists = $false
        if ($norm -match '^(src|tests)/') {
            $exists = Test-Path -LiteralPath (Join-Path $Root $norm.Replace('/', '\'))
        }
        else {
            $exists = $rsIndex.ContainsKey($norm)
        }
        # 目录模块兜底：fdf.rs 实际为 fdf/ 目录
        if (-not $exists) {
            $stem = $norm -replace '\.rs$', ''
            $exists = @($rsAll | Where-Object { $_ -like "$stem/*" -or $_ -like "*/$stem/*" }).Count -gt 0
        }
        if (-not $exists) {
            $missingTokens++
            $c2Details += ('{0}:{1} [{2}] 证据路径不可定位：{3}' -f (Get-RelPath $row.source), $row.line, $row.key, $token)
        }
    }
}
$c2Status = if ($missingTokens -gt 0) { 'FAIL' } else { 'PASS' }
$c2Summary = ('证据路径 {0} 处｜缺失 {1} 处｜已索引 Rust 文件 {2} 个' -f $evidenceTokens, $missingTokens, $rsCount)
Add-Check -Id 'C2' -Name '证据路径可定位' -Status $c2Status -Summary $c2Summary -Details ($c2Details | Select-Object -First $DetailLimit)

# ---------------- C3 阶段复选框对账 ----------------
$stageActual = @{}
$stageDoc = @{}

if ($newLayout) {
    $stageDir = Join-Path $docsRoot 'roadmap/stages'
    if (Test-Path -LiteralPath $stageDir) {
        $files = Get-ChildItem -LiteralPath $stageDir -File -Filter '*.md'
        foreach ($f in $files) {
            if ($f.Name -notmatch '^([a-f])-') { continue }
            $key = $Matches[1].ToUpper()
            $done = 0; $todo = 0
            foreach ($line in (Read-Utf8Lines $f.FullName)) {
                if ($line -match '^\s*-\s*\[[xX]\]\s*(?:\*\*)?[A-F][0-9]+(?:\s|\*\*)') { $done++ }
                elseif ($line -match '^\s*-\s*\[\s\]\s*(?:\*\*)?[A-F][0-9]+(?:\s|\*\*)') { $todo++ }
            }
            $stageActual[$key] = @{ done = $done; total = $done + $todo }
        }
    }
    $stageDocSource = Join-Path $docsRoot 'roadmap/README.md'
    $stageLines = Read-Utf8Lines $stageDocSource
}
else {
    $stageDocSource = Join-Path $docsRoot 'ROADMAP.md'
    $stageLines = Read-Utf8Lines $stageDocSource
    $current = ''
    foreach ($line in $stageLines) {
        if ($line -match '^##\s+\u9636\u6BB5\s+([A-F])') {
            $current = $Matches[1]
            if (-not $stageActual.ContainsKey($current)) { $stageActual[$current] = @{ done = 0; total = 0 } }
            continue
        }
        if ($line -match '^##\s') { $current = ''; continue }
        if (-not $current) { continue }
        if ($line -match '^\s*-\s*\[[xX]\]\s*(?:\*\*)?[A-F][0-9]+(?:\s|\*\*)') { $stageActual[$current].done++; $stageActual[$current].total++ }
        elseif ($line -match '^\s*-\s*\[\s\]\s*(?:\*\*)?[A-F][0-9]+(?:\s|\*\*)') { $stageActual[$current].total++ }
    }
}

foreach ($line in $stageLines) {
    if ($line -notmatch '^\|\s*([A-F])\s') { continue }
    $key = $Matches[1]
    $cells = Split-Cells $line
    if ($cells.Count -lt 5) { continue }
    $inc = 0; $done = 0
    if ($cells[2] -match '([0-9]+)') { $inc = [int]$Matches[1] }
    if ($cells[3] -match '^([0-9]+)') { $done = [int]$Matches[1] }
    $stageDoc[$key] = @{ done = $done; total = $inc }
}

$c3Details = @()
$c3Bad = 0
$stageKeys = @($stageDoc.Keys + $stageActual.Keys | Sort-Object -Unique)
foreach ($key in $stageKeys) {
    $doc = $stageDoc[$key]
    $act = $stageActual[$key]
    if (-not $doc) { $doc = @{ done = -1; total = -1 } }
    if (-not $act) { $act = @{ done = 0; total = 0 } }
    if ($doc.done -ne $act.done -or $doc.total -ne $act.total) {
        $c3Bad++
        $c3Details += ('阶段 {0}：总览 已完成 {1}/{2}，复选框 已完成 {3}/{4}' -f $key, $doc.done, $doc.total, $act.done, $act.total)
    }
}
$c3Status = if ($c3Bad -gt 0) { 'FAIL' } else { 'PASS' }
$c3Summary = ('阶段 {0} 个｜总览与复选框不一致 {1} 个' -f $stageKeys.Count, $c3Bad)
Add-Check -Id 'C3' -Name '阶段复选框对账' -Status $c3Status -Summary $c3Summary -Details ($c3Details | Select-Object -First $DetailLimit)

# ---------------- C4 链接完整性 ----------------
$mdFiles = @()
if (Test-Path -LiteralPath (Join-Path $Root 'docs')) {
    $mdFiles += Get-ChildItem -LiteralPath (Join-Path $Root 'docs') -Recurse -File -Filter '*.md'
}
if (Test-Path -LiteralPath (Join-Path $Root '.github')) {
    $mdFiles += Get-ChildItem -LiteralPath (Join-Path $Root '.github') -Recurse -File -Filter '*.md'
}
if (Test-Path -LiteralPath (Join-Path $Root 'README.md')) {
    $mdFiles += Get-Item -LiteralPath (Join-Path $Root 'README.md')
}
$mdFiles = $mdFiles | Where-Object {
    $rel = (Get-RelPath $_.FullName)
    $rel -notmatch '(^|/)(archive|target|\.git|\.trae)/'
}

$c4Broken = @()
$linkTotal = 0
foreach ($file in $mdFiles) {
    $rel = (Get-RelPath $file.FullName)
    $lines = Read-Utf8Lines $file.FullName
    for ($i = 0; $i -lt $lines.Length; $i++) {
        foreach ($m in [regex]::Matches($lines[$i], '\]\(([^)]+)\)')) {
            $target = $m.Groups[1].Value.Trim()
            if ($target.StartsWith('<') -and $target.EndsWith('>')) {
                $target = $target.Substring(1, $target.Length - 2)
            }
            $target = ($target -split '#')[0].Trim()
            if (-not $target) { continue }
            if ($target -match '^[A-Za-z][A-Za-z0-9+.-]*:') { continue }
            $linkTotal++
            $candidate = Join-Path (Split-Path -Parent $file.FullName) $target
            if (-not (Test-Path -LiteralPath $candidate)) {
                $c4Broken += ('{0}:{1} -> {2}' -f $rel, ($i + 1), $target)
            }
        }
    }
}
$c4Status = if ($c4Broken.Count -gt 0) { 'FAIL' } else { 'PASS' }
$c4Summary = ('相对链接 {0} 条｜断链 {1} 条｜扫描 {2} 个 md' -f $linkTotal, $c4Broken.Count, $mdFiles.Count)
Add-Check -Id 'C4' -Name '链接完整性' -Status $c4Status -Summary $c4Summary -Details ($c4Broken | Select-Object -First $DetailLimit)

# ---------------- C5 易漂移数字 / 阶段标题状态 ----------------
$scopeFiles = @()
if ($newLayout) {
    if (Test-Path -LiteralPath (Join-Path $docsRoot 'README.md')) {
        $scopeFiles += Get-Item -LiteralPath (Join-Path $docsRoot 'README.md')
    }
    foreach ($sub in @('matrix', 'roadmap')) {
        $dir = Join-Path $docsRoot $sub
        if (Test-Path -LiteralPath $dir) {
            $scopeFiles += Get-ChildItem -LiteralPath $dir -Recurse -File -Filter '*.md'
        }
    }
    # 白名单：这些文件本来就承载数字
    $scopeFiles = $scopeFiles | Where-Object { (Get-RelPath $_.FullName) -notmatch '(^|/)(module-debt.*|fixtures|principles)\.md$' }
}
else {
    $scopeFiles += Get-Item -LiteralPath (Join-Path $docsRoot 'FEATURE_MATRIX.md')
    $scopeFiles += Get-Item -LiteralPath (Join-Path $docsRoot 'ROADMAP.md')
}
$scopeFiles = $scopeFiles | Where-Object { $_.FullName -match '\.md$' -and (Get-RelPath $_.FullName) -notmatch '(^|/)(archive|changelog)/' }

$patterns = @(
    @{ name = '测试计数'; rx = '[0-9]+\s*(?:\u4E2A|\u6761|\u9879)\s*(?:\u6D4B\u8BD5|\u7528\u4F8B)|[0-9]+\s+tests?\b|[0-9]+\s+test\s+cases' },
    @{ name = '行数';     rx = '\u884C\u6570\s*[0-9]+' },
    @{ name = '体积';     rx = '[0-9]+\s*\u884C\s*/\s*[0-9]+(?:\.[0-9]+)?\s*(?:KiB|KB|MiB|MB)' }
)

$c5Details = @()
$c5Hits = 0
$c5HeadingHits = 0
$c5ByKind = @{}
foreach ($file in $scopeFiles) {
    $rel = (Get-RelPath $file.FullName)
    $lines = Read-Utf8Lines $file.FullName
    for ($i = 0; $i -lt $lines.Length; $i++) {
        $line = $lines[$i]
        foreach ($p in $patterns) {
            if ($line -match $p.rx) {
                $c5Hits++
                if (-not $c5ByKind.ContainsKey($p.name)) { $c5ByKind[$p.name] = 0 }
                $c5ByKind[$p.name]++
                if ($c5Details.Count -lt $DetailLimit) {
                    $c5Details += ('{0}:{1} [{2}] {3}' -f $rel, ($i + 1), $p.name, $line.Trim().Substring(0, [Math]::Min(60, $line.Trim().Length)))
                }
            }
        }
        if ($line -match '^#{2,4}\s' -and $line.Contains($STAGE_WORD) -and (
                $line.Contains($S_OK) -or $line.Contains($S_MID) -or $line.Contains($S_WIP) -or
                $line.Contains($S_OPEN) -or $line.Contains($S_NO) -or
                $line -match '\([0-9]+/[0-9]+\)')) {
            $c5HeadingHits++
            if ($c5Details.Count -lt $DetailLimit) {
                $c5Details += ('{0}:{1} [标题状态] {2}' -f $rel, ($i + 1), $line.Trim())
            }
        }
    }
}
$c5Status = if ($c5Hits -gt 0 -or $c5HeadingHits -gt 0) { 'FAIL' } else { 'PASS' }
$c5Summary = ('易漂移数字 {0} 处｜阶段标题带状态 {1} 处｜扫描 {2} 个文件' -f $c5Hits, $c5HeadingHits, $scopeFiles.Count)
Add-Check -Id 'C5' -Name '易漂移数字与标题状态' -Status $c5Status -Summary $c5Summary -Details $c5Details

# ---------------- C6 单文件体量 ----------------
$c6Details = @()
$c6Bad = 0
foreach ($file in $scopeFiles) {
    $rel = (Get-RelPath $file.FullName)
    $byteCount = (Get-Item -LiteralPath $file.FullName).Length
    $lineCount = (Read-Utf8Lines $file.FullName).Count
    $kib = [Math]::Round($byteCount / 1024.0, 1)
    if ($lineCount -gt $MaxLines -or $kib -gt $MaxKiB) {
        $c6Bad++
        $c6Details += ('{0}：{1} 行 / {2} KiB（上限 {3} 行 / {4} KiB）' -f $rel, $lineCount, $kib, $MaxLines, $MaxKiB)
    }
}
$c6Status = if ($c6Bad -gt 0) { 'FAIL' } else { 'PASS' }
$c6Summary = ('超限文件 {0} 个｜上限 {1} 行 / {2} KiB｜扫描 {3} 个文件' -f $c6Bad, $MaxLines, $MaxKiB, $scopeFiles.Count)
Add-Check -Id 'C6' -Name '单文件体量' -Status $c6Status -Summary $c6Summary -Details ($c6Details | Select-Object -First $DetailLimit)

# ---------------- 输出 ----------------
$failed = @($script:checks | Where-Object { $_.status -eq 'FAIL' }).Count

if ($Json) {
    $payload = [ordered]@{
        root    = $Root
        layout  = $layout
        strict  = [bool]$Strict
        summary = [ordered]@{
            total  = $script:checks.Count
            passed = @($script:checks | Where-Object { $_.status -eq 'PASS' }).Count
            failed = $failed
        }
        checks  = $script:checks
    }
    $payload | ConvertTo-Json -Depth 6
}
else {
    Write-Host ('布局：{0}（{1}）' -f $layout, (Get-RelPath $overviewSource))
    foreach ($c in $script:checks) {
        $mark = if ($c.status -eq 'PASS') { 'PASS' } else { 'FAIL' }
        Write-Host ('{0} {1} {2}：{3}' -f $c.id, $mark, $c.name, $c.summary)
        foreach ($d in $c.details) { Write-Host ('    - ' + $d) }
    }
    Write-Host ('合计：{0} 项检查，PASS {1}，FAIL {2}' -f $script:checks.Count, ($script:checks.Count - $failed), $failed)
}

if ($Strict -and $failed -gt 0) { exit 1 }
exit 0
