//! [`LinearizationIssue`] 的 `Display` 文案：每变体携带对应条款原文，与
//! [`super`] 的变体定义一一对偶（新增变体须在此补 `write!` 分支，否则 `match`
//! 不穷尽）。拆出仅为守 600 行/25600 字节文件上限。

use std::fmt;

use super::LinearizationIssue;

impl fmt::Display for LinearizationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequired(key) => {
                write!(f, "F.3.3 Table F.1: required entry /{key} is missing")
            }
            Self::WrongType {
                key,
                expected,
                found,
            } => write!(
                f,
                "Table F.1: entry /{key} shall be {expected}, found {found}"
            ),
            Self::IndirectValue(key) => write!(
                f,
                "F.3.3: all values in the linearization parameter dictionary shall be direct objects, /{key} is an indirect reference"
            ),
            Self::HintArrayLength { len } => {
                write!(
                    f,
                    "Table F.1: /H shall be an array of two or four integers, found {len} element(s)"
                )
            }
            Self::FileLengthMismatch { declared, actual } => write!(
                f,
                "Table F.1: /L {declared} is not exactly equal to the actual file length {actual}; the file shall be treated as ordinary PDF"
            ),
            Self::DictionaryBeyondFirst1024 { end_offset, limit } => write!(
                f,
                "F.3.3: the linearization parameter dictionary shall be entirely contained within the first {limit} bytes, it ends at offset {end_offset}"
            ),
            Self::NotFirstBodyObject { object_number } => write!(
                f,
                "F.3.3: the first object in the body of the file shall be the linearization parameter dictionary, object {object_number} precedes it"
            ),
            Self::ReferencedFrom { location, key } => write!(
                f,
                "F.3.3: there shall be no references to the linearization parameter dictionary, found one from /{key} of the {location}"
            ),
            Self::NotNormalEntry {
                role,
                object_number,
            } => write!(
                f,
                "F.3.3/F.3.4: the first-page cross-reference table shall contain a normal entry for the {role} (object {object_number})"
            ),
            Self::FirstPageTableFreeEntries { count } => write!(
                f,
                "F.3.4: the first-page cross-reference table shall consist of a single subsection with no free entries, found {count} free entries"
            ),
            Self::FirstPageTrailerEntry { key, problem } => write!(
                f,
                "F.3.4: the first-page trailer shall contain a valid /{key} entry, but {problem}"
            ),
            Self::TableEntryOrder {
                role,
                object_number,
            } => write!(
                f,
                "F.3.4: the first-page cross-reference table shall list the {role} (object {object_number}) at the expected position"
            ),
            Self::NonZeroGeneration { role, generation } => write!(
                f,
                "F.3.1: all objects shall have a generation number of 0, the {role} has {generation}"
            ),
            Self::ForbiddenInObjectStream { role } => write!(
                f,
                "F.3.1: the {role} may not be contained in an object stream"
            ),
            Self::CrossReferenceSections { count } => write!(
                f,
                "F.3.1: the two object groups shall be indexed by exactly two cross-reference table sections, found {count}"
            ),
            Self::ObjectNumberGroups {
                first_group_min,
                second_group_max,
            } => write!(
                f,
                "F.3.1: the first object group shall start at the first object number after the last number of the second group, found {first_group_min} after {second_group_max}"
            ),
            Self::FirstPageObject {
                object_number,
                problem,
            } => write!(
                f,
                "F.3.7: the first page's page object {object_number} is unusable: {problem}"
            ),
            Self::FirstPageMissingAttribute { object_number, key } => write!(
                f,
                "F.3.7: page object {object_number} shall explicitly specify /{key} instead of inheriting it"
            ),
            Self::PageCountMismatch { declared, actual } => write!(
                f,
                "Table F.1: /N {declared} does not match the {actual} page(s) of the document catalogue"
            ),
            Self::FirstPageNumberOutOfRange {
                page_number,
                page_count,
            } => write!(
                f,
                "Table F.1: /P {page_number} is out of range for a document of {page_count} page(s)"
            ),
            Self::OffsetBeyondFile {
                key,
                offset,
                file_length,
            } => write!(
                f,
                "Table F.1: /{key} offset {offset} is beyond the end of the file ({file_length} bytes)"
            ),
            Self::HintRangeBeyondFile {
                which,
                end,
                file_length,
            } => write!(
                f,
                "Table F.1: the {which} hint stream range ends at {end}, beyond the end of the file ({file_length} bytes)"
            ),
            Self::HintTargetNotStream { which, offset } => write!(
                f,
                "F.3.6: the {which} hint table shall be enclosed within a stream object, the object at offset {offset} is not a stream"
            ),
            Self::MainCrossReferenceOffset { offset, problem } => {
                write!(f, "Table F.1: /T offset {offset} is invalid: {problem}")
            }
            Self::MainCrossReferenceTable { problem } => {
                write!(
                    f,
                    "F.3.11: main cross-reference table is invalid: {problem}"
                )
            }
            Self::MainTrailerEntries { keys } => write!(
                f,
                "F.3.11: the main trailer shall not contain entries other than Size, found {}",
                keys.join(", ")
            ),
            Self::VersionBelowGate { version } => write!(
                f,
                "F.3.2: linearization may be applied to any PDF file of version 1.1 or greater, the file declares {version}"
            ),
            Self::HintStreamUnreadable { which, problem } => write!(
                f,
                "F.3.6: the {which} hint stream cannot supply the decoded bit stream for the F.4 tables: {problem}"
            ),
            Self::SharedHintTableMissing => write!(
                f,
                "F.3.6 Table F.2: the key S is Required, the primary hint stream dictionary shall give the position of the shared object hint table (F.4.2)"
            ),
            Self::HintStreamIndirectValue { which, key } => write!(
                f,
                "F.3.6: with one exception, the values of all entries in the {which} hint stream dictionary shall be direct objects and may contain no indirect object references (the exception is /Length); /{key} contains an indirect reference"
            ),
            Self::HintTablePositionInvalid { key, problem } => write!(
                f,
                "F.3.6: the hint stream dictionary entry /{key} gives a table position that is unusable: {problem}"
            ),
            Self::HintTableCorrupt { key, problem } => write!(
                f,
                "F.4.2 (Table F.5/F.6): the hint table {key} is corrupt: {problem}"
            ),
            Self::PageOffsetTableCorrupt { problem } => write!(
                f,
                "F.4.1 (Table F.3/F.4): the page offset hint table at stream offset 0 is corrupt: {problem}"
            ),
            Self::PageOffsetWidthOutOfRange { item, bits } => write!(
                f,
                "F.4.1: the bit width declared by Table F.3 {item} shall be in the range 0 through 32, found {bits}; the raw value is kept and the table is read as declared"
            ),
            Self::PageOffsetFirstPageSharedRefs { count } => write!(
                f,
                "Table F.4 item 3: for the first page the number of shared objects referenced shall be 0 (the next two items start with the second page), found {count}"
            ),
            Self::PageOffsetObjectNumberConflict {
                first_page_object,
                second_group_max,
            } => write!(
                f,
                "Table F.4 item 1: the first object of the second page shall have an object number of 1 and subsequent pages accumulate, so the pages from the second one use object numbers up to {second_group_max}; /O {first_page_object} does not start after them (F.3.1)"
            ),
            Self::PageOffsetSharedObjectIndexOutOfRange {
                page,
                index,
                entries,
            } => write!(
                f,
                "Table F.4 item 4: page {page} refers to shared object hint table entry {index}, but F.4.2 requires an index that is a sequential position in that table counting from 0 and the table holds {entries} entry(ies)"
            ),
            Self::PageOffsetSharedFractionDenominatorZero { references } => write!(
                f,
                "Table F.3 item 13: the denominator of the fractional position is 0 while the pages reference {references} shared object(s), so Table F.4 item 5 gives no decidable position class; the reader performs no division and the typed position degrades to None"
            ),
            Self::ThumbnailHintTableMissing => write!(
                f,
                "F.4.3: the primary hint stream dictionary declares the thumbnail hint table under key T, but Table F.7 item 3 declares 0 pages that have thumbnail images although the thumbnails section shall include one or more per-page entries (Table F.8)"
            ),
            Self::ThumbnailTableCorrupt { problem } => write!(
                f,
                "F.4.3 (Table F.7/F.8): the thumbnail hint table is corrupt: {problem}"
            ),
            Self::ThumbnailWidthOutOfRange { item, bits } => write!(
                f,
                "F.4.3: the bit width declared by Table F.7 {item} is expected to be in the range 0 through 32 (the wording of F.4.1 for Table F.3, applied by analogy), found {bits}; the raw value is kept and the table is read as declared"
            ),
            Self::GenericHintTableCorrupt { key, problem } => write!(
                f,
                "F.4.4 (Table F.9): the generic hint table for key {key} is corrupt: {problem}"
            ),
            Self::ExtendedHintTableCorrupt { key, problem } => write!(
                f,
                "F.4.5 (Table F.10): the extended generic hint table for key {key} is corrupt: {problem}"
            ),
            Self::ExtendedHintIdWidthExceedsDocument {
                key,
                group_bits,
                document_bits,
            } => write!(
                f,
                "F.4.5 (Table F.10 item 6) / F.4.1 (Table F.3 item 11): the extended generic hint table for key {key} declares {group_bits} bit(s) for the greatest shared object identifier of its group while the page offset hint table declares {document_bits} bit(s) for the whole document; the table is read with its own group width and the raw values are kept (derived consistency rule, not a literal shall)"
            ),
            Self::EmbeddedFileHintTableCorrupt { problem } => write!(
                f,
                "F.4.6 (Table F.11/F.12): the embedded file stream hint table for key B is corrupt: {problem}"
            ),
            Self::EmbeddedFileIdWidthUnavailable => write!(
                f,
                "F.4.6 (Table F.12 item 5): key B declares an embedded file stream hint table, but the document-level bit width from Table F.3 item 11 is unavailable because the page offset hint table is missing or corrupt; the table is not typed because item 5's identifiers would otherwise be fabricated as zeros"
            ),
            Self::EmbeddedFileGroupCountZero => write!(
                f,
                "F.4.6 (Table F.11 item 3): key B declares an embedded file stream hint table whose header declares 0 groups, although F.4.6 repeats every item of Table F.12 once per group and key B is present only if embedded file streams exist"
            ),
            Self::EmbeddedFileSharedOnlyGroupViolation {
                group,
                rule,
                object_count,
                group_length,
                shared_reference_count,
            } => write!(
                f,
                "F.4.6 (Table F.12): group {group} violates the rule \"{rule}\": item 2 = {object_count}, item 3 = {group_length}, item 4 = {shared_reference_count}; the raw values are kept and the table is still read (derived from the group convention in F.4.6)"
            ),
            Self::EmbeddedFileSharedObjectIndexOutOfRange {
                group,
                index,
                entries,
            } => write!(
                f,
                "F.4.6 (Table F.12 item 5): group {group} refers to shared object hint table entry {index}, but F.4.2 requires an index that is a sequential position in that table counting from 0 and the table holds {entries} entry(ies)"
            ),
            Self::EmbeddedFileFirstGroupAnchorMismatch {
                object_number,
                run_start,
                run_end,
            } => write!(
                f,
                "F.4.6 (Table F.11 items 1/3 + Table F.12 item 1): the first embedded file stream group's object number {object_number} lies outside the adjacent object number range {run_start}..={run_end} derived from Table F.11 item 1 and the group's object count, although F.4.6 requires all objects of a group to have adjacent object numbers (derived consistency rule, not a literal shall)"
            ),
        }
    }
}

impl std::error::Error for LinearizationIssue {}
