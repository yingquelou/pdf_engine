//! vendored CMap 原文清单（数据文件与名称一一对应）。
//!
//! 顺序约束：H 变体必须排在对应 V 变体之前，V 的 `usecmap` 才能从已解析
//! 结果中找到基座；`ETenms-B5-*` 依赖 `ETen-B5-H`，`ETenms-B5-V` 依赖
//! `ETenms-B5-H`。数据内容与 `cmap_resources/README.txt` 的 sha256 一致。
pub(super) const VENDORED_SOURCES: &[(&str, &str)] = &[
    ("GB-EUC-H", include_str!("../cmap_resources/GB-EUC-H")),
    ("GB-EUC-V", include_str!("../cmap_resources/GB-EUC-V")),
    ("GBpc-EUC-H", include_str!("../cmap_resources/GBpc-EUC-H")),
    ("GBpc-EUC-V", include_str!("../cmap_resources/GBpc-EUC-V")),
    ("GB-H", include_str!("../cmap_resources/GB-H")),
    ("GB-V", include_str!("../cmap_resources/GB-V")),
    ("GBK-EUC-H", include_str!("../cmap_resources/GBK-EUC-H")),
    ("GBK-EUC-V", include_str!("../cmap_resources/GBK-EUC-V")),
    ("GBKp-EUC-H", include_str!("../cmap_resources/GBKp-EUC-H")),
    ("GBKp-EUC-V", include_str!("../cmap_resources/GBKp-EUC-V")),
    ("GBT-H", include_str!("../cmap_resources/GBT-H")),
    ("GBT-V", include_str!("../cmap_resources/GBT-V")),
    ("GBT-EUC-H", include_str!("../cmap_resources/GBT-EUC-H")),
    ("GBT-EUC-V", include_str!("../cmap_resources/GBT-EUC-V")),
    ("GBTpc-EUC-H", include_str!("../cmap_resources/GBTpc-EUC-H")),
    ("GBTpc-EUC-V", include_str!("../cmap_resources/GBTpc-EUC-V")),
    ("GBK2K-H", include_str!("../cmap_resources/GBK2K-H")),
    ("GBK2K-V", include_str!("../cmap_resources/GBK2K-V")),
    (
        "UniGB-UCS2-H",
        include_str!("../cmap_resources/UniGB-UCS2-H"),
    ),
    (
        "UniGB-UCS2-V",
        include_str!("../cmap_resources/UniGB-UCS2-V"),
    ),
    (
        "UniGB-UTF16-H",
        include_str!("../cmap_resources/UniGB-UTF16-H"),
    ),
    (
        "UniGB-UTF16-V",
        include_str!("../cmap_resources/UniGB-UTF16-V"),
    ),
    (
        "UniGB-UTF32-H",
        include_str!("../cmap_resources/UniGB-UTF32-H"),
    ),
    (
        "UniGB-UTF32-V",
        include_str!("../cmap_resources/UniGB-UTF32-V"),
    ),
    (
        "UniGB-UTF8-H",
        include_str!("../cmap_resources/UniGB-UTF8-H"),
    ),
    (
        "UniGB-UTF8-V",
        include_str!("../cmap_resources/UniGB-UTF8-V"),
    ),
    ("B5-H", include_str!("../cmap_resources/B5-H")),
    ("B5-V", include_str!("../cmap_resources/B5-V")),
    ("B5pc-H", include_str!("../cmap_resources/B5pc-H")),
    ("B5pc-V", include_str!("../cmap_resources/B5pc-V")),
    ("CNS-EUC-H", include_str!("../cmap_resources/CNS-EUC-H")),
    ("CNS-EUC-V", include_str!("../cmap_resources/CNS-EUC-V")),
    ("CNS1-H", include_str!("../cmap_resources/CNS1-H")),
    ("CNS1-V", include_str!("../cmap_resources/CNS1-V")),
    ("CNS2-H", include_str!("../cmap_resources/CNS2-H")),
    ("CNS2-V", include_str!("../cmap_resources/CNS2-V")),
    ("ETen-B5-H", include_str!("../cmap_resources/ETen-B5-H")),
    ("ETen-B5-V", include_str!("../cmap_resources/ETen-B5-V")),
    ("ETenms-B5-H", include_str!("../cmap_resources/ETenms-B5-H")),
    ("ETenms-B5-V", include_str!("../cmap_resources/ETenms-B5-V")),
    ("ETHK-B5-H", include_str!("../cmap_resources/ETHK-B5-H")),
    ("ETHK-B5-V", include_str!("../cmap_resources/ETHK-B5-V")),
    ("HKscs-B5-H", include_str!("../cmap_resources/HKscs-B5-H")),
    ("HKscs-B5-V", include_str!("../cmap_resources/HKscs-B5-V")),
    ("HKdla-B5-H", include_str!("../cmap_resources/HKdla-B5-H")),
    ("HKdla-B5-V", include_str!("../cmap_resources/HKdla-B5-V")),
    ("HKdlb-B5-H", include_str!("../cmap_resources/HKdlb-B5-H")),
    ("HKdlb-B5-V", include_str!("../cmap_resources/HKdlb-B5-V")),
    ("HKgccs-B5-H", include_str!("../cmap_resources/HKgccs-B5-H")),
    ("HKgccs-B5-V", include_str!("../cmap_resources/HKgccs-B5-V")),
    ("HKm314-B5-H", include_str!("../cmap_resources/HKm314-B5-H")),
    ("HKm314-B5-V", include_str!("../cmap_resources/HKm314-B5-V")),
    ("HKm471-B5-H", include_str!("../cmap_resources/HKm471-B5-H")),
    ("HKm471-B5-V", include_str!("../cmap_resources/HKm471-B5-V")),
    (
        "Adobe-CNS1-0",
        include_str!("../cmap_resources/Adobe-CNS1-0"),
    ),
    (
        "Adobe-CNS1-1",
        include_str!("../cmap_resources/Adobe-CNS1-1"),
    ),
    (
        "Adobe-CNS1-2",
        include_str!("../cmap_resources/Adobe-CNS1-2"),
    ),
    (
        "Adobe-CNS1-3",
        include_str!("../cmap_resources/Adobe-CNS1-3"),
    ),
    (
        "Adobe-CNS1-4",
        include_str!("../cmap_resources/Adobe-CNS1-4"),
    ),
    (
        "Adobe-CNS1-5",
        include_str!("../cmap_resources/Adobe-CNS1-5"),
    ),
    (
        "Adobe-CNS1-6",
        include_str!("../cmap_resources/Adobe-CNS1-6"),
    ),
    (
        "Adobe-CNS1-7",
        include_str!("../cmap_resources/Adobe-CNS1-7"),
    ),
    (
        "UniCNS-UCS2-H",
        include_str!("../cmap_resources/UniCNS-UCS2-H"),
    ),
    (
        "UniCNS-UCS2-V",
        include_str!("../cmap_resources/UniCNS-UCS2-V"),
    ),
    (
        "UniCNS-UTF16-H",
        include_str!("../cmap_resources/UniCNS-UTF16-H"),
    ),
    (
        "UniCNS-UTF16-V",
        include_str!("../cmap_resources/UniCNS-UTF16-V"),
    ),
    (
        "UniCNS-UTF32-H",
        include_str!("../cmap_resources/UniCNS-UTF32-H"),
    ),
    (
        "UniCNS-UTF32-V",
        include_str!("../cmap_resources/UniCNS-UTF32-V"),
    ),
    (
        "UniCNS-UTF8-H",
        include_str!("../cmap_resources/UniCNS-UTF8-H"),
    ),
    (
        "UniCNS-UTF8-V",
        include_str!("../cmap_resources/UniCNS-UTF8-V"),
    ),
    ("78-EUC-H", include_str!("../cmap_resources/78-EUC-H")),
    ("78-EUC-V", include_str!("../cmap_resources/78-EUC-V")),
    ("78-H", include_str!("../cmap_resources/78-H")),
    ("78-V", include_str!("../cmap_resources/78-V")),
    ("78-RKSJ-H", include_str!("../cmap_resources/78-RKSJ-H")),
    ("78-RKSJ-V", include_str!("../cmap_resources/78-RKSJ-V")),
    ("78ms-RKSJ-H", include_str!("../cmap_resources/78ms-RKSJ-H")),
    ("78ms-RKSJ-V", include_str!("../cmap_resources/78ms-RKSJ-V")),
    ("83pv-RKSJ-H", include_str!("../cmap_resources/83pv-RKSJ-H")),
    ("90ms-RKSJ-H", include_str!("../cmap_resources/90ms-RKSJ-H")),
    ("90ms-RKSJ-V", include_str!("../cmap_resources/90ms-RKSJ-V")),
    (
        "90msp-RKSJ-H",
        include_str!("../cmap_resources/90msp-RKSJ-H"),
    ),
    (
        "90msp-RKSJ-V",
        include_str!("../cmap_resources/90msp-RKSJ-V"),
    ),
    ("90pv-RKSJ-H", include_str!("../cmap_resources/90pv-RKSJ-H")),
    ("90pv-RKSJ-V", include_str!("../cmap_resources/90pv-RKSJ-V")),
    ("Add-H", include_str!("../cmap_resources/Add-H")),
    ("Add-V", include_str!("../cmap_resources/Add-V")),
    ("Add-RKSJ-H", include_str!("../cmap_resources/Add-RKSJ-H")),
    ("Add-RKSJ-V", include_str!("../cmap_resources/Add-RKSJ-V")),
    ("EUC-H", include_str!("../cmap_resources/EUC-H")),
    ("EUC-V", include_str!("../cmap_resources/EUC-V")),
    ("Ext-H", include_str!("../cmap_resources/Ext-H")),
    ("Ext-V", include_str!("../cmap_resources/Ext-V")),
    ("Ext-RKSJ-H", include_str!("../cmap_resources/Ext-RKSJ-H")),
    ("Ext-RKSJ-V", include_str!("../cmap_resources/Ext-RKSJ-V")),
    ("H", include_str!("../cmap_resources/H")),
    ("V", include_str!("../cmap_resources/V")),
    ("Hankaku", include_str!("../cmap_resources/Hankaku")),
    ("Hiragana", include_str!("../cmap_resources/Hiragana")),
    ("Katakana", include_str!("../cmap_resources/Katakana")),
    ("Roman", include_str!("../cmap_resources/Roman")),
    ("NWP-H", include_str!("../cmap_resources/NWP-H")),
    ("NWP-V", include_str!("../cmap_resources/NWP-V")),
    ("RKSJ-H", include_str!("../cmap_resources/RKSJ-H")),
    ("RKSJ-V", include_str!("../cmap_resources/RKSJ-V")),
    ("WP-Symbol", include_str!("../cmap_resources/WP-Symbol")),
    (
        "Adobe-Japan1-0",
        include_str!("../cmap_resources/Adobe-Japan1-0"),
    ),
    (
        "Adobe-Japan1-1",
        include_str!("../cmap_resources/Adobe-Japan1-1"),
    ),
    (
        "Adobe-Japan1-2",
        include_str!("../cmap_resources/Adobe-Japan1-2"),
    ),
    (
        "Adobe-Japan1-3",
        include_str!("../cmap_resources/Adobe-Japan1-3"),
    ),
    (
        "Adobe-Japan1-4",
        include_str!("../cmap_resources/Adobe-Japan1-4"),
    ),
    (
        "Adobe-Japan1-5",
        include_str!("../cmap_resources/Adobe-Japan1-5"),
    ),
    (
        "Adobe-Japan1-6",
        include_str!("../cmap_resources/Adobe-Japan1-6"),
    ),
    (
        "Adobe-Japan1-7",
        include_str!("../cmap_resources/Adobe-Japan1-7"),
    ),
    (
        "UniJIS-UCS2-H",
        include_str!("../cmap_resources/UniJIS-UCS2-H"),
    ),
    (
        "UniJIS-UCS2-HW-H",
        include_str!("../cmap_resources/UniJIS-UCS2-HW-H"),
    ),
    (
        "UniJIS-UCS2-HW-V",
        include_str!("../cmap_resources/UniJIS-UCS2-HW-V"),
    ),
    (
        "UniJIS-UCS2-V",
        include_str!("../cmap_resources/UniJIS-UCS2-V"),
    ),
    (
        "UniJIS-UTF16-H",
        include_str!("../cmap_resources/UniJIS-UTF16-H"),
    ),
    (
        "UniJIS-UTF16-V",
        include_str!("../cmap_resources/UniJIS-UTF16-V"),
    ),
    (
        "UniJIS-UTF32-H",
        include_str!("../cmap_resources/UniJIS-UTF32-H"),
    ),
    (
        "UniJIS-UTF32-V",
        include_str!("../cmap_resources/UniJIS-UTF32-V"),
    ),
    (
        "UniJIS-UTF8-H",
        include_str!("../cmap_resources/UniJIS-UTF8-H"),
    ),
    (
        "UniJIS-UTF8-V",
        include_str!("../cmap_resources/UniJIS-UTF8-V"),
    ),
    (
        "UniJIS2004-UTF16-H",
        include_str!("../cmap_resources/UniJIS2004-UTF16-H"),
    ),
    (
        "UniJIS2004-UTF16-V",
        include_str!("../cmap_resources/UniJIS2004-UTF16-V"),
    ),
    (
        "UniJIS2004-UTF32-H",
        include_str!("../cmap_resources/UniJIS2004-UTF32-H"),
    ),
    (
        "UniJIS2004-UTF32-V",
        include_str!("../cmap_resources/UniJIS2004-UTF32-V"),
    ),
    (
        "UniJIS2004-UTF8-H",
        include_str!("../cmap_resources/UniJIS2004-UTF8-H"),
    ),
    (
        "UniJIS2004-UTF8-V",
        include_str!("../cmap_resources/UniJIS2004-UTF8-V"),
    ),
    (
        "UniJISPro-UCS2-HW-V",
        include_str!("../cmap_resources/UniJISPro-UCS2-HW-V"),
    ),
    (
        "UniJISPro-UCS2-V",
        include_str!("../cmap_resources/UniJISPro-UCS2-V"),
    ),
    (
        "UniJISPro-UTF8-V",
        include_str!("../cmap_resources/UniJISPro-UTF8-V"),
    ),
    (
        "UniJISX0213-UTF32-H",
        include_str!("../cmap_resources/UniJISX0213-UTF32-H"),
    ),
    (
        "UniJISX0213-UTF32-V",
        include_str!("../cmap_resources/UniJISX0213-UTF32-V"),
    ),
    (
        "UniJISX02132004-UTF32-H",
        include_str!("../cmap_resources/UniJISX02132004-UTF32-H"),
    ),
    (
        "UniJISX02132004-UTF32-V",
        include_str!("../cmap_resources/UniJISX02132004-UTF32-V"),
    ),
    ("KSC-EUC-H", include_str!("../cmap_resources/KSC-EUC-H")),
    ("KSC-EUC-V", include_str!("../cmap_resources/KSC-EUC-V")),
    ("KSC-H", include_str!("../cmap_resources/KSC-H")),
    ("KSC-V", include_str!("../cmap_resources/KSC-V")),
    ("KSC-Johab-H", include_str!("../cmap_resources/KSC-Johab-H")),
    ("KSC-Johab-V", include_str!("../cmap_resources/KSC-Johab-V")),
    ("KSCms-UHC-H", include_str!("../cmap_resources/KSCms-UHC-H")),
    ("KSCms-UHC-V", include_str!("../cmap_resources/KSCms-UHC-V")),
    (
        "KSCms-UHC-HW-H",
        include_str!("../cmap_resources/KSCms-UHC-HW-H"),
    ),
    (
        "KSCms-UHC-HW-V",
        include_str!("../cmap_resources/KSCms-UHC-HW-V"),
    ),
    ("KSCpc-EUC-H", include_str!("../cmap_resources/KSCpc-EUC-H")),
    ("KSCpc-EUC-V", include_str!("../cmap_resources/KSCpc-EUC-V")),
    (
        "Adobe-Korea1-0",
        include_str!("../cmap_resources/Adobe-Korea1-0"),
    ),
    (
        "Adobe-Korea1-1",
        include_str!("../cmap_resources/Adobe-Korea1-1"),
    ),
    (
        "Adobe-Korea1-2",
        include_str!("../cmap_resources/Adobe-Korea1-2"),
    ),
    (
        "UniKS-UCS2-H",
        include_str!("../cmap_resources/UniKS-UCS2-H"),
    ),
    (
        "UniKS-UCS2-V",
        include_str!("../cmap_resources/UniKS-UCS2-V"),
    ),
    (
        "UniKS-UTF16-H",
        include_str!("../cmap_resources/UniKS-UTF16-H"),
    ),
    (
        "UniKS-UTF16-V",
        include_str!("../cmap_resources/UniKS-UTF16-V"),
    ),
    (
        "UniKS-UTF32-H",
        include_str!("../cmap_resources/UniKS-UTF32-H"),
    ),
    (
        "UniKS-UTF32-V",
        include_str!("../cmap_resources/UniKS-UTF32-V"),
    ),
    (
        "UniKS-UTF8-H",
        include_str!("../cmap_resources/UniKS-UTF8-H"),
    ),
    (
        "UniKS-UTF8-V",
        include_str!("../cmap_resources/UniKS-UTF8-V"),
    ),
];
