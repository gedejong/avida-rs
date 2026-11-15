//! Reading Avida data files (`tasks.dat`, `count.dat`, ...): `#` comment
//! lines, of which `#  3: Name` lines name the columns, then rows of
//! whitespace-separated values.

/// A parsed data file.
#[derive(Clone, Debug, Default)]
pub struct Dat {
    /// Column titles from the header, by position (an empty string for a
    /// column the header does not name).
    pub columns: Vec<String>,
    /// The rows: the file's line number and one cell per value, `None`
    /// for a token that is not a number.
    pub rows: Vec<(usize, Vec<Option<f64>>)>,
    /// The last line was cut short and left out (a file still being
    /// written).
    pub cut_tail: bool,
}

/// `#  3: Name` gives (3, "Name").
fn header_column(line: &str) -> Option<(usize, &str)> {
    let rest = line.trim_start_matches('#').trim_start();
    let (num, name) = rest.split_once(':')?;
    let k: usize = num.trim().parse().ok().filter(|&k| k >= 1)?;
    Some((k, name.trim()))
}

/// Parses the text of a data file. A row with a different number of values
/// than the header lists is an error, except the last line of the file,
/// which a run that is writing it may have cut short.
pub fn parse(text: &str) -> Result<Dat, String> {
    let mut dat = Dat::default();
    let lines: Vec<&str> = text.lines().collect();
    let last_data = lines
        .iter()
        .rposition(|l| !l.trim().is_empty() && !l.starts_with('#'));
    for (i, line) in lines.iter().enumerate() {
        let line_no = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            if let Some((k, name)) = header_column(trimmed) {
                if dat.columns.len() < k {
                    dat.columns.resize(k, String::new());
                }
                dat.columns[k - 1] = name.to_string();
            }
            continue;
        }
        let cells: Vec<Option<f64>> = trimmed
            .split_whitespace()
            .map(|t| t.parse::<f64>().ok())
            .collect();
        let want = dat.columns.len();
        if want > 0 && cells.len() != want {
            if Some(i) == last_data && cells.len() < want {
                dat.cut_tail = true;
                continue;
            }
            return Err(format!(
                "line {line_no} has {} values where the header lists {want} columns",
                cells.len()
            ));
        }
        dat.rows.push((line_no, cells));
    }
    Ok(dat)
}

impl Dat {
    /// The position of the first column whose title, lower-cased, starts
    /// with `prefix` (lower case).
    pub fn find(&self, prefix: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|c| c.to_ascii_lowercase().starts_with(prefix))
    }

    /// Like [`Dat::find`], as an error naming the file when absent.
    pub fn need(&self, file: &str, prefix: &str) -> Result<usize, String> {
        self.find(prefix)
            .ok_or_else(|| format!("{file} has no column named \"{prefix}...\""))
    }

    /// Column `col` of every row as numbers; a token that is not a number
    /// is an error naming the line.
    pub fn numbers(&self, file: &str, col: usize) -> Result<Vec<f64>, String> {
        self.rows
            .iter()
            .map(|(line, cells)| {
                cells.get(col).copied().flatten().ok_or_else(|| {
                    format!(
                        "{file} line {line}: the value in column {} is not a number",
                        col + 1
                    )
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_come_from_the_header_and_rows_from_the_body() {
        let d = parse("# x\n#  1: Update\n#  2: Not\n\n0 3\n100 4\n").unwrap();
        assert_eq!(d.columns, ["Update", "Not"]);
        assert_eq!(d.rows.len(), 2);
        assert_eq!(d.numbers("t.dat", 1).unwrap(), [3.0, 4.0]);
        assert_eq!(d.find("not"), Some(1));
        assert!(d.need("t.dat", "nand").is_err());
    }

    #[test]
    fn a_cut_last_line_is_dropped_but_a_cut_middle_line_is_an_error() {
        let d = parse("#  1: A\n#  2: B\n1 2\n3").unwrap();
        assert!(d.cut_tail);
        assert_eq!(d.rows.len(), 1);
        let e = parse("#  1: A\n#  2: B\n1 2\n3\n4 5\n").unwrap_err();
        assert!(e.contains("line 4"), "{e}");
    }

    #[test]
    fn text_in_a_column_that_is_read_is_an_error_elsewhere_it_is_fine() {
        let d = parse("#  1: A\n#  2: Name\n1 abc\n").unwrap();
        assert!(d.numbers("t.dat", 0).is_ok());
        assert!(d.numbers("t.dat", 1).unwrap_err().contains("not a number"));
        let n = parse("#  1: A\n1\n").unwrap();
        assert_eq!(n.rows.len(), 1);
        let nan = parse("#  1: A\nnan\n").unwrap();
        assert!(nan.numbers("t.dat", 0).unwrap()[0].is_nan());
    }
}
