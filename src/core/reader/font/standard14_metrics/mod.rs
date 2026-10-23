use std::sync::OnceLock;

const METRIC_TABLE_COUNT: usize = 10;
const METRIC_COLUMN_COUNT: usize = METRIC_TABLE_COUNT + 1;
const METRIC_HEADER: &str = "code,helvetica,helvetica_bold,helvetica_oblique,helvetica_bold_oblique,times_roman,times_bold,times_italic,times_bold_italic,symbol,zapf_dingbats";

type MetricTables = [[Option<u16>; 256]; METRIC_TABLE_COUNT];

static METRICS: OnceLock<MetricTables> = OnceLock::new();

fn metrics() -> &'static MetricTables {
    METRICS.get_or_init(|| {
        parse_metrics(include_str!("../standard14_metrics.csv"))
            .unwrap_or_else(|error| panic!("invalid embedded Standard 14 metrics: {error}"))
    })
}

fn parse_metrics(data: &str) -> Result<MetricTables, String> {
    let mut lines = data
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    let header = lines
        .next()
        .ok_or_else(|| "missing CSV header".to_string())?;
    if header != METRIC_HEADER {
        return Err("unexpected CSV header".to_string());
    }

    let mut tables = [[None; 256]; METRIC_TABLE_COUNT];
    let mut row_count = 0;
    for (expected_code, line) in lines.enumerate() {
        if expected_code >= 256 {
            return Err("more than 256 code rows".to_string());
        }
        let mut columns = line.split(',');
        let code = columns
            .next()
            .ok_or_else(|| format!("row {expected_code} has no code"))?
            .parse::<usize>()
            .map_err(|_| format!("row {expected_code} has an invalid code"))?;
        if code != expected_code {
            return Err(format!("expected code {expected_code}, found {code}"));
        }
        for (column, value) in columns.enumerate() {
            if column >= METRIC_TABLE_COUNT {
                return Err(format!("row {code} has too many columns"));
            }
            tables[column][code] = if value == "-" {
                None
            } else {
                Some(
                    value
                        .parse::<u16>()
                        .map_err(|_| format!("invalid width in row {code}, column {column}"))?,
                )
            };
        }
        if columns_count(line) != METRIC_COLUMN_COUNT {
            return Err(format!("row {code} has an unexpected column count"));
        }
        row_count += 1;
    }
    if row_count != 256 {
        return Err("expected exactly 256 code rows".to_string());
    }
    Ok(tables)
}

fn columns_count(line: &str) -> usize {
    line.split(',').count()
}

fn width(table: usize, code: u8) -> Option<f64> {
    metrics()[table][usize::from(code)].map(f64::from)
}

pub(super) fn helvetica_width(code: u8, bold: bool, oblique: bool) -> Option<f64> {
    let table = match (bold, oblique) {
        (false, false) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (true, true) => 3,
    };
    width(table, code)
}

pub(super) fn times_width(code: u8, bold: bool, italic: bool) -> Option<f64> {
    let table = match (bold, italic) {
        (false, false) => 4,
        (true, false) => 5,
        (false, true) => 6,
        (true, true) => 7,
    };
    width(table, code)
}

pub(super) fn symbol_width(code: u8) -> Option<f64> {
    width(8, code)
}

pub(super) fn zapf_dingbats_width(code: u8) -> Option<f64> {
    width(9, code)
}

#[cfg(test)]
mod tests {
    use super::{METRIC_HEADER, METRIC_TABLE_COUNT, parse_metrics};

    #[test]
    fn adobe_afm_tables_have_expected_coverage_and_widths() {
        let tables =
            parse_metrics(include_str!("../standard14_metrics.csv")).expect("valid embedded data");
        assert_eq!(
            tables
                .iter()
                .take(4)
                .map(|table| table.iter().flatten().count())
                .collect::<Vec<_>>(),
            [149, 149, 149, 149]
        );
        assert_eq!(
            tables
                .iter()
                .skip(4)
                .take(4)
                .map(|table| table.iter().flatten().count())
                .collect::<Vec<_>>(),
            [149, 149, 149, 149]
        );
        assert_eq!(tables[8].iter().flatten().count(), 189);
        assert_eq!(tables[9].iter().flatten().count(), 202);
        assert_eq!(tables[0][32], Some(278));
        assert_eq!(tables[1][65], Some(722));
        assert_eq!(tables[4][32], Some(250));
        assert_eq!(tables[6][65], Some(611));
        assert_eq!(tables[8][0xA0], Some(750));
        assert_eq!(tables[9][0xFE], Some(918));
        assert!(tables.iter().all(|table| table[0].is_none()));
        assert_eq!(tables.len(), METRIC_TABLE_COUNT);
    }

    #[test]
    fn metrics_parser_rejects_missing_and_malformed_rows() {
        let empty_row = "0,-,-,-,-,-,-,-,-,-,-";
        assert!(parse_metrics(&format!("{METRIC_HEADER}\n{empty_row}\n")).is_err());
        assert!(
            parse_metrics(&format!(
                "{METRIC_HEADER}\n{empty_row}\n2,-,-,-,-,-,-,-,-,-,-\n"
            ))
            .is_err()
        );
        assert!(
            parse_metrics(&format!(
                "{METRIC_HEADER}\n0,not-a-width,-,-,-,-,-,-,-,-,-\n"
            ))
            .is_err()
        );
        assert!(parse_metrics(&format!("{METRIC_HEADER}\n0,-\n")).is_err());
    }
}
