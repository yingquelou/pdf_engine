# 规范索引共享库：标题/表格解析、锚点归一化、键名窗口检索。
# 被 scripts/spec_index.ps1 与 scripts/spec_check.ps1 共同引用，保证索引与检查使用同一套解析规则。

$script:SpecRangeCache = @{}

function Get-Sha256Hex {
    param([string]$Path)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $stream = [System.IO.File]::OpenRead($Path)
        try {
            $bytes = $sha.ComputeHash($stream)
        }
        finally { $stream.Dispose() }
    }
    finally { $sha.Dispose() }
    return ([System.BitConverter]::ToString($bytes) -replace '-', '').ToLowerInvariant()
}

# 文本哈希按 UTF-8 无 BOM 的字节计算，避免平台换行差异
function Get-Sha256OfText {
    param([string]$Text)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
        return ([System.BitConverter]::ToString($sha.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant()
    }
    finally { $sha.Dispose() }
}

function Test-TocLine {
    param([string]$Line)
    # 注意：不要写成 (\s*\.\s*){5,}，该模式在长行上会灾难性回溯（实测 26s/36k 行）
    return ($Line -match '\.{3,}' -or $Line -match '\.\s*\.\s*\.\s*\.\s*\.')
}

function Get-SpecModel {
    param([string[]]$Lines)

    $headings = New-Object System.Collections.ArrayList
    for ($i = 0; $i -lt $Lines.Length; $i++) {
        $line = $Lines[$i]
        if (Test-TocLine $line) { continue }

        # 数字标题：允许编号与标题之间无空格（抽取伪影，如 14.11Prepress Support）
        if ($line -match '^\s*(\d{1,2}(?:\.\d{1,3}){0,3})\s*([A-Z0-9][^\r\n]*)$') {
            $id = $Matches[1]
            $title = $Matches[2].Trim()
            if ($title.Length -gt 80) { continue }
            if ($title -match '\s{2,}') { continue }
            if ($title -match '[.,;:]$') { continue }
            if ($id -notmatch '\.') {
                # 章标题不参与窗口边界；章级锚点由前缀展开处理
                continue
            }
            if ($title -match '^\d' -and $id -notmatch '\.') { continue }
            [void]$headings.Add([pscustomobject]@{ idx = $i; id = $id; title = $title; depth = ($id.Split('.').Count); kind = 'section' })
            continue
        }

        # 附录标题；正文中为 "Annex  F"，目录行由点引导过滤
        if ($line -match '^\s*Annex\s+([A-Z])(?:\s+(.*))?$') {
            $letter = $Matches[1]
            $title = if ($Matches.Count -gt 2) { $Matches[2].Trim() } else { '' }
            [void]$headings.Add([pscustomobject]@{ idx = $i; id = ('Annex ' + $letter); title = $title; depth = 2; kind = 'annex' })
            continue
        }
    }

    # 按深度计算窗口：从标题到下一个同级或更浅标题（单调栈，O(n)）
    $byId = @{}
    $ordered = @($headings | Sort-Object idx)
    $stack = New-Object System.Collections.ArrayList
    $seen = @{}
    foreach ($h in $ordered) {
        if ($seen.ContainsKey($h.id)) { continue }
        $seen[$h.id] = $true
        $tree = ($h.id -split '\.')[0]
        if ($h.kind -eq 'annex') {
            while ($stack.Count -gt 0) {
                $o = $stack[$stack.Count - 1]
                $stack.RemoveAt($stack.Count - 1)
                if (-not $byId.ContainsKey($o.id)) { $byId[$o.id] = @(, @{ start = $o.idx; end = $h.idx - 1 }) }
            }
        }
        else {
            while ($stack.Count -gt 0) {
                $o = $stack[$stack.Count - 1]
                if ($o.kind -ne 'annex' -and $o.tree -eq $tree -and $o.depth -lt $h.depth) { break }
                $stack.RemoveAt($stack.Count - 1)
                if (-not $byId.ContainsKey($o.id)) { $byId[$o.id] = @(, @{ start = $o.idx; end = $h.idx - 1 }) }
            }
        }
        [void]$stack.Add([pscustomobject]@{ id = $h.id; idx = $h.idx; depth = $h.depth; kind = $h.kind; tree = $tree })
    }
    while ($stack.Count -gt 0) {
        $o = $stack[$stack.Count - 1]
        $stack.RemoveAt($stack.Count - 1)
        if (-not $byId.ContainsKey($o.id)) { $byId[$o.id] = @(, @{ start = $o.idx; end = $Lines.Length - 1 }) }
    }

    # 表格：标题行到下一个标题或下一张表；同表续表合并
    $captions = New-Object System.Collections.ArrayList
    for ($i = 0; $i -lt $Lines.Length; $i++) {
        if ($Lines[$i] -match 'Table\s+(\d+)\s*[\u2013\u2014-]') {
            [void]$captions.Add([pscustomobject]@{ idx = $i; n = [int]$Matches[1] })
        }
    }
    $tables = @{}
    $tableOwner = @{}
    $hPtr = 0
    for ($k = 0; $k -lt $captions.Count; $k++) {
        $c = $captions[$k]
        # $captions 与 $ordered 均按 idx 升序，用单调指针代替每张表全量扫描
        while ($hPtr -lt $ordered.Count -and $ordered[$hPtr].idx -le $c.idx) { $hPtr++ }
        $limit = $Lines.Length - 1
        if ($k + 1 -lt $captions.Count) { $limit = $captions[$k + 1].idx - 1 }
        if ($hPtr -lt $ordered.Count -and ($ordered[$hPtr].idx - 1) -lt $limit) { $limit = $ordered[$hPtr].idx - 1 }
        if (-not $tables.ContainsKey($c.n)) { $tables[$c.n] = @{ start = $c.idx; end = $limit } }
        else { $tables[$c.n].end = [Math]::Max($tables[$c.n].end, $limit) }
        if (-not $tableOwner.ContainsKey($c.n) -and $hPtr -gt 0) { $tableOwner[$c.n] = $ordered[$hPtr - 1].id }
    }

    $sectionIds = @($byId.Keys | Where-Object { $_ -match '^(7|8|9|10|11|12|13|14)(\.|$)' -and $_ -notmatch '^Annex' } | Sort-Object)
    # 预计算数值元组与章索引，避免逐条锚点做字符串拆分与全表扫描
    $tuples = @{}
    $tupleList = New-Object System.Collections.ArrayList
    foreach ($id in $sectionIds) {
        $t = @($id.Split('.') | ForEach-Object { [int]$_ })
        $tuples[$id] = $t
        $k = (@($t | ForEach-Object { $_.ToString('D3') }) -join '.')
        [void]$tupleList.Add([pscustomobject]@{ id = $id; t = $t; k = $k })
    }
    $tupleList = @($tupleList | Sort-Object k)
    $chapterIds = @{}
    foreach ($ch in @('7', '8', '9', '10', '11', '12', '13', '14')) {
        $chapterIds[$ch] = @($sectionIds | Where-Object { $_.StartsWith($ch + '.') })
    }
    return [pscustomobject]@{
        Lines      = $Lines
        Headings   = $ordered
        ById       = $byId
        Tables     = $tables
        TableOwner = $tableOwner
        SectionIds = $sectionIds
        Tuples     = $tuples
        TupleList  = $tupleList
        ChapterIds = $chapterIds
        Cache      = @{}
    }
}

# 纯数值比较，避免逐次拆分字符串
function Compare-SpecTupleFast {
    param($A, $B)
    $n = [Math]::Max($A.Count, $B.Count)
    for ($i = 0; $i -lt $n; $i++) {
        $x = if ($i -lt $A.Count) { $A[$i] } else { 0 }
        $y = if ($i -lt $B.Count) { $B[$i] } else { 0 }
        if ($x -ne $y) { return [Math]::Sign($x - $y) }
    }
    return 0
}

function Compare-SpecTuple {
    param($A, $B)
    $n = [Math]::Max($A.Count, $B.Count)
    for ($i = 0; $i -lt $n; $i++) {
        $x = if ($i -lt $A.Count) { $A[$i] } else { 0 }
        $y = if ($i -lt $B.Count) { $B[$i] } else { 0 }
        if ($x -ne $y) { return [Math]::Sign($x - $y) }
    }
    return 0
}

function ConvertTo-SpecTuple {
    param([string]$Id)
    return , @($Id.Split('.') | ForEach-Object { [int]$_ })
}

function Test-SpecIdInRange {
    param([string]$Id, [string]$From, [string]$To)
    return ((Compare-SpecTuple (ConvertTo-SpecTuple $Id) (ConvertTo-SpecTuple $From)) -ge 0) -and
           ((Compare-SpecTuple (ConvertTo-SpecTuple $Id) (ConvertTo-SpecTuple $To)) -le 0)
}

# 返回锚点引用集合：
#   Explicit   显式写出的锚点（不退化为范围展开）
#   All        显式锚点 + 范围内已存在的小节 + 表所属小节
#   Tables     引用的表号
#   Unresolved 无法在索引中解析的锚点/表
function Get-SpecRefs {
    param([string]$Text, $Model)

    $explicit = New-Object System.Collections.ArrayList
    $all = New-Object System.Collections.ArrayList
    $tables = New-Object System.Collections.ArrayList
    $unresolved = New-Object System.Collections.ArrayList
    $rangeSpans = New-Object System.Collections.ArrayList

    # 范围两端都必须是第 7–14 章的小节号；`0.0–1.0`、`1.0–1.7` 是版本号，不是锚点
    $anchorPart = '(?<![\d.])(?:7|8|9|10|11|12|13|14)\.\d{1,2}(?:\.\d{1,2}){0,3}(?![\d.])'
    foreach ($m in [regex]::Matches($Text, '(' + $anchorPart + ')\s*[\u2013\u2014-]\s*(' + $anchorPart + ')')) {
        $from = $m.Groups[1].Value
        $to = $m.Groups[2].Value
        [void]$rangeSpans.Add(@($m.Index, $m.Index + $m.Length))
        [void]$explicit.Add($from)
        [void]$explicit.Add($to)
        [void]$all.Add($from)
        [void]$all.Add($to)
        if (-not $Model.ById.ContainsKey($from)) { [void]$unresolved.Add($from) }
        if (-not $Model.ById.ContainsKey($to)) { [void]$unresolved.Add($to) }
        $fromT = $Model.Tuples[$from]
        $toT = $Model.Tuples[$to]
        if ($fromT -and $toT) {
            # 元组列表按数值升序，二分定位下界
            $lo = 0
            $hi = $Model.TupleList.Count
            while ($lo -lt $hi) {
                # 用差值取半，避免 PowerShell 的银行家舍入把 mid 舍到 hi 导致死循环
                $mid = $lo + [int][Math]::Floor(($hi - $lo) / 2)
                if ((Compare-SpecTupleFast $Model.TupleList[$mid].t $fromT) -lt 0) { $lo = $mid + 1 } else { $hi = $mid }
            }
            for ($j = $lo; $j -lt $Model.TupleList.Count; $j++) {
                if ((Compare-SpecTupleFast $Model.TupleList[$j].t $toT) -gt 0) { break }
                [void]$all.Add($Model.TupleList[$j].id)
            }
        }
    }

    foreach ($m in [regex]::Matches($Text, $anchorPart)) {
        $inside = $false
        foreach ($s in $rangeSpans) {
            if ($m.Index -ge $s[0] -and $m.Index -lt $s[1]) { $inside = $true; break }
        }
        if ($inside) { continue }
        [void]$explicit.Add($m.Value)
        [void]$all.Add($m.Value)
        if (-not $Model.ById.ContainsKey($m.Value)) { [void]$unresolved.Add($m.Value) }
    }

    foreach ($m in [regex]::Matches($Text, '\u7B2C\s*(\d{1,2})\s*\u7AE0')) {
        $ref = 'Chapter:' + $m.Groups[1].Value
        [void]$explicit.Add($ref)
        [void]$all.Add($ref)
        $ids = $Model.ChapterIds[$m.Groups[1].Value]
        if (-not $ids -or $ids.Count -eq 0) { [void]$unresolved.Add($ref) }
    }

    foreach ($m in [regex]::Matches($Text, '(?:\u9644\u5F55|Annex)\s*([A-Z])')) {
        $ref = 'Annex ' + $m.Groups[1].Value
        [void]$explicit.Add($ref)
        [void]$all.Add($ref)
        if (-not $Model.ById.ContainsKey($ref)) { [void]$unresolved.Add($ref) }
    }

    foreach ($m in [regex]::Matches($Text, 'Table\s+(\d+)')) {
        $n = [int]$m.Groups[1].Value
        [void]$tables.Add($n)
        if (-not $Model.Tables.ContainsKey($n)) {
            [void]$unresolved.Add('Table ' + $n)
            continue
        }
        if ($Model.TableOwner.ContainsKey($n)) {
            [void]$all.Add($Model.TableOwner[$n])
        }
    }

    return [pscustomobject]@{
        Explicit   = @($explicit | Select-Object -Unique)
        All        = @($all | Select-Object -Unique)
        Tables     = @($tables | Select-Object -Unique)
        Unresolved = @($unresolved | Select-Object -Unique)
    }
}

function Get-SpecWindows {
    param($Refs, $Model)

    $ranges = New-Object System.Collections.ArrayList
    foreach ($ref in $Refs.All) {
        if ($Model.ById.ContainsKey($ref)) {
            foreach ($w in $Model.ById[$ref]) { [void]$ranges.Add(@($w.start, $w.end)) }
            continue
        }
        if ($ref -like 'Chapter:*') {
            $ids = $Model.ChapterIds[$ref.Substring(8)]
            if (-not $ids) { continue }
            foreach ($id in $ids) {
                foreach ($w in $Model.ById[$id]) { [void]$ranges.Add(@($w.start, $w.end)) }
            }
        }
    }
    foreach ($n in $Refs.Tables) {
        if ($Model.Tables.ContainsKey($n)) {
            $t = $Model.Tables[$n]
            [void]$ranges.Add(@($t.start, $t.end))
        }
    }
    return , @($ranges)
}

function Get-SpecWindowText {
    param($Model, [int]$Start, [int]$End)
    $key = '{0}-{1}' -f $Start, $End
    if ($Model.Cache.ContainsKey($key)) { return $Model.Cache[$key] }
    $text = ($Model.Lines[$Start..$End] -join "`n")
    $Model.Cache[$key] = $text
    return $text
}

function Test-SpecKeyInWindows {
    param([string]$Key, $Windows, $Model)
    $pattern = '(?<![A-Za-z0-9_])/?' + [regex]::Escape($Key) + '(?![A-Za-z0-9_])'
    foreach ($w in $Windows) {
        # 必须用大小写敏感匹配：PowerShell 的 -match 默认忽略大小写，会把正文里的
        # matrix/width 当成 PDF 键名，放行「凭空发明键」
        if ([regex]::IsMatch((Get-SpecWindowText $Model $w[0] $w[1]), $pattern, [System.Text.RegularExpressions.RegexOptions]::None)) { return $true }
    }
    return $false
}

# 从反引号片段中提取 PDF 键名；只接受以 / 开头且前面是空白或标点的键，
# 避免把 `builder/assemble/` 这类路径片段误判为键。
function Get-SpecKeys {
    param([string]$Text)
    $keys = New-Object System.Collections.ArrayList
    foreach ($m in [regex]::Matches($Text, '`([^`]*)`')) {
        $span = $m.Groups[1].Value
        foreach ($km in [regex]::Matches($span, '(^|[\s(\uFF08,\uFF0C;\uFF1B\u3001])/([A-Za-z][A-Za-z0-9_.-]*)')) {
            [void]$keys.Add('/' + $km.Groups[2].Value)
        }
    }
    return , @($keys | Select-Object -Unique)
}
