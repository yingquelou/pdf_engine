# 规范一致性检查的注入式自测（S8/-RunTests 时由 spec_check.ps1 调用）。
# 在 target/spec_check_selftest/ 构造最小文档树，复用真实索引（-IndexDir），不触碰 docs/。
[CmdletBinding()]
param(
    [string]$Root = ''
)

$ErrorActionPreference = 'Stop'
if (-not $Root) { $Root = Split-Path -Parent $PSScriptRoot }
$Root = (Resolve-Path -LiteralPath $Root).Path

$checkScript = Join-Path $PSScriptRoot 'spec_check.ps1'
$realIndex = Join-Path $Root 'docs/spec-index'
$sandbox = Join-Path $Root 'target/spec_check_selftest'

$S_OK = [string][char]0x2705
$S_MID = [string]([char]0xD83D) + [string]([char]0xDFE1)

$matrixTemplate = '| @ANCHOR@ | Form XObject `/Matrix` | 1.0 | @STATUS@ | @OK@ | @OK@ | P0 | `build_form_matrix` 测试 `@TEST@`@EXTRA@ |'
$itemTemplate = '- [ ] **A1 表单矩阵与设备映射**（@ANCHOR@）：实现 Form XObject `/Matrix`。'
$itemNoAnchorTemplate = '- [ ] **A1 表单矩阵与设备映射**：实现 Form XObject `/Matrix`。'
$sourceTemplate = @'
pub struct FormMatrix;

pub fn build_form_matrix() -> FormMatrix { FormMatrix }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_matrix_roundtrip() {
        let _ = build_form_matrix();
    }
}
'@

function Write-Fixture {
    param([string]$Anchor, [string]$Status, [string]$Test, [string]$Source, [string]$MatrixHeading,
        [switch]$NoAnchor, [string]$Marker = '', [string]$MatrixMarker = '')
    Remove-Item -LiteralPath $sandbox -Recurse -Force -ErrorAction SilentlyContinue
    $matrixDir = Join-Path $sandbox 'docs/matrix/ch08'
    $stageDir = Join-Path $sandbox 'docs/roadmap/stages'
    $srcDir = Join-Path $sandbox 'src'
    foreach ($d in @($matrixDir, $stageDir, $srcDir)) { New-Item -ItemType Directory -Force -Path $d | Out-Null }

    $row = $matrixTemplate.Replace('@ANCHOR@', $Anchor).Replace('@STATUS@', $Status).Replace('@OK@', $S_OK).Replace('@TEST@', $Test).Replace('@EXTRA@', $MatrixMarker)
    $md = $MatrixHeading + "`n`n| 章节 | 特性 | 引入版本 | 状态 | Reader | Writer | 优先级 | 证据/备注 |`n|---|---|---|---|---|---|---|---|`n" + $row + "`n"
    [System.IO.File]::WriteAllText((Join-Path $matrixDir 'form-xobject.md'), $md, (New-Object System.Text.UTF8Encoding($false)))

    $item = if ($NoAnchor) { $itemNoAnchorTemplate } else { $itemTemplate.Replace('@ANCHOR@', $Anchor) }
    $item = $item + $Marker
    [System.IO.File]::WriteAllText((Join-Path $stageDir 'selftest.md'), ($item + "`n"), (New-Object System.Text.UTF8Encoding($false)))
    [System.IO.File]::WriteAllText((Join-Path $srcDir 'lib.rs'), $Source, (New-Object System.Text.UTF8Encoding($false)))
}

function Invoke-SpecCheck {
    $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $checkScript -Root $sandbox -IndexDir $realIndex -Json -DetailLimit 10 2>&1
    $text = (@($out) | ForEach-Object { [string]$_ }) -join "`n"
    $pos = $text.IndexOf('{')
    if ($pos -lt 0) { throw ('检查器无 JSON 输出：' + $text.Substring(0, [Math]::Min(400, $text.Length))) }
    return ($text.Substring($pos) | ConvertFrom-Json)
}

function Get-Status {
    param($Result, [string]$Id)
    $c = @($Result.checks | Where-Object { $_.id -eq $Id })
    if ($c.Count -eq 0) { return '(缺失)' }
    return $c[0].status
}

function Get-Details {
    param($Result, [string]$Id)
    $c = @($Result.checks | Where-Object { $_.id -eq $Id })
    if ($c.Count -eq 0) { return '' }
    return (@($c[0].details) -join "`n")
}

$script:failures = 0
function Assert-Case {
    param([string]$Name, [bool]$Ok, [string]$Note)
    if ($Ok) { Write-Output ('[PASS] ' + $Name + $(if ($Note) { '：' + $Note } else { '' })) }
    else { Write-Output ('[FAIL] ' + $Name + $(if ($Note) { '：' + $Note } else { '' })); $script:failures++ }
}

try {
    # 1 基线夹具：锚点 8.10.2（Form XObject，含 /Matrix）→ 全部 PASS/SKIP
    Write-Fixture -Anchor '8.10.2' -Status $S_OK -Test 'form_matrix_roundtrip' -Source $sourceTemplate -MatrixHeading '# 第 8 章 · 表单（8.10.2）'
    $r = Invoke-SpecCheck
    $bad = @($r.checks | Where-Object { $_.status -eq 'FAIL' } | ForEach-Object { $_.id })
    Assert-Case -Name 'A 基线夹具全绿' -Ok ($bad.Count -eq 0) -Note ('FAIL=' + ($(if ($bad.Count) { $bad -join ',' } else { '无' })) + '，S4=' + (Get-Status $r 'S4'))

    # 2 把锚点换到图像字典 8.9.2–8.9.4：/Matrix 不在其中 → S4 FAIL
    Write-Fixture -Anchor '8.9.2–8.9.4' -Status $S_OK -Test 'form_matrix_roundtrip' -Source $sourceTemplate -MatrixHeading '# 第 8 章 · 图像（8.9.2–8.9.4）'
    $r = Invoke-SpecCheck
    $s4 = Get-Status $r 'S4'
    $s4d = Get-Details $r 'S4'
    Assert-Case -Name 'B 锚点到 8.9.2–8.9.4 判 /Matrix 为伪造' -Ok ($s4 -eq 'FAIL' -and $s4d -match '/Matrix') -Note ('S4=' + $s4 + '；S5=' + (Get-Status $r 'S5') + '；S3=' + (Get-Status $r 'S3'))

    # 3 换回 8.10.2：同一文本不再失败，证明按锚点溯源而非关键词黑名单
    Write-Fixture -Anchor '8.10.2' -Status $S_OK -Test 'form_matrix_roundtrip' -Source $sourceTemplate -MatrixHeading '# 第 8 章 · 表单（8.10.2）'
    $r = Invoke-SpecCheck
    Assert-Case -Name 'C 改锚 8.10.2 后 /Matrix 通过' -Ok ((Get-Status $r 'S4') -eq 'PASS') -Note ('S4=' + (Get-Status $r 'S4'))

    # 4 注入不存在的锚点 8.99.99 → S3 FAIL
    Write-Fixture -Anchor '8.99.99' -Status $S_OK -Test 'form_matrix_roundtrip' -Source $sourceTemplate -MatrixHeading '# 第 8 章 · 伪锚点'
    $r = Invoke-SpecCheck
    Assert-Case -Name 'D 不存在的锚点 8.99.99' -Ok ((Get-Status $r 'S3') -eq 'FAIL') -Note ('S3=' + (Get-Status $r 'S3'))

    # 5 注入已改名的测试名 → S7 FAIL
    Write-Fixture -Anchor '8.10.2' -Status $S_OK -Test 'form_matrix_roundtrip_renamed' -Source $sourceTemplate -MatrixHeading '# 第 8 章 · 表单（8.10.2）'
    $r = Invoke-SpecCheck
    $s7d = Get-Details $r 'S7'
    Assert-Case -Name 'E 改名的测试名' -Ok ((Get-Status $r 'S7') -eq 'FAIL' -and $s7d -match 'form_matrix_roundtrip_renamed') -Note ('S7=' + (Get-Status $r 'S7'))

    # 6 还原最小文档树 → 再次全绿
    Write-Fixture -Anchor '8.10.2' -Status $S_OK -Test 'form_matrix_roundtrip' -Source $sourceTemplate -MatrixHeading '# 第 8 章 · 表单（8.10.2）'
    $r = Invoke-SpecCheck
    $bad = @($r.checks | Where-Object { $_.status -eq 'FAIL' } | ForEach-Object { $_.id })
    Assert-Case -Name 'F 还原后全绿' -Ok ($bad.Count -eq 0) -Note ('FAIL=' + ($(if ($bad.Count) { $bad -join ',' } else { '无' })))
    Assert-Case -Name 'F2 基线夹具无非规范条目' -Ok ($r.nonSpec.count -eq 0) -Note ('非规范=' + $r.nonSpec.count)

    # 7 合法非规范标记 + 无锚点 → S3 PASS，并进入非规范清单
    Write-Fixture -NoAnchor -Status $S_OK -Test 'form_matrix_roundtrip' -Source $sourceTemplate -MatrixHeading '# 第 8 章 · 表单（8.10.2）' -Marker '（非规范：工程决策）'
    $r = Invoke-SpecCheck
    $s3 = Get-Status $r 'S3'
    $listed = @($r.nonSpec.items | Where-Object { $_.unit -eq 'A1' }).Count
    Assert-Case -Name 'G 合法标记且无锚点' -Ok ($s3 -eq 'PASS' -and $listed -eq 1) -Note ('S3=' + $s3 + '；清单=' + $listed + '；上限=' + $r.nonSpec.limit)

    # 8 反滥用：带标记但锚点可解析 → S3 FAIL
    Write-Fixture -Anchor '8.10.2' -Status $S_OK -Test 'form_matrix_roundtrip' -Source $sourceTemplate -MatrixHeading '# 第 8 章 · 表单（8.10.2）' -Marker '（非规范：工程决策）'
    $r = Invoke-SpecCheck
    $s3d = Get-Details $r 'S3'
    Assert-Case -Name 'H 带标记但锚点可解析' -Ok ((Get-Status $r 'S3') -eq 'FAIL' -and $s3d -match '滥用') -Note ('S3=' + (Get-Status $r 'S3'))

    # 9 空标记（只有「非规范：」）→ S3 FAIL
    Write-Fixture -NoAnchor -Status $S_OK -Test 'form_matrix_roundtrip' -Source $sourceTemplate -MatrixHeading '# 第 8 章 · 表单（8.10.2）' -Marker '（非规范：）'
    $r = Invoke-SpecCheck
    Assert-Case -Name 'I 空标记缺少依据' -Ok ((Get-Status $r 'S3') -eq 'FAIL' -and (Get-Details $r 'S3') -match '缺少依据') -Note ('S3=' + (Get-Status $r 'S3'))

    # 10 无标记无锚点 → 仍 FAIL
    Write-Fixture -NoAnchor -Status $S_OK -Test 'form_matrix_roundtrip' -Source $sourceTemplate -MatrixHeading '# 第 8 章 · 表单（8.10.2）'
    $r = Invoke-SpecCheck
    Assert-Case -Name 'J 无标记无锚点' -Ok ((Get-Status $r 'S3') -eq 'FAIL' -and (Get-Details $r 'S3') -match '无规范锚点') -Note ('S3=' + (Get-Status $r 'S3'))

    # 11 矩阵行持非规范标记 → S3 FAIL
    Write-Fixture -Anchor '8.10.2' -Status $S_OK -Test 'form_matrix_roundtrip' -Source $sourceTemplate -MatrixHeading '# 第 8 章 · 表单（8.10.2）' -MatrixMarker '（非规范：工程决策）'
    $r = Invoke-SpecCheck
    Assert-Case -Name 'K 矩阵行不允许非规范标记' -Ok ((Get-Status $r 'S3') -eq 'FAIL' -and (Get-Details $r 'S3') -match '矩阵行不允许') -Note ('S3=' + (Get-Status $r 'S3'))
}
finally {
    Remove-Item -LiteralPath $sandbox -Recurse -Force -ErrorAction SilentlyContinue
}

if ($script:failures -gt 0) {
    Write-Output ('自测失败用例：' + $script:failures)
    exit 1
}
Write-Output '自测全部通过'
exit 0
