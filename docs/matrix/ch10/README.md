# 第 10 章 渲染（Rendering）
> 事实层｜职责：登记本章特性状态与证据。｜禁止：排期、阶段归属、测试计数、历史叙述。

> 本库为语法层库而非渲染器，本章整体不在当前范围；各条目作为远期评估保留。

| 章节 | 特性 | 引入版本 | 状态 | Reader | Writer | 优先级 | 证据/备注 |
|---|---|---|---|---|---|---|---|
| 10.1 | 本章范围/术语说明 | — | — | — | — | — | 无独立实现项 |
| 10.2 | CIE-Based 颜色 → Device 颜色转换 | 1.1（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（渲染器职责） |
| 10.3 | Device 颜色空间相互转换（Gray/RGB/CMYK） | 1.0–1.1（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（渲染器职责） |
| 10.4 | Transfer Functions（TR/TR2） | 1.1（待核） | 🟡 | 🟡 | 🟡 | P3 | 结构层建模（见 7.10/8.4.4）：写端 `/TR` 函数对象（Type 0/2/3/4）与 `/TR2` 字典原样透传、软蒙版 `/TR`；读端 `ExtGState.transfer_function`/`transfer2` 保留解析后的值、`Document::soft_mask` 类型化解析 `/TR`。测试 `exponential_transfer_function_is_written_as_direct_dictionary`、`transfer_function_2_dictionary_is_written_verbatim`、`soft_mask_writes_transfer_function_smask_in_data_and_matte`、`parses_soft_mask_transfer_function_smask_in_data_and_matte`、集成 `builder_transparency_extensions_roundtrip_through_parser`。缺口：不求值/应用传递函数到设备输出（渲染器职责） |
| 10.5 | Halftones（半调字典 Type 1/5/6/10/16、Spot Functions、阈值数组） | 1.2（待核） | 🟡 | 🟡 | 🟡 | P3 | 结构层建模（D6，见 8.4.4/8.4.5）：写端 `builder/types/halftone.rs`（包装字典 `/DefaultHalftoneType` + 按色料覆盖、类型 1 `/XFrequency`、类型 2 `/Frequency`/`/Width`/`/Uncompensated`，`/SpotFunction`/`/TransferFunction` 复用函数句柄池）；读端 `reader/graphics_state/halftone`（`Document::halftone` → `HalftoneModel::{Type1,Type2,Composite,Unmodeled}`，类型 3–5、色料数组与命名半色调保留写出值）。测试 `halftone_dictionary_is_written_with_resolved_functions`、`invalid_halftone_parameters_fail_the_build`、`parses_type1_halftone_dictionary`、`parses_type2_halftone_with_uncompensated_flag`、`type3_to_type5_and_named_halftones_are_unmodeled`、`parses_halftone_composite_dictionary`、集成 `builder_transparency_extensions_roundtrip_through_parser`。缺口：类型 3（阈值数组）/4（屏幕字典）/5（Spot Function only）与 `/HTPO` 不写不建模；半调加网语义属渲染器 |
| 10.6 | Scan Conversion 细节（flatness/smoothness/扫描转换规则/自动描边调整） | 1.0（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
