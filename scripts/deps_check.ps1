<#
.SYNOPSIS
  依赖治理 D1–D9 检查（只读，不改任何文件）。

.DESCRIPTION
  规则定义见 docs/roadmap/principles.md 工程约定 #1；机器可读事实源为 scripts/deps.allow.toml。
  D1 依赖清单同步（manifest ↔ 登记表 ↔ 特性契约）
  D2 导入边界（第三方 crate 只允许出现在适配器/测试路径；已知债务记为基线）
  D3 公开 API 无外部 crate 类型（离线静态扫描 pub 签名与 re-export，无 nightly 依赖）
  D4 许可证相容（需 cargo-deny + deny.toml）
  D5 安全公告（需 cargo-audit；公告库不可达 → SKIP 并注明已尝试数据源，解析出公告才 FAIL）
  D6 unsafe 用量（需 cargo-geiger；只判 pdf_engine 自身 unsafe，依赖图计数仅信息）
  D7 复核时效与 vendored 溯源（离线，始终运行）
  D8 引入不降级登记（behaviour_delta / degraded 缺失即 FAIL；退化项须登记差异、接受理由与测试）
  D9 依赖 MSRV 不高于本项目（cargo metadata 的 rust_version；普通依赖超标即 FAIL，
     platform-specific 与 dev-dependencies 单列报告、不计 FAIL）
  外部工具缺失时状态为 SKIP 并给出安装命令与实际查找过的路径，不伪造 PASS。

.PARAMETER Root
  仓库根目录，默认取脚本所在目录的父目录。

.PARAMETER Strict
  D1–D9 任一 FAIL 时退出码 1；SKIP 不计入失败。

.PARAMETER Json
  以 JSON 输出，便于门禁与对比。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/deps_check.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/deps_check.ps1 -Strict
#>
[CmdletBinding()]
param(
    [string]$Root = '',
    [switch]$Strict,
    [switch]$Json,
    [int]$DetailLimit = 10
)

$ErrorActionPreference = 'Stop'
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

function Convert-TomlValue {
    param([string]$Value)
    $v = $Value.Trim()
    if ($v.StartsWith('[') -and $v.EndsWith(']')) {
        $inner = $v.Substring(1, $v.Length - 2).Trim()
        if ($inner -eq '') { return ,@() }
        $out = @()
        foreach ($p in $inner.Split(',')) {
            $item = $p.Trim()
            if ($item -ne '') { $out += (Convert-TomlValue $item) }
        }
        return ,$out
    }
    if ($v.StartsWith('"') -and $v.EndsWith('"') -and $v.Length -ge 2) {
        return $v.Substring(1, $v.Length - 2)
    }
    if ($v -eq 'true') { return $true }
    if ($v -eq 'false') { return $false }
    if ($v -match '^[0-9]+$') { return [int]$v }
    return $v
}

# 只解析本项目登记表用到的 TOML 子集：[meta]、[[dep]]、字符串/布尔/整数/字符串数组。
function Read-DepsToml {
    param([string]$Path)
    $lines = Read-Utf8Lines $Path
    $meta = @{}
    $deps = @()
    $cur = $null
    $section = ''
    $i = 0
    while ($i -lt $lines.Length) {
        $raw = $lines[$i].Trim()
        $i++
        if ($raw -eq '' -or $raw.StartsWith('#')) { continue }
        if ($raw.StartsWith('[[') -and $raw.EndsWith(']]')) {
            if ($cur) { $deps += ,$cur }
            $cur = @{}
            continue
        }
        if ($raw.StartsWith('[') -and $raw.EndsWith(']')) {
            $section = $raw.TrimStart('[').TrimEnd(']')
            continue
        }
        $eq = $raw.IndexOf('=')
        if ($eq -lt 1) { continue }
        $key = $raw.Substring(0, $eq).Trim()
        $val = $raw.Substring($eq + 1).Trim()
        if ($val.StartsWith('[') -and -not $val.EndsWith(']')) {
            while ($i -lt $lines.Length) {
                $val += ' ' + $lines[$i].Trim()
                $i++
                if ($val.EndsWith(']')) { break }
            }
        }
        $parsed = Convert-TomlValue $val
        if ($cur) { $cur[$key] = $parsed } else { $meta[$key] = $parsed }
    }
    if ($cur) { $deps += ,$cur }
    return @{ meta = $meta; deps = $deps }
}

function Get-ManifestDeps {
    param([string]$Path)
    $lines = Read-Utf8Lines $Path
    $section = ''
    $out = @()
    for ($i = 0; $i -lt $lines.Length; $i++) {
        $t = $lines[$i].Trim()
        if ($t -match '^\[\[') { $section = ''; continue }
        if ($t -match '^\[([A-Za-z0-9_.-]+)\]') { $section = $Matches[1]; continue }
        if ($section -ne 'dependencies' -and $section -ne 'dev-dependencies') { continue }
        if ($t -eq '' -or $t.StartsWith('#')) { continue }
        if ($t -match '^([A-Za-z0-9_-]+)\s*=') {
            $out += [pscustomobject]@{
                name    = $Matches[1]
                section = $section
                line    = ($i + 1)
                text    = $t
            }
        }
    }
    return $out
}

function Test-Adapters {
    param(
        [string]$RelPath,
        [string[]]$Prefixes
    )
    foreach ($p in $Prefixes) {
        $norm = $p.Replace('\', '/').TrimEnd('/')
        if ($norm -eq '') { continue }
        if ($RelPath -eq $norm) { return $true }
        if ($RelPath.StartsWith($norm + '/')) { return $true }
    }
    return $false
}

# ---------------- 载入事实源 ----------------
$allowPath = Join-Path $Root 'scripts/deps.allow.toml'
$manifestPath = Join-Path $Root 'Cargo.toml'
$toml = Read-DepsToml $allowPath
$deps = $toml.deps
$meta = $toml.meta
$manifest = Get-ManifestDeps $manifestPath

# ---------------- D1 清单同步 ----------------
$d1Details = @()
$d1Bad = 0
$manifestNames = @($manifest | ForEach-Object { $_.name })
$registered = @($deps | Where-Object { $_.role -ne 'data' })
$registeredNames = @($registered | ForEach-Object { $_.name })

foreach ($m in $manifest) {
    $entry = $registered | Where-Object { $_.name -eq $m.name } | Select-Object -First 1
    if (-not $entry) {
        $d1Bad++
        $d1Details += ('Cargo.toml:{0} {1} 未在 scripts/deps.allow.toml 登记' -f $m.line, $m.name)
        continue
    }
    $role = [string]$entry.role
    if ($role -eq 'dev' -and $m.section -ne 'dev-dependencies') {
        $d1Bad++
        $d1Details += ('{0} 标记为 dev，但在 [{1}] 中' -f $m.name, $m.section)
    }
    if (($role -eq 'capability' -or $role -eq 'hygiene') -and $m.section -ne 'dependencies') {
        $d1Bad++
        $d1Details += ('{0} 标记为 {1}，但不在 [dependencies] 中' -f $m.name, $role)
    }
    if ($entry.required_features) {
        foreach ($f in @($entry.required_features)) {
            if ($m.text -notmatch ('"' + [regex]::Escape($f) + '"')) {
                $d1Bad++
                $d1Details += ('Cargo.toml:{0} {1} 缺少必需特性 {2}' -f $m.line, $m.name, $f)
            }
        }
    }
    if ($entry.default_features -eq $false) {
        if ($m.text -notmatch 'default-features\s*=\s*false') {
            $d1Bad++
            $d1Details += ('Cargo.toml:{0} {1} 未关闭默认特性' -f $m.line, $m.name)
        }
    }
}

foreach ($entry in $registered) {
    if ($manifestNames -notcontains $entry.name) {
        $d1Bad++
        $d1Details += ('登记表条目 {0} 在 Cargo.toml 中不存在' -f $entry.name)
    }
}

$d1Status = 'PASS'
if ($d1Bad -gt 0) { $d1Status = 'FAIL' }
$d1Summary = ('manifest 依赖 {0} 个｜登记 {1} 个｜不一致 {2} 处' -f $manifest.Count, $registered.Count, $d1Bad)
Add-Check -Id 'D1' -Name '依赖清单同步' -Status $d1Status -Summary $d1Summary -Details ($d1Details | Select-Object -First $DetailLimit)

# ---------------- D2 导入边界 ----------------
$d2Details = @()
$d2Bad = 0
$d2Baseline = 0
$libMap = @{}
foreach ($entry in $deps) {
    if ($entry.role -eq 'capability' -and $entry.lib) { $libMap[[string]$entry.lib] = $entry }
}

$srcDir = Join-Path $Root 'src'
$srcFiles = Get-ChildItem -LiteralPath $srcDir -Recurse -File -Filter '*.rs'
foreach ($file in $srcFiles) {
    $rel = Get-RelPath $file.FullName
    $lines = Read-Utf8Lines $file.FullName
    for ($i = 0; $i -lt $lines.Length; $i++) {
        $line = $lines[$i]
        $importRoot = ''
        if ($line -match '^\s*(?:pub\s+)?use\s+([A-Za-z_][A-Za-z0-9_]*)::') {
            $importRoot = $Matches[1]
        }
        elseif ($line -match '^\s*extern\s+crate\s+([A-Za-z_][A-Za-z0-9_]*)') {
            $importRoot = $Matches[1]
        }
        if (-not $importRoot) { continue }
        if (-not $libMap.ContainsKey($importRoot)) { continue }
        $entry = $libMap[$importRoot]

        $allowed = Test-Adapters -RelPath $rel -Prefixes @($entry.adapter)
        if (-not $allowed) { $allowed = Test-Adapters -RelPath $rel -Prefixes @($entry.test_paths) }
        if (-not $allowed) { $allowed = Test-Adapters -RelPath $rel -Prefixes @($entry.local_shadow) }
        if ($allowed) { continue }

        $isBaseline = Test-Adapters -RelPath $rel -Prefixes @($entry.known_violation)
        if ($isBaseline) {
            $d2Baseline++
            $d2Details += ('{0}:{1} {2} 使用 {3}（已登记债务，见 docs/roadmap/backlog/open-issues.md）' -f $rel, ($i + 1), $importRoot, $entry.name)
        }
        else {
            $d2Bad++
            $d2Details += ('{0}:{1} 越界使用 {2}（适配器位置：{3}）' -f $rel, ($i + 1), $importRoot, (@($entry.adapter) -join ', '))
        }
    }
}

$d2Status = 'PASS'
if ($d2Bad -gt 0) { $d2Status = 'FAIL' }
$d2Summary = ('扫描 {0} 个 Rust 文件｜新增越界 {1} 处｜已登记基线 {2} 处' -f $srcFiles.Count, $d2Bad, $d2Baseline)
Add-Check -Id 'D2' -Name '导入边界' -Status $d2Status -Summary $d2Summary -Details ($d2Details | Select-Object -First $DetailLimit)

# ---------------- 外部工具探测与调用 ----------------
# 原生程序不接入 PowerShell 错误流：$ErrorActionPreference='Stop' 下 stderr 会被包装成
# 终止性 NativeCommandError，中断后续检查。统一走 Start-Process + 临时文件捕获 stdout/stderr，
# 退出码与输出分离返回，调用方自行判定，不影响脚本继续执行。
function Invoke-NativeCapture {
    param(
        [string]$Exe,
        [string[]]$Arguments = @(),
        [string]$WorkingDirectory = $Root
    )
    $outFile = [System.IO.Path]::GetTempFileName()
    $errFile = [System.IO.Path]::GetTempFileName()
    try {
        try {
            $proc = Start-Process -FilePath $Exe -ArgumentList $Arguments -WorkingDirectory $WorkingDirectory `
                -NoNewWindow -Wait -PassThru -RedirectStandardOutput $outFile -RedirectStandardError $errFile
            $code = $proc.ExitCode
        }
        catch {
            return [pscustomobject]@{ exit_code = -1; stdout = ''; stderr = [string]$_.Exception.Message }
        }
        return [pscustomobject]@{
            exit_code = $code
            stdout    = [System.IO.File]::ReadAllText($outFile, [System.Text.Encoding]::UTF8)
            stderr    = [System.IO.File]::ReadAllText($errFile, [System.Text.Encoding]::UTF8)
        }
    }
    finally {
        Remove-Item -LiteralPath $outFile, $errFile -ErrorAction SilentlyContinue
    }
}

# 工具探测：先 PATH，再 $env:USERPROFILE\.cargo\bin（cargo install 的默认落点）。
# 找不到时返回实际查找过的路径，便于排障。
function Resolve-Tool {
    param([string]$Name)
    $attempted = @()
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    $found = ''
    $cmd = Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue
    if ($cmd) {
        $found = $cmd.Source
        $attempted += ('PATH → ' + $found)
    }
    else {
        $attempted += ('PATH → 未找到 ' + $Name)
        foreach ($candidate in @((Join-Path $cargoBin ($Name + '.exe')), (Join-Path $cargoBin $Name))) {
            $attempted += $candidate
            if (-not $found -and (Test-Path -LiteralPath $candidate)) { $found = $candidate }
        }
    }
    if (-not $found) {
        return [pscustomobject]@{ found = $false; path = ''; version = ''; attempted = $attempted }
    }
    $ver = Invoke-NativeCapture -Exe $found -Arguments @('--version')
    $version = ''
    if ($ver.stdout.Trim()) { $version = (($ver.stdout.Trim() -split "`r?`n")[0]) }
    return [pscustomobject]@{ found = $true; path = $found; version = $version; attempted = $attempted }
}

function Get-LineSnippet {
    param([string]$Text, [int]$Limit = 10)
    if (-not $Text) { return @() }
    return @($Text -split "`r?`n" | Where-Object { $_.Trim() -ne '' } | Select-Object -First $Limit)
}

# 从 crate 根沿 `pub mod` 收集可达的公开模块文件；私有模块内的 pub 项不属于公开 API。
function Get-PublicModuleFiles {
    param([string]$RepoRoot)
    $pubFiles = @{}
    $seen = @{}
    $queue = New-Object System.Collections.Queue
    $queue.Enqueue(@{ File = (Join-Path $RepoRoot 'src/lib.rs'); Base = '' })
    while ($queue.Count -gt 0) {
        $item = $queue.Dequeue()
        if ($seen.ContainsKey($item.File)) { continue }
        $seen[$item.File] = $true
        if (-not (Test-Path -LiteralPath $item.File)) { continue }
        $pubFiles[$item.File] = $true
        foreach ($ln in (Read-Utf8Lines $item.File)) {
            if ($ln -notmatch '^\s*pub\s+mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;') { continue }
            $name = $Matches[1]
            $base = if ($item.Base -eq '') { $name } else { $item.Base + '/' + $name }
            $dir = Join-Path (Join-Path $RepoRoot 'src') $base
            $modFile = Join-Path $dir 'mod.rs'
            $flatFile = $dir + '.rs'
            $target = ''
            if (Test-Path -LiteralPath $modFile) { $target = $modFile }
            elseif (Test-Path -LiteralPath $flatFile) { $target = $flatFile }
            if (-not $target) { continue }
            $queue.Enqueue(@{ File = $target; Base = $base })
        }
    }
    return $pubFiles
}

# ---------------- D3 公开 API ----------------
# 不依赖 nightly：从 crate 根按 `pub mod` 收窄到可达公开模块，静态扫描 pub 项签名（fn
# 参数/返回、公有结构体具名字段、type/const/static、re-export 与公有枚举变体负载），
# 命中「外部 crate 根名」或「本文件从该 crate 导入的标识符」即 FAIL。
# 只有无法执行（无 src 文件）才 SKIP；检出问题不降级。
$apiRoots = @{}
foreach ($entry in $deps) {
    if ([string]$entry.lib -ne '' -and ($entry.role -eq 'capability' -or $entry.role -eq 'hygiene')) {
        $apiRoots[[string]$entry.lib] = [string]$entry.name
    }
}
$d3Details = @()
$d3Bad = 0
$d3Surfaces = 0
$pubFiles = Get-PublicModuleFiles $Root
$apiFiles = @($srcFiles | Where-Object { $pubFiles.ContainsKey($_.FullName) })
if ($srcFiles.Count -eq 0 -or $apiFiles.Count -eq 0) {
    Add-Check -Id 'D3' -Name '公开 API 无外部类型' -Status 'SKIP' -Summary '无法执行：未从 crate 根解析到可达的公开模块文件' -Details @()
}
else {
    $rootsRx = '(?<![A-Za-z0-9_])(' + (($apiRoots.Keys | ForEach-Object { [regex]::Escape($_) }) -join '|') + ')\s*::'
    foreach ($file in $apiFiles) {
        $rel = Get-RelPath $file.FullName
        $lines = Read-Utf8Lines $file.FullName
        $aliases = @{}
        foreach ($ln in $lines) {
            if ($ln -notmatch '^\s*(?:pub\s+)?use\s+([A-Za-z_][A-Za-z0-9_]*)::') { continue }
            if (-not $apiRoots.ContainsKey($Matches[1])) { continue }
            $tail = ($ln -replace '^\s*(?:pub\s+)?use\s+', '') -replace '\s+as\s+', ' ' -replace ';.*$', ''
            foreach ($m in [regex]::Matches($tail, '[A-Za-z_][A-Za-z0-9_]*')) {
                if (-not $aliases.ContainsKey($m.Value)) { $aliases[$m.Value] = $true }
            }
        }
        $inStruct = $false
        $inEnum = $false
        $scopeDepth = 0
        for ($i = 0; $i -lt $lines.Length; $i++) {
            $t = $lines[$i].Trim()
            $buf = $null
            $declLine = $i + 1
            $delta = ([regex]::Matches($t, '{')).Count - ([regex]::Matches($t, '}')).Count
            if ($inStruct) {
                $scopeDepth += $delta
                if ($scopeDepth -le 0) { $inStruct = $false }
                if ($t -match '^pub\s+[A-Za-z_][A-Za-z0-9_]*\s*:') { $buf = $t }
            }
            elseif ($inEnum) {
                $scopeDepth += $delta
                if ($scopeDepth -le 0) { $inEnum = $false }
                if ($t -match '^[A-Za-z_][A-Za-z0-9_]*\s*\(') { $buf = $t }
            }
            if (-not $buf) {
                $isFn = ($t -match '^pub\s+fn\b') -or ($t -match '^pub\s+(async|unsafe|const|extern)\s+fn\b')
                if ($t -match '^pub\s+struct\s') {
                    if ($delta -gt 0) { $inStruct = $true; $scopeDepth = $delta }
                    $buf = $t
                }
                elseif ($t -match '^pub\s+enum\s') {
                    if ($delta -gt 0) { $inEnum = $true; $scopeDepth = $delta }
                    $buf = $t
                }
                elseif ($isFn -or $t -match '^pub\s+(type|const|static|use)\b') {
                    $buf = $t
                    if ($isFn) {
                        while ($i + 1 -lt $lines.Length) {
                            $bt = $buf.Trim()
                            $open = ([regex]::Matches($buf, '\(')).Count
                            $close = ([regex]::Matches($buf, '\)')).Count
                            if ($open -le $close -and ($bt.EndsWith(';') -or $bt.EndsWith('{'))) { break }
                            $i++
                            $buf += ' ' + $lines[$i].Trim()
                        }
                    }
                }
            }
            if (-not $buf) { continue }
            $d3Surfaces++
            $hit = $buf -match $rootsRx
            if (-not $hit) {
                foreach ($a in $aliases.Keys) {
                    if ($buf -match ('(?<![A-Za-z0-9_])' + [regex]::Escape($a) + '(?![A-Za-z0-9_])')) { $hit = $true; break }
                }
            }
            if ($hit) {
                $d3Bad++
                $snippet = ($buf -replace '\s+', ' ')
                if ($snippet.Length -gt 110) { $snippet = $snippet.Substring(0, 110) + '…' }
                $d3Details += ('{0}:{1} {2}' -f $rel, $declLine, $snippet)
            }
        }
    }
    $d3Status = 'PASS'
    if ($d3Bad -gt 0) { $d3Status = 'FAIL' }
    Add-Check -Id 'D3' -Name '公开 API 无外部类型' -Status $d3Status -Summary ('公开模块文件 {0}/{1} 个、公开签名 {2} 处｜检测 {3} 个外部 crate｜命中 {4} 处' -f $apiFiles.Count, $srcFiles.Count, $d3Surfaces, $apiRoots.Count, $d3Bad) -Details ($d3Details | Select-Object -First $DetailLimit)
}

# ---------------- D4 许可证 ----------------
$denyCfg = Join-Path $Root 'deny.toml'
$denyProbe = Resolve-Tool 'cargo-deny'
if (-not $denyProbe.found) {
    Add-Check -Id 'D4' -Name '许可证相容' -Status 'SKIP' -Summary ('缺少 cargo-deny；已查找：{0}；安装：cargo install cargo-deny --locked' -f ($denyProbe.attempted -join ' | ')) -Details @()
}
elseif (-not (Test-Path -LiteralPath $denyCfg)) {
    Add-Check -Id 'D4' -Name '许可证相容' -Status 'FAIL' -Summary '缺少 deny.toml，无法执行许可证判定' -Details @('新建 deny.toml，allow 列表见 docs/roadmap/principles.md 工程约定 #1')
}
else {
    $denyRun = Invoke-NativeCapture -Exe $denyProbe.path -Arguments @('check', 'licenses')
    $denyStatus = 'PASS'
    if ($denyRun.exit_code -ne 0) { $denyStatus = 'FAIL' }
    $denySummary = ('{0}｜退出码 {1}' -f $denyProbe.version, $denyRun.exit_code)
    Add-Check -Id 'D4' -Name '许可证相容' -Status $denyStatus -Summary $denySummary -Details (Get-LineSnippet ($denyRun.stdout + "`n" + $denyRun.stderr) $DetailLimit)
}

# ---------------- D5 安全公告 ----------------
$auditProbe = Resolve-Tool 'cargo-audit'
if (-not $auditProbe.found) {
    Add-Check -Id 'D5' -Name 'RUSTSEC 安全公告' -Status 'SKIP' -Summary ('缺少 cargo-audit；已查找：{0}；安装：cargo install cargo-audit --locked' -f ($auditProbe.attempted -join ' | ')) -Details @()
}
else {
    $auditRun = Invoke-NativeCapture -Exe $auditProbe.path -Arguments @('audit', '--format', 'json')
    $auditDb = Join-Path $env:USERPROFILE '.cargo\advisory-db'
    $sources = @(('本地库 ' + $auditDb), '远端 git https://github.com/RustSec/advisory-db')
    $report = $null
    if ($auditRun.stdout.Trim()) {
        try { $report = $auditRun.stdout | ConvertFrom-Json } catch { $report = $null }
    }
    if (-not $report -or -not $report.PSObject.Properties['vulnerabilities']) {
        $blob = ($auditRun.stdout + "`n" + $auditRun.stderr)
        $unreachable = $blob -match 'failed to fill whole buffer|error sending request|failed to fetch|couldn''t fetch|could not fetch|Connection refused|timed out|unable to update|failed to update|git fetch|no such file or directory'
        if ($unreachable) {
            $reason = ('公告库不可达，未作漏洞判定；已尝试数据源：{0}；退出码 {1}' -f ($sources -join ' | '), $auditRun.exit_code)
            Add-Check -Id 'D5' -Name 'RUSTSEC 安全公告' -Status 'SKIP' -Summary $reason -Details (Get-LineSnippet $blob $DetailLimit)
        }
        else {
            $reason = ('cargo audit 未产出可解析报告（退出码 {0}），无法区分不可达与命中' -f $auditRun.exit_code)
            Add-Check -Id 'D5' -Name 'RUSTSEC 安全公告' -Status 'SKIP' -Summary $reason -Details (Get-LineSnippet $blob $DetailLimit)
        }
    }
    else {
        $vuln = $report.vulnerabilities
        $warnUnmaintained = @()
        if ($report.PSObject.Properties['warnings'] -and $report.warnings.PSObject.Properties['unmaintained']) {
            $warnUnmaintained = @($report.warnings.unmaintained)
        }
        $d5Details = @()
        if ($vuln.found) {
            foreach ($v in @($vuln.list)) {
                $d5Details += ('{0} {1}：{2}' -f $v.advisory.id, $v.package.name, $v.advisory.title)
            }
        }
        foreach ($w in $warnUnmaintained) {
            $d5Details += ('提示（非漏洞）{0} {1}：{2}' -f $w.advisory.id, $w.package.name, $w.advisory.title)
        }
        $db = $report.database
        $d5Status = 'PASS'
        if ($vuln.found) { $d5Status = 'FAIL' }
        $d5Summary = ('{0}｜漏洞 {1} 条｜unmaintained 提示 {2} 条｜公告库 {3} 条，快照 {4}（commit {5}）' -f `
                $auditProbe.version, $vuln.count, $warnUnmaintained.Count, $db.'advisory-count', $db.'last-updated', $db.'last-commit')
        Add-Check -Id 'D5' -Name 'RUSTSEC 安全公告' -Status $d5Status -Summary $d5Summary -Details ($d5Details | Select-Object -First $DetailLimit)
    }
}

# ---------------- D6 unsafe 用量 ----------------
$geoProbe = Resolve-Tool 'cargo-geiger'
if (-not $geoProbe.found) {
    Add-Check -Id 'D6' -Name 'unsafe 用量' -Status 'SKIP' -Summary ('缺少 cargo-geiger；已查找：{0}；安装：cargo install cargo-geiger --locked' -f ($geoProbe.attempted -join ' | ')) -Details @()
}
else {
    $geoRun = Invoke-NativeCapture -Exe $geoProbe.path -Arguments @('--output-format', 'Json')
    $geoReport = $null
    if ($geoRun.stdout.Trim()) {
        try { $geoReport = $geoRun.stdout | ConvertFrom-Json } catch { $geoReport = $null }
    }
    $self = $null
    if ($geoReport) {
        $self = @($geoReport.packages | Where-Object { $_.package.id.name -eq 'pdf_engine' }) | Select-Object -First 1
    }
    if (-not $self) {
        $reason = ('cargo-geiger 未产出可解析的 pdf_engine 报告（退出码 {0}）' -f $geoRun.exit_code)
        Add-Check -Id 'D6' -Name 'unsafe 用量' -Status 'SKIP' -Summary $reason -Details (Get-LineSnippet ($geoRun.stderr + "`n" + $geoRun.stdout) $DetailLimit)
    }
    else {
        $categories = @('functions', 'exprs', 'item_impls', 'item_traits', 'methods')
        $selfBad = 0
        $parts = @()
        foreach ($k in $categories) {
            $unsafeCount = [int]$self.unsafety.used.$k.unsafe_
            $safeCount = [int]$self.unsafety.used.$k.safe
            $selfBad += $unsafeCount
            $parts += ('{0} {1}/{2}' -f $k, $unsafeCount, ($unsafeCount + $safeCount))
        }
        $depPkgs = 0
        $depFuncs = 0
        $totalPkgs = 0
        foreach ($p in @($geoReport.packages)) {
            if ($p.package.id.name -eq 'pdf_engine') { continue }
            $totalPkgs++
            $depUnsafe = 0
            foreach ($k in $categories) { $depUnsafe += [int]$p.unsafety.used.$k.unsafe_ }
            if ($depUnsafe -gt 0) {
                $depPkgs++
                $depFuncs += [int]$p.unsafety.used.functions.unsafe_
            }
        }
        $d6Status = 'PASS'
        if ($selfBad -gt 0) { $d6Status = 'FAIL' }
        # geiger 0.14 的 forbids_unsafe 实测识别不到 lib 根上的属性；
        # 声明是否落到源码以 crate 根文件直接核对为准。
        $forbidDeclared = $false
        $libSrc = Join-Path $Root 'src/lib.rs'
        if (Test-Path -LiteralPath $libSrc) {
            $libLines = Read-Utf8Lines $libSrc
            for ($i = 0; $i -lt [Math]::Min($libLines.Length, 80); $i++) {
                if ($libLines[$i].Trim() -eq '#![forbid(unsafe_code)]') { $forbidDeclared = $true; break }
            }
        }
        $forbidText = if ($forbidDeclared) { '已声明（src/lib.rs）' } else { '未声明' }
        $geoForbid = 'false'
        if ($self.unsafety.PSObject.Properties['forbids_unsafe'] -and $self.unsafety.forbids_unsafe) { $geoForbid = 'true' }
        $d6Summary = ('{0}｜pdf_engine 自身 unsafe {1}（{2}）｜forbid(unsafe_code) {3}｜依赖 {4}/{5} 包含 unsafe（其 unsafe 函数 {6}，仅信息）' -f `
                $geoProbe.version, $selfBad, ($parts -join '，'), $forbidText, $depPkgs, $totalPkgs, $depFuncs)
        $d6Details = @(('判定口径：只计 pdf_engine 自身 used 类别；依赖图 unsafe 为信息项，不计失败。'))
        if ($forbidDeclared -and $geoForbid -eq 'false') {
            $d6Details += ('cargo-geiger 的 forbids_unsafe=false 未识别 lib 根属性，以源码声明为准。')
        }
        if ($geoRun.exit_code -ne 0) {
            $d6Details += ('cargo-geiger 退出码 {0}（告警导致，不影响计数结论）' -f $geoRun.exit_code)
        }
        Add-Check -Id 'D6' -Name 'unsafe 用量' -Status $d6Status -Summary $d6Summary -Details ($d6Details | Select-Object -First $DetailLimit)
    }
}

# ---------------- D7 复核时效与溯源 ----------------
$d7Details = @()
$d7Bad = 0
$now = Get-Date
$interval = 365
if ($meta.review_interval_days) { $interval = [int]$meta.review_interval_days }
$validMaintenance = @('active', 'frozen-format', 'vendored')

foreach ($entry in $deps) {
    $mtype = [string]$entry.maintenance
    if ($validMaintenance -notcontains $mtype) {
        $d7Bad++
        $d7Details += ('{0} 的 maintenance 值 {1} 不合法' -f $entry.name, $mtype)
    }
    if (-not $entry.reviewed) {
        $d7Bad++
        $d7Details += ('{0} 缺少 reviewed 日期' -f $entry.name)
        continue
    }
    try {
        $reviewed = [datetime]::ParseExact([string]$entry.reviewed, 'yyyy-MM-dd', $null)
        $age = ($now - $reviewed).Days
        if ($age -gt $interval) {
            $d7Bad++
            $d7Details += ('{0} 复核已过期 {1} 天（上限 {2} 天）' -f $entry.name, $age, $interval)
        }
    }
    catch {
        $d7Bad++
        $d7Details += ('{0} 的 reviewed 日期格式非法：{1}' -f $entry.name, $entry.reviewed)
    }
}

foreach ($entry in @($deps | Where-Object { $_.role -eq 'data' })) {
    foreach ($field in @('path', 'provenance', 'license_file')) {
        $val = [string]$entry[$field]
        if (-not $val) {
            $d7Bad++
            $d7Details += ('{0} 缺少 {1} 字段' -f $entry.name, $field)
            continue
        }
        if (-not (Test-Path -LiteralPath (Join-Path $Root $val.Replace('/', '\')))) {
            $d7Bad++
            $d7Details += ('{0} 的 {1} 不存在：{2}' -f $entry.name, $field, $val)
        }
    }
}

$d7Status = 'PASS'
if ($d7Bad -gt 0) { $d7Status = 'FAIL' }
$d7Summary = ('复核条目 {0} 个｜超期或非法 {1} 处｜复核周期 {2} 天' -f $deps.Count, $d7Bad, $interval)
Add-Check -Id 'D7' -Name '复核时效与溯源' -Status $d7Status -Summary $d7Summary -Details ($d7Details | Select-Object -First $DetailLimit)

# ---------------- D8 引入不降级登记 ----------------
# 每个依赖必须登记行为差异与是否退化；退化项必须另有逐条差异、接受理由与固化测试，
# 缺字段或路径不存在即 FAIL。
$d8Details = @()
$d8Bad = 0
$d8Degraded = 0
foreach ($entry in $deps) {
    $name = [string]$entry.name
    if ([string]$entry['behaviour_delta'] -eq '') {
        $d8Bad++
        $d8Details += ('{0} 缺少 behaviour_delta（能力替换/新增、错误语义、容错恢复、性能内存、MSRV、维护与许可证的差异说明）' -f $name)
    }
    if (-not $entry.ContainsKey('degraded')) {
        $d8Bad++
        $d8Details += ('{0} 缺少 degraded 布尔字段' -f $name)
        continue
    }
    if ($entry['degraded'] -eq $true) {
        $d8Degraded++
        if (@($entry['degradations']).Count -eq 0) {
            $d8Bad++
            $d8Details += ('{0} degraded=true 但未登记 degradations' -f $name)
        }
        if ([string]$entry['degradation_acceptance'] -eq '') {
            $d8Bad++
            $d8Details += ('{0} degraded=true 但未登记 degradation_acceptance' -f $name)
        }
        if (@($entry['degradation_tests']).Count -eq 0) {
            $d8Bad++
            $d8Details += ('{0} degraded=true 但未绑定 degradation_tests' -f $name)
        }
        foreach ($tpath in @($entry['degradation_tests'])) {
            if ($tpath -and -not (Test-Path -LiteralPath (Join-Path $Root $tpath))) {
                $d8Bad++
                $d8Details += ('{0} 的 degradation_tests 路径不存在：{1}' -f $name, $tpath)
            }
        }
        foreach ($g in @($entry['degradations'])) { $d8Details += ('基线差异 {0}：{1}' -f $name, $g) }
    }
}
$d8Status = 'PASS'
if ($d8Bad -gt 0) { $d8Status = 'FAIL' }
$d8Summary = ('登记依赖 {0} 个｜缺字段或补偿 {1} 处｜退化基线 {2} 个' -f $deps.Count, $d8Bad, $d8Degraded)
Add-Check -Id 'D8' -Name '引入不降级登记' -Status $d8Status -Summary $d8Summary -Details ($d8Details | Select-Object -First $DetailLimit)

# ---------------- D9 依赖 MSRV 不高于本项目 ----------------
# 事实源：cargo metadata 的 packages[].rust_version。从根包沿 resolve 图遍历：
# 普通依赖（kind 非 dev、target 为空）逐包比较声明值，任一高于 Cargo.toml 的
# rust-version 即 FAIL；platform-specific 与 dev-dependencies 单列报告、不计 FAIL；
# 未声明 rust_version 的包不参与比较（其真实下限由选型评估单独核实）。
$d9Details = @()
$d9Bad = 0
$projectMsrv = ''
foreach ($ln in (Read-Utf8Lines $manifestPath)) {
    if ($ln -match '^\s*rust-version\s*=\s*"([^"]+)"') { $projectMsrv = $Matches[1]; break }
}

function Convert-MsrvParts {
    param([string]$Value)
    $parts = @()
    if (-not $Value) { return ,$parts }
    foreach ($p in ($Value -split '\.')) {
        if ($p -notmatch '^[0-9]+$') { break }
        $parts += [int]$p
    }
    return ,$parts
}

function Test-MsrvHigher {
    param([string]$Left, [string]$Right)
    $l = Convert-MsrvParts $Left
    $r = Convert-MsrvParts $Right
    if ($l.Count -eq 0 -or $r.Count -eq 0) { return $false }
    $n = [Math]::Max($l.Count, $r.Count)
    for ($i = 0; $i -lt $n; $i++) {
        $lv = if ($i -lt $l.Count) { $l[$i] } else { 0 }
        $rv = if ($i -lt $r.Count) { $r[$i] } else { 0 }
        if ($lv -gt $rv) { return $true }
        if ($lv -lt $rv) { return $false }
    }
    return $false
}

$d9Status = 'PASS'
if (-not $projectMsrv) {
    $d9Status = 'SKIP'
    $d9Summary = 'Cargo.toml 未声明 rust-version，无法比较依赖 MSRV'
}
else {
    $cargoProbe = Resolve-Tool 'cargo'
    if (-not $cargoProbe.found) {
        $d9Status = 'SKIP'
        $d9Summary = ('缺少 cargo；已查找：{0}' -f ($cargoProbe.attempted -join ' | '))
    }
    else {
        $metaRun = Invoke-NativeCapture -Exe $cargoProbe.path -Arguments @('metadata', '--format-version', '1')
        if ($metaRun.exit_code -ne 0) {
            $d9Status = 'SKIP'
            $d9Summary = ('cargo metadata 失败（退出码 {0}），无法读取依赖 rust_version' -f $metaRun.exit_code)
            $d9Details += (Get-LineSnippet $metaRun.stderr $DetailLimit)
        }
        else {
            $metadata = $metaRun.stdout | ConvertFrom-Json
            if (-not $metadata.resolve) {
                $d9Status = 'SKIP'
                $d9Summary = 'cargo metadata 未返回 resolve 图，无法判定依赖来源'
            }
            else {
                $pkgById = @{}
                foreach ($p in $metadata.packages) { $pkgById[[string]$p.id] = $p }
                $nodesById = @{}
                foreach ($n in $metadata.resolve.nodes) { $nodesById[[string]$n.id] = $n }

                $mandatory = @{}
                $platform = @{}
                $dev = @{}
                $rootId = [string]$metadata.resolve.root
                $queue = New-Object System.Collections.Queue
                $queue.Enqueue($rootId)
                $seenNode = @{}
                while ($queue.Count -gt 0) {
                    $id = [string]$queue.Dequeue()
                    if ($seenNode.ContainsKey($id)) { continue }
                    $seenNode[$id] = $true
                    if (-not $nodesById.ContainsKey($id)) { continue }
                    $node = $nodesById[$id]
                    foreach ($d in $node.deps) {
                        if (-not $d.dep_kinds) { continue }
                        foreach ($k in $d.dep_kinds) {
                            if ($k.kind -eq 'dev') {
                                if ($id -eq $rootId) { $dev[[string]$d.pkg] = $true }
                                continue
                            }
                            if ($k.target -ne $null -and [string]$k.target -ne '') {
                                $platform[[string]$d.pkg] = $true
                                continue
                            }
                            $mandatory[[string]$d.pkg] = $true
                            $queue.Enqueue([string]$d.pkg)
                        }
                    }
                }

                $mandCount = 0
                $mandNoDecl = 0
                $mandMax = ''
                foreach ($id in $mandatory.Keys) {
                    $p = $pkgById[$id]
                    if (-not $p) { continue }
                    $mandCount++
                    $rv = [string]$p.rust_version
                    if (-not $rv) { $mandNoDecl++; continue }
                    if (-not $mandMax -or (Test-MsrvHigher $rv $mandMax)) { $mandMax = $rv }
                    if (Test-MsrvHigher $rv $projectMsrv) {
                        $d9Bad++
                        $d9Details += ('{0} {1} 要求 Rust {2}，高于项目声明 {3}' -f $p.name, $p.version, $rv, $projectMsrv)
                    }
                }

                $platMax = ''
                foreach ($id in $platform.Keys) {
                    $p = $pkgById[$id]
                    if (-not $p) { continue }
                    $rv = [string]$p.rust_version
                    if ($rv -and (-not $platMax -or (Test-MsrvHigher $rv $platMax))) { $platMax = $rv }
                    $d9Details += ('平台特定（不计失败）：{0} {1} 声明 Rust {2}' -f $p.name, $p.version, $(if ($rv) { $rv } else { '未声明' }))
                }

                $devMax = ''
                foreach ($id in $dev.Keys) {
                    $p = $pkgById[$id]
                    if (-not $p) { continue }
                    $rv = [string]$p.rust_version
                    if ($rv -and (-not $devMax -or (Test-MsrvHigher $rv $devMax))) { $devMax = $rv }
                    $d9Details += ('dev（不计失败）：{0} {1} 声明 Rust {2}' -f $p.name, $p.version, $(if ($rv) { $rv } else { '未声明' }))
                }

                $d9Summary = ('项目声明 {0}｜普通依赖 {1} 个（声明最高 {2}，未声明 {3} 个）｜超标 {4} 处｜平台特定 {5} 个（最高 {6}）｜dev {7} 个（最高 {8}）' -f `
                        $projectMsrv, $mandCount, $(if ($mandMax) { $mandMax } else { '—' }), $mandNoDecl, $d9Bad, `
                        $platform.Count, $(if ($platMax) { $platMax } else { '—' }), $dev.Count, $(if ($devMax) { $devMax } else { '—' }))
            }
        }
    }
}
if ($d9Status -eq 'PASS' -and $d9Bad -gt 0) { $d9Status = 'FAIL' }
Add-Check -Id 'D9' -Name '依赖 MSRV 不高于本项目' -Status $d9Status -Summary $d9Summary -Details ($d9Details | Select-Object -First $DetailLimit)

# ---------------- 输出 ----------------
$failed = @($script:checks | Where-Object { $_.status -eq 'FAIL' }).Count
$skipped = @($script:checks | Where-Object { $_.status -eq 'SKIP' }).Count

if ($Json) {
    $payload = [ordered]@{
        root    = $Root
        strict  = [bool]$Strict
        summary = [ordered]@{
            total   = $script:checks.Count
            passed  = @($script:checks | Where-Object { $_.status -eq 'PASS' }).Count
            failed  = $failed
            skipped = $skipped
        }
        checks  = $script:checks
    }
    $payload | ConvertTo-Json -Depth 6
}
else {
    foreach ($c in $script:checks) {
        Write-Host ('{0} {1} {2}：{3}' -f $c.id, $c.status, $c.name, $c.summary)
        foreach ($d in $c.details) { Write-Host ('    - ' + $d) }
    }
    Write-Host ('合计：{0} 项检查，PASS {1}，FAIL {2}，SKIP {3}' -f $script:checks.Count, ($script:checks.Count - $failed - $skipped), $failed, $skipped)
}

if ($Strict -and $failed -gt 0) { exit 1 }
exit 0
